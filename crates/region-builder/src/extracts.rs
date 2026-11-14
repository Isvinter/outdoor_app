//! Choosing the OpenStreetMap extracts of Geofabrik that cover a region.

use serde::Deserialize;

use crate::BuildError;

/// Address of the list of all extracts with their outlines (GeoJSON).
pub const GEOFABRIK_INDEX_URL: &str = "https://download.geofabrik.de/index-v1.json";
/// The chosen area is probed at the centres of a grid of this many cells per side.
const GRID: usize = 16;
const EARTH_RADIUS_KM: f64 = 6371.0;

/// An OpenStreetMap extract offered for download.
#[derive(Debug, Clone, PartialEq)]
pub struct Extract {
    pub id: String,
    /// Address of the `.osm.pbf` file.
    pub url: String,
    /// Area of the outline; stands in for the size of the download.
    pub area_km2: f64,
}

/// Rings of `[lon, lat]` positions; the first is the outline, the rest are holes.
type Polygon = Vec<Vec<[f64; 2]>>;

struct Entry {
    extract: Extract,
    polygons: Vec<Polygon>,
}

/// The extracts Geofabrik offers.
pub struct GeofabrikIndex {
    entries: Vec<Entry>,
}

#[derive(Deserialize)]
struct FeatureCollection {
    features: Vec<Feature>,
}

#[derive(Deserialize)]
struct Feature {
    properties: Properties,
    geometry: Option<Geometry>,
}

#[derive(Deserialize)]
struct Properties {
    id: String,
    urls: Option<Urls>,
}

#[derive(Deserialize)]
struct Urls {
    pbf: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "type", content = "coordinates")]
enum Geometry {
    Polygon(Polygon),
    MultiPolygon(Vec<Polygon>),
    #[serde(other)]
    Other,
}

impl GeofabrikIndex {
    /// Reads the index. Entries without a download or an outline are skipped.
    pub fn parse(json: &str) -> Result<Self, BuildError> {
        let collection: FeatureCollection =
            serde_json::from_str(json).map_err(|e| BuildError::Index(e.to_string()))?;
        let entries = collection
            .features
            .into_iter()
            .filter_map(|feature| {
                let url = feature.properties.urls?.pbf?;
                let polygons = match feature.geometry? {
                    Geometry::Polygon(polygon) => vec![polygon],
                    Geometry::MultiPolygon(polygons) => polygons,
                    Geometry::Other => return None,
                };
                let area_km2: f64 = polygons.iter().map(polygon_area_km2).sum();
                (area_km2 > 0.0).then(|| Entry {
                    extract: Extract {
                        id: feature.properties.id,
                        url,
                        area_km2,
                    },
                    polygons,
                })
            })
            .collect();
        Ok(GeofabrikIndex { entries })
    }

    /// Extracts that together cover the box (west, south, east, north), as
    /// little download as possible: repeatedly the extract that covers the
    /// most of what is still open per square kilometre of its own area. Parts
    /// of the box that no extract covers (open sea) are left out.
    pub fn select(&self, bbox: [f64; 4]) -> Result<Vec<Extract>, BuildError> {
        let [west, south, east, north] = bbox;
        let points: Vec<[f64; 2]> = (0..GRID * GRID)
            .map(|i| {
                let (column, row) = ((i % GRID) as f64, (i / GRID) as f64);
                [
                    west + (east - west) * (column + 0.5) / GRID as f64,
                    south + (north - south) * (row + 0.5) / GRID as f64,
                ]
            })
            .collect();
        // For every extract near the box, the grid points it covers.
        let candidates: Vec<(&Entry, Vec<bool>)> = self
            .entries
            .iter()
            .map(|entry| {
                let covered: Vec<bool> = points.iter().map(|p| entry.contains(*p)).collect();
                (entry, covered)
            })
            .filter(|(_, covered)| covered.contains(&true))
            .collect();
        let mut open: Vec<bool> = (0..points.len())
            .map(|i| candidates.iter().any(|(_, covered)| covered[i]))
            .collect();
        if !open.contains(&true) {
            return Err(BuildError::NoExtract);
        }

        let mut chosen = Vec::new();
        while open.contains(&true) {
            let gain = |covered: &[bool]| {
                covered
                    .iter()
                    .zip(&open)
                    .filter(|(covers, open)| **covers && **open)
                    .count() as f64
            };
            let best = candidates
                .iter()
                .map(|(entry, covered)| (entry, covered, gain(covered) / entry.extract.area_km2))
                .filter(|(_, _, value)| *value > 0.0)
                .max_by(|a, b| {
                    a.2.total_cmp(&b.2)
                        .then(b.0.extract.area_km2.total_cmp(&a.0.extract.area_km2))
                });
            let Some((entry, covered, _)) = best else {
                break;
            };
            for (open, covers) in open.iter_mut().zip(covered) {
                *open &= !covers;
            }
            chosen.push(entry.extract.clone());
        }
        Ok(chosen)
    }
}

impl Entry {
    fn contains(&self, point: [f64; 2]) -> bool {
        self.polygons.iter().any(|polygon| {
            // Inside the outline and in none of the holes: an odd number of rings.
            polygon
                .iter()
                .filter(|ring| ring_contains(ring, point))
                .count()
                % 2
                == 1
        })
    }
}

/// Even-odd rule with a ray towards the east.
fn ring_contains(ring: &[[f64; 2]], [x, y]: [f64; 2]) -> bool {
    let mut inside = false;
    for (a, b) in ring.iter().zip(ring.iter().cycle().skip(1)) {
        let crosses = (a[1] > y) != (b[1] > y);
        if crosses && x < a[0] + (y - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) {
            inside = !inside;
        }
    }
    inside
}

fn polygon_area_km2(polygon: &Polygon) -> f64 {
    let mut rings = polygon.iter().map(|ring| ring_area_km2(ring));
    let outline = rings.next().unwrap_or(0.0);
    (outline - rings.sum::<f64>()).max(0.0)
}

/// Area of a ring on the sphere.
fn ring_area_km2(ring: &[[f64; 2]]) -> f64 {
    let sum: f64 = ring
        .iter()
        .zip(ring.iter().cycle().skip(1))
        .map(|(a, b)| {
            (b[0] - a[0]).to_radians() * (2.0 + a[1].to_radians().sin() + b[1].to_radians().sin())
        })
        .sum();
    (sum * EARTH_RADIUS_KM * EARTH_RADIUS_KM / 2.0).abs()
}
