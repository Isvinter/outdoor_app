//! Airspaces from the openAIP export (GeoJSON).
//!
//! openAIP (openaip.net) publishes one file per country, e.g.
//! `https://storage.openaip.net/openaip-system-exports/de_asp.geojson`: a GeoJSON
//! feature collection with one polygon per airspace. Kinds, classes and limits come as
//! numbers; this module turns them into the same [`Airspace`] values the OpenAir reader
//! makes, using the OpenAir names for the kinds, so everything after reading is the same.
//!
//! The data is licensed by openAIP under CC BY-NC 4.0; whoever shows it must name
//! openAIP as the source.

use serde::Deserialize;

use crate::airspace::{Airspace, AirspaceFile, FOOT_M, Issue, Limit, Reference, Unit};

/// Who to name wherever the data is shown.
pub const ATTRIBUTION: &str = "Luftraumdaten © openAIP (openaip.net), CC BY-NC 4.0";

// `Deserialize` (from serde) generates the code that fills these structs from JSON;
// fields that are not listed here are skipped.
#[derive(Deserialize)]
struct Collection {
    features: Vec<Feature>,
}

#[derive(Deserialize)]
struct Feature {
    properties: Properties,
    geometry: Option<Geometry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Properties {
    #[serde(default)]
    name: String,
    #[serde(rename = "type")]
    kind: u32,
    icao_class: u32,
    lower_limit: RawLimit,
    upper_limit: RawLimit,
    #[serde(default)]
    frequencies: Vec<Frequency>,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
struct RawLimit {
    value: f64,
    unit: u32,
    reference_datum: u32,
}

#[derive(Deserialize)]
struct Frequency {
    value: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    primary: bool,
}

#[derive(Deserialize)]
struct Geometry {
    #[serde(rename = "type")]
    kind: String,
    /// For a polygon: rings of `[longitude, latitude]`; the first ring is the outline.
    coordinates: serde_json::Value,
}

/// Reads an openAIP export. Features that cannot be used are skipped and reported as
/// issues (with their position in the file as "line"); a file that is not a feature
/// collection at all gives one issue and no airspaces.
pub fn parse(bytes: &[u8]) -> AirspaceFile {
    let mut file = AirspaceFile::default();
    let collection: Collection = match serde_json::from_slice(bytes) {
        Ok(collection) => collection,
        Err(error) => {
            file.issues.push(Issue {
                line: error.line(),
                message: format!("not an openAIP airspace export: {error}"),
            });
            return file;
        }
    };
    for (index, feature) in collection.features.into_iter().enumerate() {
        match airspace(feature) {
            Ok(airspace) => file.airspaces.push(airspace),
            Err(message) => file.issues.push(Issue {
                line: index + 1,
                message,
            }),
        }
    }
    file
}

fn airspace(feature: Feature) -> Result<Airspace, String> {
    let p = feature.properties;
    let named = |message: String| format!("{}: {message}", p.name);
    let outline = outline(feature.geometry.as_ref()).map_err(named)?;
    let primary = p
        .frequencies
        .iter()
        .find(|f| f.primary)
        .or(p.frequencies.first());
    Ok(Airspace {
        class: class_name(p.icao_class).to_owned(),
        kind: Some(kind_name(p.kind).to_owned()),
        lower: limit(p.lower_limit).map_err(named)?,
        upper: limit(p.upper_limit).map_err(named)?,
        frequency: primary.map(|f| f.value.clone()),
        station: primary.and_then(|f| f.name.clone()),
        name: p.name,
        outline,
    })
}

/// The outer ring as `(latitude, longitude)`, without repeating the first point.
fn outline(geometry: Option<&Geometry>) -> Result<Vec<(f64, f64)>, String> {
    let geometry = geometry.ok_or("no geometry")?;
    if geometry.kind != "Polygon" {
        return Err(format!("geometry {} is not supported", geometry.kind));
    }
    let ring = geometry
        .coordinates
        .get(0)
        .and_then(|ring| ring.as_array())
        .ok_or("polygon without outline")?;
    let mut points = Vec::with_capacity(ring.len());
    for point in ring {
        let lon = point.get(0).and_then(serde_json::Value::as_f64);
        let lat = point.get(1).and_then(serde_json::Value::as_f64);
        match (lat, lon) {
            (Some(lat), Some(lon))
                if (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon) =>
            {
                points.push((lat, lon));
            }
            _ => return Err("invalid coordinate".to_owned()),
        }
    }
    if points.len() > 1 && points.first() == points.last() {
        points.pop();
    }
    if points.len() < 3 {
        return Err("outline with fewer than three points".to_owned());
    }
    Ok(points)
}

/// openAIP units: 0 metres, 1 feet, 6 flight level. Reference: 0 ground, 1 mean sea
/// level, 2 standard pressure.
fn limit(raw: RawLimit) -> Result<Limit, String> {
    let RawLimit {
        value,
        unit,
        reference_datum,
    } = raw;
    if !value.is_finite() || value < 0.0 {
        return Err(format!("invalid limit value {value}"));
    }
    let (metres, unit_kind) = match unit {
        0 => (value, Unit::Metres),
        1 => (value * FOOT_M, Unit::Feet),
        6 => return Ok(Limit::FlightLevel(value.round() as u16)),
        other => return Err(format!("unknown limit unit {other}")),
    };
    match reference_datum {
        0 if value == 0.0 => Ok(Limit::Ground),
        0 => Ok(Limit::Height {
            metres,
            unit: unit_kind,
            reference: Reference::Agl,
        }),
        1 => Ok(Limit::Height {
            metres,
            unit: unit_kind,
            reference: Reference::Msl,
        }),
        // A height at standard pressure is a flight level.
        2 => Ok(Limit::FlightLevel((metres / FOOT_M / 100.0).round() as u16)),
        other => Err(format!("unknown limit reference {other}")),
    }
}

/// ICAO class as in OpenAir files (`AC`).
fn class_name(code: u32) -> &'static str {
    match code {
        0 => "A",
        1 => "B",
        2 => "C",
        3 => "D",
        4 => "E",
        5 => "F",
        6 => "G",
        _ => "UNC",
    }
}

/// openAIP airspace type as the OpenAir 2 type name (`AY`), which
/// [`Airspace::category`] understands.
fn kind_name(code: u32) -> &'static str {
    match code {
        1 => "R",
        2 => "Q",
        3 => "P",
        4 => "CTR",
        5 => "TMZ",
        6 => "RMZ",
        7 => "TMA",
        8 => "TRA",
        9 => "TSA",
        10 => "FIR",
        11 => "UIR",
        12 => "ADIZ",
        13 => "ATZ",
        14 => "MATZ",
        15 => "AWY",
        16 => "MTR",
        17 => "ALERT",
        18 => "WARNING",
        19 => "PROTECTED",
        20 => "HTZ",
        21 => "GSEC",
        22 => "TRP",
        23 => "TIZ",
        24 => "TIA",
        25 => "MTA",
        26 => "CTA",
        27 => "ACCSEC",
        28 => "ASRA",
        29 => "OFR",
        30 => "MRT",
        31 => "TFR",
        32 => "VFRSEC",
        33 => "FIS",
        34 => "LTA",
        35 => "UTA",
        36 => "MCTR",
        _ => "NONE",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::airspace::Category;

    /// Two airspaces in the shape of the real export (shortened), plus a broken one.
    const SAMPLE: &str = r#"{"type":"FeatureCollection","features":[
      {"type":"Feature","id":1,"properties":{"_id":"a","name":"TEST CTR","type":4,"icaoClass":3,
        "country":"DE","upperLimit":{"value":3500,"unit":1,"referenceDatum":1},
        "lowerLimit":{"value":0,"unit":1,"referenceDatum":0},
        "frequencies":[{"value":"118.500","primary":false,"name":"OTHER"},
                       {"value":"120.100","primary":true,"name":"TEST TOWER","unit":2}]},
        "geometry":{"type":"Polygon","coordinates":[[[11.0,47.0],[11.2,47.0],[11.2,47.2],[11.0,47.2],[11.0,47.0]]]}},
      {"type":"Feature","id":2,"properties":{"name":"TEST TMA","type":7,"icaoClass":2,
        "upperLimit":{"value":100,"unit":6,"referenceDatum":2},
        "lowerLimit":{"value":1500,"unit":1,"referenceDatum":0}},
        "geometry":{"type":"Polygon","coordinates":[[[11.0,47.0],[12.0,47.0],[12.0,48.0],[11.0,47.0]]]}},
      {"type":"Feature","id":3,"properties":{"name":"BROKEN","type":1,"icaoClass":8,
        "upperLimit":{"value":100,"unit":6,"referenceDatum":2},
        "lowerLimit":{"value":0,"unit":1,"referenceDatum":0}},
        "geometry":{"type":"Point","coordinates":[11.0,47.0]}}
    ]}"#;

    #[test]
    fn reads_kinds_limits_and_outlines() {
        let file = parse(SAMPLE.as_bytes());
        assert_eq!(file.airspaces.len(), 2);
        let ctr = &file.airspaces[0];
        assert_eq!(ctr.name, "TEST CTR");
        assert_eq!(
            (ctr.class.as_str(), ctr.kind.as_deref()),
            ("D", Some("CTR"))
        );
        assert_eq!(ctr.category(), Category::Ctr);
        assert_eq!(ctr.lower, Limit::Ground);
        assert_eq!(ctr.upper.label(), "3500 ft");
        assert_eq!(ctr.frequency.as_deref(), Some("120.100"));
        assert_eq!(ctr.station.as_deref(), Some("TEST TOWER"));
        assert_eq!(ctr.outline.len(), 4, "closing point dropped");
        assert_eq!(ctr.outline[1], (47.0, 11.2), "latitude first");
        assert!(ctr.contains(47.1, 11.1));

        let tma = &file.airspaces[1];
        assert_eq!(tma.category(), Category::Controlled);
        assert_eq!(tma.lower.label(), "1500 ft AGL");
        assert_eq!(tma.upper, Limit::FlightLevel(100));

        assert_eq!(file.issues.len(), 1);
        assert_eq!(file.issues[0].line, 3);
        assert!(file.issues[0].message.contains("BROKEN"));
    }

    #[test]
    fn limits_in_all_units() {
        let raw = |value, unit, reference_datum| {
            limit(RawLimit {
                value,
                unit,
                reference_datum,
            })
        };
        assert_eq!(raw(95.0, 6, 2), Ok(Limit::FlightLevel(95)));
        assert_eq!(raw(9500.0, 1, 2), Ok(Limit::FlightLevel(95)));
        assert_eq!(raw(0.0, 1, 0), Ok(Limit::Ground));
        assert_eq!(raw(1000.0, 0, 1).unwrap().label(), "1000 m");
        assert!(raw(5.0, 9, 1).is_err());
        assert!(raw(5.0, 1, 7).is_err());
    }

    #[test]
    fn rejects_files_that_are_no_export() {
        let file = parse(b"AC D\nAN Not JSON\n");
        assert!(file.airspaces.is_empty());
        assert_eq!(file.issues.len(), 1);
        assert!(file.issues[0].message.contains("not an openAIP"));
    }
}
