//! Builds the search file of a region from an osmium GeoJSON-sequence export.
//!
//! The export comes from `osmium export --add-unique-id=type_id -f geojsonseq
//! --geometry-types=point,linestring,polygon`. The input is read twice: first
//! the places, so that every entry can name its town, then everything else.

mod classify;
mod streets;
mod towns;

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use core_maps::SearchInfo;
use core_search::{Entry, Kind, SEARCH_FORMAT, SearchError, SearchWriter};
use serde_json::Value;

use classify::Tags;
use streets::Streets;
use towns::Towns;

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path}, line {line}: {reason}")]
    Line {
        path: PathBuf,
        line: usize,
        reason: String,
    },
    #[error(transparent)]
    Search(#[from] SearchError),
}

/// One parsed input line.
struct Feature {
    id: Option<String>,
    tags: Tags,
    /// Mean of the outer coordinates, `(lat, lon)`.
    position: Option<(f64, f64)>,
}

fn parse_line(text: &str) -> Result<Feature, String> {
    let value: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let tags = match value.get("properties") {
        Some(Value::Object(tags)) => tags.clone(),
        _ => Tags::new(),
    };
    let id = value.get("id").and_then(Value::as_str).map(str::to_string);
    let position = value.get("geometry").and_then(position_of);
    Ok(Feature { id, tags, position })
}

/// Mean of the outer coordinates of a geometry, `(lat, lon)`.
fn position_of(geometry: &Value) -> Option<(f64, f64)> {
    let coordinates = geometry.get("coordinates")?;
    let mut sum = Sum::default();
    match geometry.get("type")?.as_str()? {
        "Point" => sum.position(coordinates),
        "LineString" => sum.positions(coordinates),
        "Polygon" => sum.positions(coordinates.get(0)?),
        "MultiPolygon" => {
            for polygon in coordinates.as_array()? {
                sum.positions(polygon.get(0)?);
            }
        }
        _ => return None,
    }
    sum.mean()
}

#[derive(Default)]
struct Sum {
    lat: f64,
    lon: f64,
    count: u32,
}

impl Sum {
    fn position(&mut self, position: &Value) {
        if let (Some(lon), Some(lat)) = (
            position.get(0).and_then(Value::as_f64),
            position.get(1).and_then(Value::as_f64),
        ) {
            self.lat += lat;
            self.lon += lon;
            self.count += 1;
        }
    }

    fn positions(&mut self, list: &Value) {
        for position in list.as_array().into_iter().flatten() {
            self.position(position);
        }
    }

    fn mean(&self) -> Option<(f64, f64)> {
        let n = f64::from(self.count);
        (self.count > 0).then(|| (self.lat / n, self.lon / n))
    }
}

/// Calls `each` for every feature of the file, once per id: a closed way
/// appears as linestring and as polygon.
fn read_features(
    input: &Path,
    seen: &mut HashSet<String>,
    mut each: impl FnMut(Feature) -> Result<(), SearchError>,
) -> Result<(), BuildError> {
    let io = |source| BuildError::Io {
        path: input.to_path_buf(),
        source,
    };
    let reader = BufReader::new(File::open(input).map_err(io)?);
    for (index, line) in reader.lines().enumerate() {
        let line = line.map_err(io)?;
        let text = line.trim_start_matches('\u{1e}').trim();
        if text.is_empty() {
            continue;
        }
        let feature = parse_line(text).map_err(|reason| BuildError::Line {
            path: input.to_path_buf(),
            line: index + 1,
            reason,
        })?;
        if let Some(id) = &feature.id
            && !seen.insert(id.clone())
        {
            continue;
        }
        each(feature)?;
    }
    Ok(())
}

/// What a place needs from the first pass.
struct Place {
    name: String,
    alt_names: Vec<String>,
    lat: f64,
    lon: f64,
    radius: f64,
}

/// Builds the search file `output` from `input`. The file is written as
/// `<output>.tmp` and renamed when done; on an error no output is left behind.
pub fn build(
    input: &Path,
    output: &Path,
    osm_timestamp: Option<&str>,
) -> Result<SearchInfo, BuildError> {
    let mut tmp = output.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    // Left behind by a crashed run.
    let _ = std::fs::remove_file(&tmp);
    let result = build_into(input, &tmp, osm_timestamp).and_then(|info| {
        std::fs::rename(&tmp, output).map_err(|source| BuildError::Io {
            path: output.to_path_buf(),
            source,
        })?;
        Ok(info)
    });
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

fn build_into(
    input: &Path,
    tmp: &Path,
    osm_timestamp: Option<&str>,
) -> Result<SearchInfo, BuildError> {
    // Pass 1: the places. A feature that is also a POI counts as POI only.
    let mut places: Vec<Place> = Vec::new();
    read_features(input, &mut HashSet::new(), |f| {
        if let (Some(radius), Some(name), Some((lat, lon)), None) = (
            classify::place_radius(&f.tags),
            classify::name(&f.tags),
            f.position,
            classify::poi(&f.tags),
        ) {
            places.push(Place {
                name: name.to_string(),
                alt_names: classify::alt_names(&f.tags, name),
                lat,
                lon,
                radius,
            });
        }
        Ok(())
    })?;

    let mut info = SearchInfo {
        format: SEARCH_FORMAT,
        places: 0,
        streets: 0,
        pois: 0,
        named_pois: 0,
    };
    let mut writer = SearchWriter::create(tmp, osm_timestamp)?;
    let mut towns = Towns::default();
    for place in places {
        towns.add(&place.name, place.lat, place.lon, place.radius);
        writer.add(&Entry {
            kind: Kind::Place,
            category: None,
            subtype: None,
            name: place.name,
            alt_names: place.alt_names,
            town: None,
            lat: place.lat,
            lon: place.lon,
        })?;
        info.places += 1;
    }

    // Pass 2: POIs are written as they come, streets are merged first.
    let mut streets = Streets::default();
    read_features(input, &mut HashSet::new(), |f| {
        let Some((lat, lon)) = f.position else {
            return Ok(());
        };
        let name = classify::name(&f.tags).unwrap_or("");
        if let Some((category, subtype)) = classify::poi(&f.tags) {
            writer.add(&Entry {
                kind: Kind::Poi,
                category: Some(category),
                subtype: Some(subtype),
                name: name.to_string(),
                alt_names: classify::alt_names(&f.tags, name),
                town: towns.nearest(lat, lon).map(str::to_string),
                lat,
                lon,
            })?;
            info.pois += 1;
            info.named_pois += u64::from(!name.is_empty());
        } else if classify::place_radius(&f.tags).is_none()
            && classify::is_street(&f.tags)
            && !name.is_empty()
        {
            streets.add(name, classify::alt_names(&f.tags, name), lat, lon);
        }
        Ok(())
    })?;

    for street in streets.into_merged() {
        writer.add(&Entry {
            kind: Kind::Street,
            category: None,
            subtype: None,
            town: towns.nearest(street.lat, street.lon).map(str::to_string),
            name: street.name,
            alt_names: street.alt_names,
            lat: street.lat,
            lon: street.lon,
        })?;
        info.streets += 1;
    }
    writer.finish()?;
    Ok(info)
}
