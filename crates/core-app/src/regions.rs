//! Offline map regions: what the catalog offers, what is installed, downloads.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use core_maps::{DefaultFetcher, InstalledRegion, Layer, MapsError, Progress, RegionStore, Tile};
use serde::{Deserialize, Serialize};

use crate::AppError;
use crate::app::{App, now_ms};

/// Settings key holding the catalog location (a URL or a directory).
const CATALOG_SOURCE_KEY: &str = "maps.catalog_source";
/// Download progress is sent to the listener at most this often.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(150);

/// Map state owned by the app.
pub(crate) struct Maps {
    store: RegionStore,
    fetcher: DefaultFetcher,
    /// Cancel flags of the downloads in progress, by region id.
    downloads: Mutex<HashMap<String, Arc<AtomicBool>>>,
    /// Directory with the definitions (`<id>.json`) of regions that can be
    /// built on this device. The desktop writes them; nothing does on mobile.
    definitions: PathBuf,
}

impl Maps {
    pub(crate) fn open(dir: &Path, definitions: &Path) -> Result<Self, MapsError> {
        Ok(Maps {
            definitions: definitions.to_path_buf(),
            store: RegionStore::open(dir)?,
            fetcher: DefaultFetcher::new(),
            downloads: Mutex::default(),
        })
    }

    pub(crate) fn store(&self) -> &RegionStore {
        &self.store
    }

    fn definition_path(&self, id: &str) -> PathBuf {
        self.definitions.join(format!("{id}.json"))
    }

    /// The regions that have a definition. Files that cannot be read as one
    /// are skipped.
    fn defined(&self) -> Vec<Definition> {
        let Ok(entries) = fs::read_dir(&self.definitions) else {
            return Vec::new();
        };
        entries
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                let definition: Definition =
                    serde_json::from_str(&fs::read_to_string(&path).ok()?).ok()?;
                // The file name is what a build is started with.
                (path == self.definition_path(&definition.id)).then_some(definition)
            })
            .collect()
    }

    fn downloads(&self) -> std::sync::MutexGuard<'_, HashMap<String, Arc<AtomicBool>>> {
        self.downloads
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

/// The part of a region definition the region list shows.
#[derive(Deserialize)]
struct Definition {
    id: String,
    name: String,
    bbox: [f64; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionState {
    /// Defined on this device, but not built yet (or the build did not finish).
    NotBuilt,
    /// In the catalog, not on this device.
    Available,
    /// On this device and up to date (or the catalog does not list it).
    Installed,
    /// On this device; the catalog has a newer build.
    UpdateAvailable,
    /// Being downloaded right now.
    Downloading,
}

/// A region as shown in the region list.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RegionInfo {
    pub id: String,
    pub name: String,
    /// West, south, east, north in degrees.
    pub bbox: [f64; 4],
    /// Download size, or the space taken on disk for installed regions.
    pub size: u64,
    /// Build offered by the catalog, or the installed one if the catalog does not list the region.
    pub version: String,
    pub installed_version: Option<String>,
    pub state: RegionState,
    /// This device has the definition of the region and can build it.
    pub buildable: bool,
}

/// Everything the region list needs.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RegionsOverview {
    /// Catalog location; empty if none is configured.
    pub source: String,
    /// Why the catalog could not be read, if so. Installed regions are listed regardless.
    pub catalog_error: Option<String>,
    pub regions: Vec<RegionInfo>,
    /// Attribution lines of all installed regions, without duplicates.
    pub attribution: Vec<String>,
    /// Smallest box around all installed regions (west, south, east, north);
    /// there are no offline tiles outside of it.
    pub installed_bounds: Option<[f64; 4]>,
}

impl App {
    /// The configured catalog location; empty if none is set.
    pub fn catalog_source(&self) -> Result<String, AppError> {
        Ok(self
            .shared
            .inner()
            .storage
            .setting(CATALOG_SOURCE_KEY)?
            .unwrap_or_default())
    }

    /// Sets the catalog location: a URL or a directory containing `catalog.json`.
    pub fn set_catalog_source(&self, source: &str) -> Result<(), AppError> {
        Ok(self
            .shared
            .inner()
            .storage
            .set_setting(CATALOG_SOURCE_KEY, source.trim(), now_ms())?)
    }

    /// Lists installed regions and what the catalog offers. Reads the catalog,
    /// which may take a while over the network; never fails because of it.
    pub fn regions(&self) -> Result<RegionsOverview, AppError> {
        let maps = &self.shared.maps;
        let source = self.catalog_source()?;
        let (catalog, catalog_error) = if source.is_empty() {
            (Vec::new(), None)
        } else {
            match maps.store.catalog(&source, &maps.fetcher) {
                Ok(catalog) => (catalog.regions, None),
                Err(e) => (Vec::new(), Some(e.to_string())),
            }
        };
        let installed = maps.store.installed();
        let downloading = maps.downloads();

        let mut attribution: Vec<String> = Vec::new();
        for line in installed
            .iter()
            .flat_map(|region| &region.manifest.attribution)
        {
            if !attribution.contains(line) {
                attribution.push(line.clone());
            }
        }

        let installed_bounds =
            installed
                .iter()
                .map(|region| region.manifest.bbox)
                .reduce(|a, b| {
                    [
                        a[0].min(b[0]),
                        a[1].min(b[1]),
                        a[2].max(b[2]),
                        a[3].max(b[3]),
                    ]
                });

        let mut regions: BTreeMap<String, RegionInfo> = BTreeMap::new();
        for InstalledRegion { manifest, bytes } in installed {
            let info = RegionInfo {
                id: manifest.id.clone(),
                name: manifest.name,
                bbox: manifest.bbox,
                size: bytes,
                version: manifest.version.clone(),
                installed_version: Some(manifest.version),
                state: RegionState::Installed,
                buildable: false,
            };
            regions.insert(manifest.id, info);
        }
        for entry in catalog {
            match regions.get_mut(&entry.id) {
                Some(info) => {
                    if entry.version > info.version {
                        info.version = entry.version;
                        info.state = RegionState::UpdateAvailable;
                    }
                }
                None => {
                    let info = RegionInfo {
                        id: entry.id.clone(),
                        name: entry.name,
                        bbox: entry.bbox,
                        size: entry.size,
                        version: entry.version,
                        installed_version: None,
                        state: RegionState::Available,
                        buildable: false,
                    };
                    regions.insert(entry.id, info);
                }
            }
        }
        for definition in maps.defined() {
            regions
                .entry(definition.id.clone())
                .or_insert(RegionInfo {
                    id: definition.id,
                    name: definition.name,
                    bbox: definition.bbox,
                    size: 0,
                    version: String::new(),
                    installed_version: None,
                    state: RegionState::NotBuilt,
                    buildable: false,
                })
                .buildable = true;
        }
        for info in regions
            .values_mut()
            .filter(|info| downloading.contains_key(&info.id))
        {
            info.state = RegionState::Downloading;
        }
        Ok(RegionsOverview {
            source,
            catalog_error,
            regions: regions.into_values().collect(),
            attribution,
            installed_bounds,
        })
    }

    /// Downloads a region from the catalog and makes it available. Blocks until
    /// done; progress goes to the listener. An interrupted download continues
    /// where it stopped when this is called again.
    pub fn install_region(&self, id: &str) -> Result<InstalledRegion, AppError> {
        let maps = &self.shared.maps;
        let source = self.catalog_source()?;
        if source.is_empty() {
            return Err(AppError::NoCatalog);
        }
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut downloads = maps.downloads();
            if downloads.contains_key(id) {
                return Err(MapsError::Busy(id.to_string()).into());
            }
            downloads.insert(id.to_string(), Arc::clone(&cancel));
        }

        let listener = &self.shared.listener;
        let mut last_report: Option<Instant> = None;
        let mut report = |progress: Progress| {
            let finished = progress.done >= progress.total;
            if finished || last_report.is_none_or(|at| at.elapsed() >= PROGRESS_INTERVAL) {
                last_report = Some(Instant::now());
                listener.on_region_progress(&progress);
            }
        };
        let result = maps
            .store
            .install(&source, id, &maps.fetcher, &mut report, &cancel);
        maps.downloads().remove(id);
        // An update replaced the routing data the open router reads.
        self.shared.routing.forget_region(id);
        Ok(result?)
    }

    /// Stops the download of a region, if one is running. The partial download is kept.
    pub fn cancel_region_download(&self, id: &str) {
        if let Some(cancel) = self.shared.maps.downloads().get(id) {
            cancel.store(true, Ordering::SeqCst);
        }
    }

    /// Installs a region package built on this device (a directory with
    /// `manifest.json` and its files). The package is moved, not copied.
    pub fn install_local_region(&self, package: &Path) -> Result<InstalledRegion, AppError> {
        let region = self.shared.maps.store.install_local(package)?;
        // A rebuild replaced the routing data the open router reads.
        self.shared.routing.forget_region(&region.manifest.id);
        Ok(region)
    }

    /// Removes an installed region from this device, and its definition if it
    /// was defined here. A region that was defined but never built only loses
    /// the definition.
    pub fn delete_region(&self, id: &str) -> Result<(), AppError> {
        let maps = &self.shared.maps;
        self.shared.routing.forget_region(id);
        let defined = maps.defined().iter().any(|definition| definition.id == id);
        match maps.store.delete(id) {
            Err(MapsError::NotInstalled(_)) if defined => {}
            result => result?,
        }
        if defined {
            let path = maps.definition_path(id);
            fs::remove_file(&path).map_err(|source| MapsError::Io { path, source })?;
        }
        Ok(())
    }

    /// Whether any installed region has the archive of `layer`.
    pub fn map_has_layer(&self, layer: Layer) -> bool {
        self.shared.maps.store.has_layer(layer)
    }

    /// A map tile from the installed regions; `None` if no region covers it.
    pub fn map_tile(&self, layer: Layer, z: u8, x: u32, y: u32) -> Result<Option<Tile>, AppError> {
        Ok(self.shared.maps.store.tile(layer, z, x, y)?)
    }
}
