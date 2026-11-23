//! Region definitions as the build script reads them, ids and size estimates.

use serde::{Deserialize, Serialize};

use crate::BuildError;

const BASEMAP_MAXZOOM: u8 = 14;
const DEM_MAXZOOM: u8 = 13;
/// Longest id made from a name; leaves room for a counter within the 64
/// characters a region id may have.
const MAX_SLUG_LEN: usize = 40;
/// Package size per square kilometre, from the test region
/// isarwinkel-achensee (see docs/MESSPROTOKOLL.md).
const BYTES_PER_KM2: f64 = 40_000.0;
/// Shortest edge of a region.
const MIN_EDGE_KM: f64 = 1.0;
const KM_PER_DEGREE: f64 = 111.195;

/// Which layers a region package gets; decides the build steps and files.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Template {
    Urlaub,
    #[default]
    Outdoor,
    Fliegen,
}

impl Template {
    pub fn is_outdoor(&self) -> bool {
        *self == Template::Outdoor
    }
}

/// What `scripts/build-region.sh` needs to know about a region.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionDefinition {
    pub id: String,
    pub name: String,
    /// West, south, east, north in degrees.
    pub bbox: [f64; 4],
    /// Addresses of the OpenStreetMap extracts that together cover the region.
    pub osm: Vec<String>,
    pub basemap_maxzoom: u8,
    pub dem_maxzoom: u8,
    pub attribution: Vec<String>,
    /// Missing in old definitions, which are `outdoor`.
    #[serde(default, skip_serializing_if = "Template::is_outdoor")]
    pub template: Template,
}

impl RegionDefinition {
    /// A definition with the zoom levels and attribution every region built
    /// in the app gets.
    pub fn new(id: &str, name: &str, bbox: [f64; 4], osm: Vec<String>) -> Self {
        RegionDefinition {
            id: id.to_string(),
            name: name.to_string(),
            bbox,
            osm,
            basemap_maxzoom: BASEMAP_MAXZOOM,
            dem_maxzoom: DEM_MAXZOOM,
            attribution: vec![
                "© OpenStreetMap-Mitwirkende (ODbL)".to_string(),
                "© OpenMapTiles".to_string(),
                MAPTERHORN_ATTRIBUTION.to_string(),
            ],
            template: Template::Outdoor,
        }
    }
}

/// The elevation sources differ from place to place; Mapterhorn lists them.
const MAPTERHORN_ATTRIBUTION: &str =
    "Höhendaten: Mapterhorn und dessen Datenquellen – mapterhorn.com/attribution";

/// A region id made from a name: lower-case letters, digits and single dashes.
pub fn slug(name: &str) -> String {
    let mut slug = String::new();
    for c in name.to_lowercase().chars() {
        match c {
            'a'..='z' | '0'..='9' => slug.push(c),
            'ä' => slug.push_str("ae"),
            'ö' => slug.push_str("oe"),
            'ü' => slug.push_str("ue"),
            'ß' => slug.push_str("ss"),
            _ if slug.is_empty() || slug.ends_with('-') => {}
            _ => slug.push('-'),
        }
    }
    slug.truncate(MAX_SLUG_LEN);
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() {
        "region".to_string()
    } else {
        slug.to_string()
    }
}

/// `slug`, or `slug-2`, `slug-3`, … if that is taken.
pub fn unique_id(slug: &str, taken: &dyn Fn(&str) -> bool) -> String {
    if !taken(slug) {
        return slug.to_string();
    }
    (2u32..)
        .map(|n| format!("{slug}-{n}"))
        .find(|id| !taken(id))
        .unwrap_or_else(|| slug.to_string())
}

/// Width (at the middle latitude) and height of a box in kilometres.
pub fn size_km(bbox: [f64; 4]) -> (f64, f64) {
    let [west, south, east, north] = bbox;
    let middle = ((south + north) / 2.0).to_radians();
    (
        (east - west) * KM_PER_DEGREE * middle.cos(),
        (north - south) * KM_PER_DEGREE,
    )
}

/// Rough size of the package a region of this extent becomes.
pub fn estimated_bytes(bbox: [f64; 4]) -> u64 {
    let (width, height) = size_km(bbox);
    (width * height * BYTES_PER_KM2).max(0.0) as u64
}

/// Checks that a box (west, south, east, north) can be built.
pub fn validate_bbox(bbox: [f64; 4]) -> Result<(), BuildError> {
    let [west, south, east, north] = bbox;
    let fail = |reason: &str| Err(BuildError::InvalidArea(reason.to_string()));
    if bbox.iter().any(|v| !v.is_finite()) || west >= east || south >= north {
        return fail("the corners are not in order");
    }
    if west < -180.0 || east > 180.0 || south < -85.0 || north > 85.0 {
        return fail("it reaches beyond the map");
    }
    let (width, height) = size_km(bbox);
    if width < MIN_EDGE_KM || height < MIN_EDGE_KM {
        return fail("it is smaller than one kilometre");
    }
    Ok(())
}
