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
use towns::{Located, Rank, Towns};

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
    /// The outer coordinates, `(lat, lon)`.
    vertices: Vec<(f64, f64)>,
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
    let vertices = value
        .get("geometry")
        .and_then(outer_vertices)
        .unwrap_or_default();
    let position = mean(&vertices);
    Ok(Feature {
        id,
        tags,
        vertices,
        position,
    })
}

/// The outer coordinates of a geometry, `(lat, lon)`.
fn outer_vertices(geometry: &Value) -> Option<Vec<(f64, f64)>> {
    let coordinates = geometry.get("coordinates")?;
    let mut vertices = Vec::new();
    match geometry.get("type")?.as_str()? {
        "Point" => push_vertex(&mut vertices, coordinates),
        "LineString" => push_vertices(&mut vertices, coordinates),
        "Polygon" => push_vertices(&mut vertices, coordinates.get(0)?),
        "MultiPolygon" => {
            for polygon in coordinates.as_array()? {
                push_vertices(&mut vertices, polygon.get(0)?);
            }
        }
        _ => return None,
    }
    Some(vertices)
}

fn push_vertex(vertices: &mut Vec<(f64, f64)>, position: &Value) {
    if let (Some(lon), Some(lat)) = (
        position.get(0).and_then(Value::as_f64),
        position.get(1).and_then(Value::as_f64),
    ) {
        vertices.push((lat, lon));
    }
}

fn push_vertices(vertices: &mut Vec<(f64, f64)>, list: &Value) {
    for position in list.as_array().into_iter().flatten() {
        push_vertex(vertices, position);
    }
}

fn mean(vertices: &[(f64, f64)]) -> Option<(f64, f64)> {
    if vertices.is_empty() {
        return None;
    }
    let n = vertices.len() as f64;
    let (lat, lon) = vertices
        .iter()
        .fold((0.0, 0.0), |sum, v| (sum.0 + v.0, sum.1 + v.1));
    Some((lat / n, lon / n))
}

/// The OSM object an id stands for. osmium numbers areas `a<2n>` for way `n`
/// and `a<2n+1>` for relation `n`; a closed way is exported as linestring
/// (`w<n>`) and as polygon (`a<2n>`), which must count as one object.
fn source_id(id: &str) -> Result<String, String> {
    let Some(number) = id.strip_prefix('a') else {
        return Ok(id.to_string());
    };
    let number: u64 = number
        .parse()
        .map_err(|_| format!("area id \"{id}\" is not a number"))?;
    let kind = if number.is_multiple_of(2) { 'w' } else { 'r' };
    Ok(format!("{kind}{}", number / 2))
}

/// Calls `each` for every feature of the file, once per OSM object: a closed
/// way appears as linestring and as polygon.
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
        let source = feature
            .id
            .as_deref()
            .map(source_id)
            .transpose()
            .map_err(|reason| BuildError::Line {
                path: input.to_path_buf(),
                line: index + 1,
                reason,
            })?;
        if let Some(id) = source
            && !seen.insert(id)
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
    /// The short list, used when a street or POI is searched with this town.
    alt_names: Vec<String>,
    /// Every name the place itself is found under.
    search_names: Vec<String>,
    lat: f64,
    lon: f64,
    radius: f64,
    rank: Rank,
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
        if let (Some((radius, rank)), Some(name), Some((lat, lon)), None) = (
            classify::place(&f.tags),
            classify::name(&f.tags),
            f.position,
            classify::poi(&f.tags),
        ) {
            places.push(Place {
                name: name.to_string(),
                alt_names: classify::alt_names(&f.tags, name),
                search_names: classify::place_names(&f.tags, name),
                lat,
                lon,
                radius,
                rank,
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
        towns.add(
            &place.name,
            &place.alt_names,
            place.rank,
            (place.lat, place.lon),
            place.radius,
        );
        writer.add(&Entry {
            kind: Kind::Place,
            category: None,
            subtype: Some(place.rank.as_str().to_string()),
            name: place.name,
            alt_names: place.search_names,
            town: None,
            town_names: Vec::new(),
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
            let mut alt_names = classify::alt_names(&f.tags, name);
            // Without a name, the first other name is what the entry shows.
            let display = if name.is_empty() && !alt_names.is_empty() {
                alt_names.remove(0)
            } else {
                name.to_string()
            };
            let Located { town, town_names } = towns.locate(lat, lon);
            info.named_pois += u64::from(!display.is_empty());
            writer.add(&Entry {
                kind: Kind::Poi,
                category: Some(category),
                subtype: Some(subtype),
                name: display,
                alt_names,
                town,
                town_names,
                lat,
                lon,
            })?;
            info.pois += 1;
        } else if classify::place_radius(&f.tags).is_none()
            && classify::is_street(&f.tags)
            && !name.is_empty()
        {
            let alt_names = classify::alt_names(&f.tags, name);
            streets.add(name, alt_names, f.vertices);
        }
        Ok(())
    })?;

    for street in streets.into_merged() {
        let Located { town, town_names } = towns.locate(street.lat, street.lon);
        writer.add(&Entry {
            kind: Kind::Street,
            category: None,
            subtype: None,
            town,
            town_names,
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
