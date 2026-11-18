//! Decoding of Mapbox Vector Tiles.
#![allow(clippy::unwrap_used)]

use serde_json::json;
use tile_inspect::mvt::{Geometry, decode};

fn varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push((value & 0x7f) as u8 | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn field(out: &mut Vec<u8>, number: u64, data: &[u8]) {
    varint(out, number << 3 | 2);
    varint(out, data.len() as u64);
    out.extend_from_slice(data);
}

fn uint(out: &mut Vec<u8>, number: u64, value: u64) {
    varint(out, number << 3);
    varint(out, value);
}

fn packed(values: &[u64]) -> Vec<u8> {
    let mut out = Vec::new();
    for v in values {
        varint(&mut out, *v);
    }
    out
}

fn feature(tags: &[u64], geom_type: u64, geometry: &[u64]) -> Vec<u8> {
    let mut f = Vec::new();
    field(&mut f, 2, &packed(tags));
    uint(&mut f, 3, geom_type);
    field(&mut f, 4, &packed(geometry));
    f
}

/// Layer `flying`: a takeoff point with winch, a power line with voltage and ratio.
fn sample_tile() -> Vec<u8> {
    let mut layer = Vec::new();
    uint(&mut layer, 15, 2);
    field(&mut layer, 1, b"flying");
    // feature 0: kind=takeoff, winch=true; feature 1: kind=power_line, voltage=110, ratio=0.5
    field(&mut layer, 2, &feature(&[0, 0, 1, 1], 1, &[9, 50, 50]));
    field(
        &mut layer,
        2,
        &feature(&[0, 2, 2, 3, 3, 4], 2, &[9, 0, 0, 10, 20, 20]),
    );
    for key in ["kind", "winch", "voltage", "ratio"] {
        field(&mut layer, 3, key.as_bytes());
    }
    let mut takeoff = Vec::new();
    field(&mut takeoff, 1, b"takeoff");
    let mut yes = Vec::new();
    uint(&mut yes, 7, 1);
    let mut power = Vec::new();
    field(&mut power, 1, b"power_line");
    let mut volts = Vec::new();
    uint(&mut volts, 6, 220); // zigzag(110)
    let mut ratio = Vec::new();
    varint(&mut ratio, 3 << 3 | 1);
    ratio.extend_from_slice(&0.5f64.to_le_bytes());
    for value in [takeoff, yes, power, volts, ratio] {
        field(&mut layer, 4, &value);
    }
    uint(&mut layer, 5, 4096);
    let mut tile = Vec::new();
    field(&mut tile, 3, &layer);
    tile
}

#[test]
fn decodes_layer_geometry_and_typed_properties() {
    let layers = decode(&sample_tile());
    assert_eq!(layers.len(), 1);
    assert_eq!(layers[0].name, "flying");
    let features = &layers[0].features;
    assert_eq!(features.len(), 2);
    assert_eq!(features[0].geometry, Geometry::Point);
    assert_eq!(features[0].props["kind"], json!("takeoff"));
    assert_eq!(features[0].props["winch"], json!(true));
    assert_eq!(features[1].geometry, Geometry::Line);
    assert_eq!(features[1].props["voltage"], json!(110));
    assert_eq!(features[1].props["ratio"], json!(0.5));
}

#[test]
fn empty_tile_has_no_layers() {
    assert!(decode(&[]).is_empty());
}
