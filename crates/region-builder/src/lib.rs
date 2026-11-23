//! Builds offline region packages on the desktop: writes the definition of a
//! region chosen on the map, picks the OpenStreetMap extracts that cover it
//! and runs `scripts/build-region.sh`. Not used on mobile.
//!
//! Below the data directory of the app:
//!
//! ```text
//! region-defs/<id>.json         definitions of the regions built here
//! region-packages/<id>/         a finished package, until it is installed
//! region-build/<id>/build.log   output of the last build
//! ```
//!
//! Downloads and intermediate files stay in the data directory of the
//! repository (`data/`, or `OUTDOOR_DATA`), shared with the command line.

mod definition;
mod error;
mod extracts;
mod prereq;
mod run;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde::Serialize;

pub use definition::{
    RegionDefinition, Template, estimated_bytes, size_km, slug, unique_id, validate_bbox,
};
pub use error::BuildError;
pub use extracts::{Extract, GEOFABRIK_INDEX_URL, GeofabrikIndex};
pub use run::StepProgress;

/// The repository this program was built from; that is where the build script is.
const REPO_ENV: &str = "OUTDOOR_REPO";
/// Another script to run instead of `scripts/build-region.sh` (tests).
const SCRIPT_ENV: &str = "OUTDOOR_BUILD_SCRIPT";
/// Where the build script keeps downloads and intermediate files.
const DATA_ENV: &str = "OUTDOOR_DATA";
/// The index of extracts is a few megabytes.
const MAX_INDEX_LEN: u64 = 64 << 20;

/// What building an area would mean.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BuildPlan {
    pub width_km: f64,
    pub height_km: f64,
    /// Rough size of the finished package.
    pub estimated_bytes: u64,
    /// Addresses of the OpenStreetMap extracts that will be downloaded.
    pub extracts: Vec<String>,
}

/// Builds region packages with the build script of the repository.
pub struct Builder {
    app_data: PathBuf,
    repo: PathBuf,
    script: PathBuf,
    /// The programs the script of the repository needs are only checked for that script.
    check_tools: bool,
    index: Mutex<Option<Arc<GeofabrikIndex>>>,
}

impl Builder {
    /// A builder keeping its files below `app_data`. The repository is the one
    /// this program was compiled in unless `OUTDOOR_REPO` names another.
    pub fn new(app_data: &Path) -> Self {
        let repo = std::env::var_os(REPO_ENV).map_or_else(
            || Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."),
            PathBuf::from,
        );
        match std::env::var_os(SCRIPT_ENV) {
            Some(script) => Self::with_paths(app_data, &repo, Path::new(&script)),
            None => Builder {
                check_tools: true,
                ..Self::with_paths(app_data, &repo, &repo.join("scripts/build-region.sh"))
            },
        }
    }

    /// A builder running another script, which takes the arguments of
    /// `build-region.sh` and brings its own requirements.
    pub fn with_paths(app_data: &Path, repo: &Path, script: &Path) -> Self {
        Builder {
            app_data: app_data.to_path_buf(),
            repo: repo.to_path_buf(),
            script: script.to_path_buf(),
            check_tools: false,
            index: Mutex::default(),
        }
    }

    /// Uses this list of extracts instead of downloading it from Geofabrik.
    pub fn use_index(&self, json: &str) -> Result<(), BuildError> {
        let index = Arc::new(GeofabrikIndex::parse(json)?);
        *self.index.lock().unwrap_or_else(PoisonError::into_inner) = Some(index);
        Ok(())
    }

    /// Codes of what is missing for a build: `platform`, `repo`, the name of a
    /// program, `java_version` or `docker_daemon`. Empty if a build can start.
    pub fn check(&self) -> Vec<String> {
        prereq::missing(&self.script, self.check_tools)
    }

    /// Size and downloads of a region with these bounds (west, south, east,
    /// north). Downloads the list of extracts the first time.
    pub fn plan(&self, bbox: [f64; 4]) -> Result<BuildPlan, BuildError> {
        validate_bbox(bbox)?;
        let (width_km, height_km) = size_km(bbox);
        Ok(BuildPlan {
            width_km,
            height_km,
            estimated_bytes: estimated_bytes(bbox),
            extracts: self.extracts(bbox)?,
        })
    }

    /// Writes the definition of a new region. Its id is made from the name;
    /// ids for which `taken` holds, or that have a definition, are avoided.
    pub fn define(
        &self,
        name: &str,
        bbox: [f64; 4],
        taken: &dyn Fn(&str) -> bool,
    ) -> Result<RegionDefinition, BuildError> {
        validate_bbox(bbox)?;
        let osm = self.extracts(bbox)?;
        let name = match name.trim() {
            "" => "Region",
            name => name,
        };
        let mut slug = slug(name);
        // Ids ending in a template name would be checked against that template.
        if slug.ends_with("-urlaub") || slug.ends_with("-fliegen") {
            slug.push_str("-region");
        }
        let id = unique_id(&slug, &|id| taken(id) || self.definition_path(id).exists());
        let definition = RegionDefinition::new(&id, name, bbox, osm);
        self.write_definition(&definition)?;
        Ok(definition)
    }

    /// Builds the package of a defined region and returns its directory.
    /// Blocks until done; `on_step` hears of every step, setting `cancel`
    /// stops the build with [`BuildError::Cancelled`]. Steps that finished
    /// are not repeated by the next call, unless `refresh` asks for a build
    /// from fresh OpenStreetMap data.
    pub fn build(
        &self,
        id: &str,
        refresh: bool,
        on_step: &mut dyn FnMut(StepProgress),
        cancel: &AtomicBool,
    ) -> Result<PathBuf, BuildError> {
        let unknown = || BuildError::UnknownRegion(id.to_string());
        if !is_plain_name(id) {
            return Err(unknown());
        }
        let config = self.definition_path(id);
        let json = fs::read_to_string(&config).map_err(|_| unknown())?;
        let missing = self.check();
        if !missing.is_empty() {
            return Err(BuildError::Prerequisites(missing));
        }
        if refresh {
            // Geofabrik may have split or renamed its extracts since.
            let mut definition: RegionDefinition =
                serde_json::from_str(&json).map_err(|_| unknown())?;
            definition.osm = self.extracts(definition.bbox)?;
            self.write_definition(&definition)?;
        }

        let packages = self.app_data.join("region-packages");
        let log_dir = self.app_data.join("region-build").join(id);
        for dir in [&packages, &log_dir] {
            fs::create_dir_all(dir).map_err(BuildError::io(dir))?;
        }
        let mut command = Command::new("bash");
        command
            .arg(&self.script)
            .arg(id)
            .arg("--config")
            .arg(&config)
            .arg("--package-dir")
            .arg(&packages);
        if refresh {
            command.args(["--force", "--refresh-osm"]);
        }
        run::run(command, id, &log_dir.join("build.log"), on_step, cancel)?;
        Ok(packages.join(id))
    }

    /// Removes what a build of the region left behind: intermediate files, an
    /// uninstalled package, the log and, if asked, the definition. Never fails.
    pub fn cleanup(&self, id: &str, definition: bool) {
        if !is_plain_name(id) {
            return;
        }
        let data = std::env::var_os(DATA_ENV).map_or_else(|| self.repo.join("data"), PathBuf::from);
        for dir in [
            data.join("build").join(id),
            self.app_data.join("region-packages").join(id),
            self.app_data.join("region-build").join(id),
        ] {
            let _ = fs::remove_dir_all(dir);
        }
        if definition {
            let _ = fs::remove_file(self.definition_path(id));
        }
    }

    fn definition_path(&self, id: &str) -> PathBuf {
        self.app_data.join("region-defs").join(format!("{id}.json"))
    }

    fn write_definition(&self, definition: &RegionDefinition) -> Result<(), BuildError> {
        let path = self.definition_path(&definition.id);
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(BuildError::io(dir))?;
        }
        let json = serde_json::to_string_pretty(definition)
            .map_err(|e| BuildError::io(&path)(std::io::Error::other(e)))?;
        fs::write(&path, json + "\n").map_err(BuildError::io(&path))
    }

    fn extracts(&self, bbox: [f64; 4]) -> Result<Vec<String>, BuildError> {
        let extracts = self.index()?.select(bbox)?;
        Ok(extracts.into_iter().map(|extract| extract.url).collect())
    }

    /// The list of extracts; downloaded once and then kept.
    fn index(&self) -> Result<Arc<GeofabrikIndex>, BuildError> {
        let mut index = self.index.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(index) = index.as_ref() {
            return Ok(Arc::clone(index));
        }
        let fetched = Arc::new(GeofabrikIndex::parse(&download_index()?)?);
        *index = Some(Arc::clone(&fetched));
        Ok(fetched)
    }
}

fn download_index() -> Result<String, BuildError> {
    let fail = |e: ureq::Error| BuildError::Index(e.to_string());
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_global(Some(Duration::from_secs(120)))
        .build()
        .into();
    agent
        .get(GEOFABRIK_INDEX_URL)
        .call()
        .map_err(fail)?
        .body_mut()
        .with_config()
        .limit(MAX_INDEX_LEN)
        .read_to_string()
        .map_err(fail)
}

/// The rule for region ids of `core-maps`; such a name is safe in a path.
fn is_plain_name(name: &str) -> bool {
    let allowed = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_';
    name.len() <= 64
        && name.chars().all(allowed)
        && name
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
}
