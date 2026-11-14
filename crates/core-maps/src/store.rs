//! Installed regions: download, validation, deletion and tile lookup.
//!
//! Layout below the store directory:
//!
//! ```text
//! <id>/manifest.json, <id>/<layer>.pmtiles    installed regions
//! .staging/<id>/                              downloads in progress
//! ```
//!
//! A region becomes visible only after all of its files were downloaded and
//! verified; the switch is a directory rename.

use std::collections::{BTreeMap, HashSet};
use std::f64::consts::PI;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError, RwLock};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::MapsError;
use crate::dem::{DemCache, DemTile, ProfilePoint, decode_dem_tile, resample};
use crate::fetch::Fetcher;
use crate::manifest::{Catalog, CatalogEntry, Layer, ROUTING_FILE, RegionManifest, RoutingInfo};
use crate::pmtiles::Archive;

const MANIFEST: &str = "manifest.json";
const STAGING: &str = ".staging";
/// Catalogs and manifests larger than this are refused.
const MAX_DOCUMENT_LEN: u64 = 4 << 20;
const CHUNK_LEN: usize = 64 << 10;

/// Download progress of one region.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Progress {
    pub region: String,
    /// Bytes on disk so far.
    pub done: u64,
    /// Bytes of the complete region.
    pub total: u64,
}

/// A region available for offline use.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InstalledRegion {
    pub manifest: RegionManifest,
    /// Space the region takes on disk.
    pub bytes: u64,
}

/// A map tile ready to be handed to the renderer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tile {
    pub data: Vec<u8>,
    pub mime: &'static str,
}

/// The routing tiles of an installed region.
#[derive(Debug, Clone, PartialEq)]
pub struct RoutingTiles {
    /// Id of the region.
    pub region: String,
    pub path: PathBuf,
    pub info: Option<RoutingInfo>,
}

struct Region {
    info: InstalledRegion,
    archives: Vec<(Layer, Archive)>,
}

/// The regions installed in one directory.
pub struct RegionStore {
    dir: PathBuf,
    regions: RwLock<BTreeMap<String, Region>>,
    /// Regions with a download in progress.
    busy: Mutex<HashSet<String>>,
    /// Decoded elevation tiles, most recently used first.
    dem_cache: Mutex<DemCache>,
}

impl RegionStore {
    /// Opens the store in `dir`, creating the directory if needed. Directories
    /// that do not hold a valid region are ignored.
    pub fn open(dir: &Path) -> Result<Self, MapsError> {
        fs::create_dir_all(dir).map_err(MapsError::io(dir))?;
        let mut regions = BTreeMap::new();
        for entry in fs::read_dir(dir).map_err(MapsError::io(dir))? {
            let path = entry.map_err(MapsError::io(dir))?.path();
            let hidden = path
                .file_name()
                .is_none_or(|name| name.to_string_lossy().starts_with('.'));
            if hidden || !path.join(MANIFEST).is_file() {
                continue;
            }
            if let Ok(region) = load_region(&path) {
                regions.insert(region.info.manifest.id.clone(), region);
            }
        }
        Ok(RegionStore {
            dir: dir.to_path_buf(),
            regions: RwLock::new(regions),
            busy: Mutex::default(),
            dem_cache: Mutex::default(),
        })
    }

    /// The installed regions, ordered by id.
    pub fn installed(&self) -> Vec<InstalledRegion> {
        let regions = self.regions.read().unwrap_or_else(PoisonError::into_inner);
        regions.values().map(|region| region.info.clone()).collect()
    }

    /// Reads the catalog at `source`: a directory or URL containing
    /// `catalog.json`, or the location of the catalog file itself.
    pub fn catalog(&self, source: &str, fetcher: &dyn Fetcher) -> Result<Catalog, MapsError> {
        let location = join(&catalog_base(source), "catalog.json");
        Catalog::parse(&fetch_document(fetcher, &location)?, &location)
    }

    /// Downloads region `id` from the catalog at `source` and makes it
    /// available. Blocks until done; `progress` is called after every chunk and
    /// setting `cancel` aborts with [`MapsError::Cancelled`]. Partial downloads
    /// are kept and continued by the next call.
    pub fn install(
        &self,
        source: &str,
        id: &str,
        fetcher: &dyn Fetcher,
        progress: &mut dyn FnMut(Progress),
        cancel: &AtomicBool,
    ) -> Result<InstalledRegion, MapsError> {
        let _guard = BusyGuard::acquire(&self.busy, id)?;
        let base = catalog_base(source);
        let catalog = self.catalog(source, fetcher)?;
        let entry: &CatalogEntry = catalog
            .regions
            .iter()
            .find(|entry| entry.id == id)
            .ok_or_else(|| MapsError::UnknownRegion(id.to_string()))?;
        let package = join(&base, &entry.path);

        let manifest_location = join(&package, MANIFEST);
        let manifest_json = fetch_document(fetcher, &manifest_location)?;
        let manifest = RegionManifest::parse(&manifest_json, &manifest_location)?;
        if manifest.id != id {
            return Err(MapsError::InvalidManifest {
                location: manifest_location,
                reason: format!("describes region {} instead of {id}", manifest.id),
            });
        }

        let staging = self.dir.join(STAGING).join(id);
        prepare_staging(&staging, &manifest_json)?;

        let total: u64 = manifest.files.iter().map(|file| file.size).sum();
        let mut done: u64 = manifest
            .files
            .iter()
            .map(|file| downloaded_len(&staging, &file.name, file.size))
            .sum();
        let mut report = |done: u64| {
            progress(Progress {
                region: id.to_string(),
                done,
                total,
            })
        };
        report(done);

        for file in &manifest.files {
            let target = staging.join(&file.name);
            if target.is_file() {
                continue;
            }
            let part = part_path(&staging, &file.name);
            let mut have = downloaded_len(&staging, &file.name, file.size);
            done -= have;
            if have < file.size {
                let location = join(&package, &file.name);
                let mut fetched = fetcher.open(&location, have)?;
                if fetched.offset != have {
                    // The source cannot resume, start this file over.
                    have = 0;
                }
                let mut out = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&part)
                    .map_err(MapsError::io(&part))?;
                out.set_len(have).map_err(MapsError::io(&part))?;
                let mut chunk = vec![0u8; CHUNK_LEN];
                loop {
                    if cancel.load(Ordering::SeqCst) {
                        return Err(MapsError::Cancelled);
                    }
                    let n = fetched
                        .body
                        .read(&mut chunk)
                        .map_err(|e| MapsError::Download {
                            location: location.clone(),
                            reason: e.to_string(),
                        })?;
                    if n == 0 {
                        break;
                    }
                    have += n as u64;
                    if have > file.size {
                        let _ = fs::remove_file(&part);
                        return Err(MapsError::Checksum {
                            region: id.to_string(),
                            file: file.name.clone(),
                        });
                    }
                    out.write_all(&chunk[..n]).map_err(MapsError::io(&part))?;
                    report(done + have);
                }
                out.sync_all().map_err(MapsError::io(&part))?;
            }
            if have != file.size || sha256_of(&part)? != file.sha256 {
                let _ = fs::remove_file(&part);
                return Err(MapsError::Checksum {
                    region: id.to_string(),
                    file: file.name.clone(),
                });
            }
            fs::rename(&part, &target).map_err(MapsError::io(&target))?;
            done += have;
        }
        report(total);

        // Fails if an archive cannot be read, before anything is replaced.
        drop(load_region(&staging)?);
        self.activate(id, &staging)
    }

    /// Installs a region package that was built on this machine: a directory
    /// holding `manifest.json` and the files it lists. The files are verified
    /// and then moved, not copied; afterwards the package directory is gone.
    /// A package that fails verification is left untouched.
    pub fn install_local(&self, package: &Path) -> Result<InstalledRegion, MapsError> {
        let manifest_path = package.join(MANIFEST);
        let manifest_json =
            fs::read_to_string(&manifest_path).map_err(MapsError::io(&manifest_path))?;
        let manifest = RegionManifest::parse(&manifest_json, &manifest_path.to_string_lossy())?;
        let id = manifest.id.as_str();
        let _guard = BusyGuard::acquire(&self.busy, id)?;

        for file in &manifest.files {
            let path = package.join(&file.name);
            let len = fs::metadata(&path).map_err(MapsError::io(&path))?.len();
            if len != file.size || sha256_of(&path)? != file.sha256 {
                return Err(MapsError::Checksum {
                    region: id.to_string(),
                    file: file.name.clone(),
                });
            }
        }

        let staging = self.dir.join(STAGING).join(id);
        if staging.exists() {
            fs::remove_dir_all(&staging).map_err(MapsError::io(&staging))?;
        }
        fs::create_dir_all(&staging).map_err(MapsError::io(&staging))?;
        for name in manifest
            .files
            .iter()
            .map(|file| file.name.as_str())
            .chain([MANIFEST])
        {
            let target = staging.join(name);
            move_file(&package.join(name), &target, |from, to| {
                fs::rename(from, to)
            })
            .map_err(MapsError::io(&target))?;
        }
        // Whatever else the build left there (notes, logs) is not part of the region.
        let _ = fs::remove_dir_all(package);

        drop(load_region(&staging)?);
        self.activate(id, &staging)
    }

    /// Moves a verified download into place, replacing an older version.
    fn activate(&self, id: &str, staging: &Path) -> Result<InstalledRegion, MapsError> {
        let target = self.dir.join(id);
        let old = self.dir.join(STAGING).join(format!("{id}.old"));
        let mut regions = self.regions.write().unwrap_or_else(PoisonError::into_inner);
        regions.remove(id);
        self.dem_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        if old.exists() {
            fs::remove_dir_all(&old).map_err(MapsError::io(&old))?;
        }
        if target.exists() {
            fs::rename(&target, &old).map_err(MapsError::io(&target))?;
        }
        fs::rename(staging, &target).map_err(MapsError::io(&target))?;
        let _ = fs::remove_dir_all(&old);
        let region = load_region(&target)?;
        let info = region.info.clone();
        regions.insert(id.to_string(), region);
        Ok(info)
    }

    /// Removes an installed region and its files.
    pub fn delete(&self, id: &str) -> Result<(), MapsError> {
        let mut regions = self.regions.write().unwrap_or_else(PoisonError::into_inner);
        if regions.remove(id).is_none() {
            return Err(MapsError::NotInstalled(id.to_string()));
        }
        self.dem_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        let target = self.dir.join(id);
        fs::remove_dir_all(&target).map_err(MapsError::io(&target))
    }

    /// Looks a tile up in the installed regions. `None` if no region has it.
    pub fn tile(&self, layer: Layer, z: u8, x: u32, y: u32) -> Result<Option<Tile>, MapsError> {
        let Some(bounds) = tile_bounds(z, x, y) else {
            return Ok(None);
        };
        let regions = self.regions.read().unwrap_or_else(PoisonError::into_inner);
        for region in regions.values() {
            if !intersects(&region.info.manifest.bbox, &bounds) {
                continue;
            }
            for (_, archive) in region.archives.iter().filter(|(l, _)| *l == layer) {
                if let Some(data) = archive.tile(z, x, y)? {
                    return Ok(Some(Tile {
                        data,
                        mime: archive.header().tile_type.mime(),
                    }));
                }
            }
        }
        Ok(None)
    }
}

impl RegionStore {
    /// The routing tiles of the first region that contains all `(lat, lon)`
    /// points; routes cannot cross from one region into another.
    pub fn routing_tiles(&self, points: &[(f64, f64)]) -> Option<RoutingTiles> {
        if points.is_empty() {
            return None;
        }
        let regions = self.regions.read().unwrap_or_else(PoisonError::into_inner);
        regions.iter().find_map(|(id, region)| {
            let manifest = &region.info.manifest;
            let [west, south, east, north] = manifest.bbox;
            let inside = |(lat, lon): &(f64, f64)| {
                (west..=east).contains(lon) && (south..=north).contains(lat)
            };
            let path = self.dir.join(id).join(ROUTING_FILE);
            (manifest.files.iter().any(|file| file.name == ROUTING_FILE)
                && points.iter().all(inside)
                && path.is_file())
            .then(|| RoutingTiles {
                region: id.clone(),
                path,
                info: manifest.routing.clone(),
            })
        })
    }

    /// Ground elevation in metres at a position, from the finest elevation
    /// tiles of the installed regions; `None` where no region has data.
    /// Interpolated between the four nearest pixels of a tile.
    pub fn elevation(&self, lat: f64, lon: f64) -> Result<Option<f32>, MapsError> {
        if !(-85.0..=85.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
            return Ok(None);
        }
        let regions = self.regions.read().unwrap_or_else(PoisonError::into_inner);
        for (id, region) in regions.iter() {
            let [west, south, east, north] = region.info.manifest.bbox;
            if lon < west || lon > east || lat < south || lat > north {
                continue;
            }
            for (_, archive) in region
                .archives
                .iter()
                .filter(|(layer, _)| *layer == Layer::Dem)
            {
                let header = archive.header();
                // Finest level first; a region may lack the finest tiles in places.
                for z in (header.min_zoom..=header.max_zoom.min(24)).rev() {
                    let n = f64::from(1u32 << z);
                    let x = (lon + 180.0) / 360.0 * n;
                    let y = (1.0 - lat.to_radians().tan().asinh() / PI) / 2.0 * n;
                    let (tile_x, tile_y) = (x.floor() as u32, y.floor() as u32);
                    let key = (id.clone(), z, tile_x, tile_y);
                    let cached = self
                        .dem_cache
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .get(&key);
                    let tile: Arc<DemTile> = match cached {
                        Some(tile) => tile,
                        None => {
                            let Some(data) = archive.tile(z, tile_x, tile_y)? else {
                                continue;
                            };
                            let tile = Arc::new(decode_dem_tile(header.tile_type, &data).map_err(
                                |reason| {
                                    MapsError::Elevation(format!(
                                        "tile {z}/{tile_x}/{tile_y}: {reason}"
                                    ))
                                },
                            )?);
                            self.dem_cache
                                .lock()
                                .unwrap_or_else(PoisonError::into_inner)
                                .put(key, Arc::clone(&tile));
                            tile
                        }
                    };
                    return Ok(Some(tile.sample(x.fract(), y.fract())));
                }
            }
        }
        Ok(None)
    }

    /// Elevation along a line of `(latitude, longitude)` points, sampled at
    /// most every `step_m` metres; the points of the line itself are kept.
    pub fn elevation_profile(
        &self,
        line: &[(f64, f64)],
        step_m: f64,
    ) -> Result<Vec<ProfilePoint>, MapsError> {
        resample(line, step_m)
            .into_iter()
            .map(|(distance_m, lat, lon)| {
                Ok(ProfilePoint {
                    distance_m,
                    lat,
                    lon,
                    ele: self.elevation(lat, lon)?,
                })
            })
            .collect()
    }
}

/// Marks a region as being downloaded for as long as it lives.
struct BusyGuard<'a> {
    busy: &'a Mutex<HashSet<String>>,
    id: String,
}

impl<'a> BusyGuard<'a> {
    fn acquire(busy: &'a Mutex<HashSet<String>>, id: &str) -> Result<Self, MapsError> {
        if busy
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id.to_string())
        {
            Ok(BusyGuard {
                busy,
                id: id.to_string(),
            })
        } else {
            Err(MapsError::Busy(id.to_string()))
        }
    }
}

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.busy
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.id);
    }
}

fn load_region(dir: &Path) -> Result<Region, MapsError> {
    let manifest_path = dir.join(MANIFEST);
    let json = fs::read_to_string(&manifest_path).map_err(MapsError::io(&manifest_path))?;
    let manifest = RegionManifest::parse(&json, &manifest_path.to_string_lossy())?;
    let mut archives = Vec::new();
    let mut bytes = 0;
    for file in &manifest.files {
        let path = dir.join(&file.name);
        bytes += fs::metadata(&path).map_err(MapsError::io(&path))?.len();
        if let Some(layer) = Layer::ALL
            .into_iter()
            .find(|layer| layer.file_name() == file.name)
        {
            archives.push((layer, Archive::open(&path)?));
        }
    }
    Ok(Region {
        info: InstalledRegion { manifest, bytes },
        archives,
    })
}

/// Makes sure the staging directory belongs to this version of the region;
/// partial files of another version are discarded.
fn prepare_staging(staging: &Path, manifest_json: &str) -> Result<(), MapsError> {
    let manifest_path = staging.join(MANIFEST);
    if fs::read_to_string(&manifest_path).is_ok_and(|existing| existing == manifest_json) {
        return Ok(());
    }
    if staging.exists() {
        fs::remove_dir_all(staging).map_err(MapsError::io(staging))?;
    }
    fs::create_dir_all(staging).map_err(MapsError::io(staging))?;
    fs::write(&manifest_path, manifest_json).map_err(MapsError::io(&manifest_path))
}

/// Moves a file; copies and deletes where renaming is impossible, as it is
/// across file systems.
fn move_file(
    from: &Path,
    to: &Path,
    rename: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
    if rename(from, to).is_ok() {
        return Ok(());
    }
    fs::copy(from, to)?;
    fs::remove_file(from)
}

fn part_path(staging: &Path, name: &str) -> PathBuf {
    staging.join(format!("{name}.part"))
}

/// Bytes of a file already in the staging directory. A partial file longer
/// than the announced size counts as nothing and is fetched again.
fn downloaded_len(staging: &Path, name: &str, size: u64) -> u64 {
    if staging.join(name).is_file() {
        return size;
    }
    match fs::metadata(part_path(staging, name)) {
        Ok(meta) if meta.len() <= size => meta.len(),
        _ => 0,
    }
}

fn sha256_of(path: &Path) -> Result<String, MapsError> {
    let mut file = File::open(path).map_err(MapsError::io(path))?;
    let mut hasher = Sha256::new();
    let mut chunk = vec![0u8; CHUNK_LEN];
    loop {
        let n = file.read(&mut chunk).map_err(MapsError::io(path))?;
        if n == 0 {
            break;
        }
        hasher.update(&chunk[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn fetch_document(fetcher: &dyn Fetcher, location: &str) -> Result<String, MapsError> {
    let fail = |reason: String| MapsError::Download {
        location: location.to_string(),
        reason,
    };
    let mut text = String::new();
    let read = fetcher
        .open(location, 0)?
        .body
        .take(MAX_DOCUMENT_LEN + 1)
        .read_to_string(&mut text)
        .map_err(|e| fail(e.to_string()))?;
    if read as u64 > MAX_DOCUMENT_LEN {
        return Err(fail("the document is implausibly large".into()));
    }
    Ok(text)
}

/// The directory (or URL) a catalog source refers to.
fn catalog_base(source: &str) -> String {
    let source = source.trim();
    let base = source.strip_suffix("catalog.json").unwrap_or(source);
    match base.trim_end_matches('/') {
        // The file system root, or a relative `catalog.json`.
        "" if base.starts_with('/') => "/".to_string(),
        "" => ".".to_string(),
        trimmed => trimmed.to_string(),
    }
}

fn join(base: &str, relative: &str) -> String {
    format!("{}/{relative}", base.trim_end_matches('/'))
}

/// West, south, east, north of a tile in degrees; `None` for impossible coordinates.
fn tile_bounds(z: u8, x: u32, y: u32) -> Option<[f64; 4]> {
    if z > 31 || u64::from(x) >= 1u64 << z || u64::from(y) >= 1u64 << z {
        return None;
    }
    let n = (1u64 << z) as f64;
    let lon = |x: f64| x / n * 360.0 - 180.0;
    let lat = |y: f64| (PI * (1.0 - 2.0 * y / n)).sinh().atan().to_degrees();
    let (x, y) = (f64::from(x), f64::from(y));
    Some([lon(x), lat(y + 1.0), lon(x + 1.0), lat(y)])
}

fn intersects(a: &[f64; 4], b: &[f64; 4]) -> bool {
    a[0] <= b[2] && b[0] <= a[2] && a[1] <= b[3] && b[1] <= a[3]
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn move_file_copies_when_rename_fails() {
        let dir = std::env::temp_dir().join(format!("core-maps-move-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let (from, to) = (dir.join("from"), dir.join("to"));
        fs::write(&from, b"tile").unwrap();

        move_file(&from, &to, |_, _| {
            Err(std::io::ErrorKind::CrossesDevices.into())
        })
        .unwrap();

        assert_eq!(fs::read(&to).unwrap(), b"tile");
        assert!(!from.exists());
    }
}
