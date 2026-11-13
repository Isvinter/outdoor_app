//! Descriptions of downloadable regions: the catalog and the manifest of a region.
//!
//! Both come from outside the app, so everything that later ends up in a file
//! path is validated here.

use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::MapsError;

/// Version of the catalog and manifest format this app understands.
pub const FORMAT: u32 = 1;

/// The tile sets a region consists of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Layer {
    /// Vector base map in the OpenMapTiles schema.
    Basemap,
    /// Vector overlay with trails, signposted routes, pistes and mountain POIs.
    Outdoor,
    /// Elevation tiles in Terrarium encoding.
    Dem,
    /// Raster tiles colouring terrain by slope angle (optional).
    Slope,
}

impl Layer {
    pub const ALL: [Layer; 4] = [Layer::Basemap, Layer::Outdoor, Layer::Dem, Layer::Slope];

    pub fn as_str(self) -> &'static str {
        match self {
            Layer::Basemap => "basemap",
            Layer::Outdoor => "outdoor",
            Layer::Dem => "dem",
            Layer::Slope => "slope",
        }
    }

    /// Name of the archive holding this layer inside a region directory.
    pub fn file_name(self) -> String {
        format!("{}.pmtiles", self.as_str())
    }
}

impl FromStr for Layer {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Layer::ALL
            .into_iter()
            .find(|layer| layer.as_str() == s)
            .ok_or(())
    }
}

/// One file of a region package.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionFile {
    pub name: String,
    pub size: u64,
    /// Lower-case hexadecimal SHA-256 of the file.
    pub sha256: String,
}

/// How the routing tiles of a region were built.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutingInfo {
    /// Version of Valhalla that wrote the tiles.
    pub valhalla: String,
    /// Ways with a higher `mtb:scale` are closed to bicycles.
    pub max_mtb_scale: u8,
}

/// Name of the routing tiles inside a region directory.
pub const ROUTING_FILE: &str = "routing.tar";

/// `manifest.json` of a region package.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionManifest {
    pub format: u32,
    pub id: String,
    pub name: String,
    /// West, south, east, north in degrees.
    pub bbox: [f64; 4],
    /// Build time of the package; newer packages compare greater.
    pub version: String,
    pub attribution: Vec<String>,
    /// Present if the region has routing tiles ([`ROUTING_FILE`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routing: Option<RoutingInfo>,
    pub files: Vec<RegionFile>,
    /// Sum of all file sizes in bytes.
    pub size: u64,
}

impl RegionManifest {
    /// Parses and validates a manifest. `location` only serves error messages.
    pub fn parse(json: &str, location: &str) -> Result<Self, MapsError> {
        let fail = |reason: String| MapsError::InvalidManifest {
            location: location.to_string(),
            reason,
        };
        let manifest: RegionManifest =
            serde_json::from_str(json).map_err(|e| fail(e.to_string()))?;
        check_format(manifest.format).map_err(fail)?;
        check_id(&manifest.id).map_err(fail)?;
        check_bbox(&manifest.bbox).map_err(fail)?;
        if manifest.files.is_empty() {
            return Err(fail("the region has no files".into()));
        }
        for file in &manifest.files {
            let stem = file.name.strip_suffix(".pmtiles").unwrap_or("");
            if !is_plain_name(stem) && file.name != ROUTING_FILE {
                return Err(fail(format!("unexpected file name {:?}", file.name)));
            }
            let is_hex = |c: char| c.is_ascii_digit() || ('a'..='f').contains(&c);
            if file.sha256.len() != 64 || !file.sha256.chars().all(is_hex) {
                return Err(fail(format!("{} has no valid SHA-256 checksum", file.name)));
            }
        }
        Ok(manifest)
    }
}

/// One region offered by a catalog.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub id: String,
    pub name: String,
    pub bbox: [f64; 4],
    pub version: String,
    pub size: u64,
    /// Directory of the region package, relative to the catalog.
    pub path: String,
}

/// `catalog.json`: the regions a server (or a directory) offers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Catalog {
    pub format: u32,
    pub regions: Vec<CatalogEntry>,
}

impl Catalog {
    /// Parses and validates a catalog. `location` only serves error messages.
    pub fn parse(json: &str, location: &str) -> Result<Self, MapsError> {
        let fail = |reason: String| MapsError::InvalidManifest {
            location: location.to_string(),
            reason,
        };
        let catalog: Catalog = serde_json::from_str(json).map_err(|e| fail(e.to_string()))?;
        check_format(catalog.format).map_err(fail)?;
        for entry in &catalog.regions {
            check_id(&entry.id).map_err(fail)?;
            check_bbox(&entry.bbox).map_err(fail)?;
            if entry.path.is_empty() || !entry.path.split('/').all(is_plain_name) {
                return Err(fail(format!(
                    "unexpected path {:?} of region {}",
                    entry.path, entry.id
                )));
            }
        }
        Ok(catalog)
    }
}

fn check_format(format: u32) -> Result<(), String> {
    if format == FORMAT {
        Ok(())
    } else {
        Err(format!(
            "format {format} is not supported, expected {FORMAT}"
        ))
    }
}

/// Lower-case letters, digits, `-` and `_`; must start with a letter or digit.
fn is_plain_name(name: &str) -> bool {
    let allowed = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_';
    name.len() <= 64
        && name.chars().all(allowed)
        && name
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
}

fn check_id(id: &str) -> Result<(), String> {
    if is_plain_name(id) {
        Ok(())
    } else {
        Err(format!("unexpected region id {id:?}"))
    }
}

fn check_bbox(bbox: &[f64; 4]) -> Result<(), String> {
    let [west, south, east, north] = *bbox;
    let valid = (-180.0..=180.0).contains(&west)
        && (-180.0..=180.0).contains(&east)
        && (-90.0..=90.0).contains(&south)
        && (-90.0..=90.0).contains(&north)
        && west < east
        && south < north;
    if valid {
        Ok(())
    } else {
        Err(format!("invalid bounding box {bbox:?}"))
    }
}

/// Address of a tile as used in tile URLs: `<layer>/<z>/<x>/<y>`, optionally
/// with a leading slash and a file extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TilePath {
    pub layer: Layer,
    pub z: u8,
    pub x: u32,
    pub y: u32,
}

impl FromStr for TilePath {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.strip_prefix('/').unwrap_or(s).split('/');
        let layer = parts.next().ok_or(())?.parse()?;
        let z = parts.next().ok_or(())?.parse().map_err(|_| ())?;
        let x = parts.next().ok_or(())?.parse().map_err(|_| ())?;
        let y = parts.next().ok_or(())?;
        let y = y
            .split_once('.')
            .map_or(y, |(number, _)| number)
            .parse()
            .map_err(|_| ())?;
        match parts.next() {
            None => Ok(TilePath { layer, z, x, y }),
            Some(_) => Err(()),
        }
    }
}
