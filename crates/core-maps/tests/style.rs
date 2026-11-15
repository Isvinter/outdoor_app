//! Filling the offline map styles for a platform.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use core_maps::{Contours, StyleOptions, offline_style};
use serde_json::Value;

const BASE: &str = "outdoor-tiles://localhost/";
const BOUNDS: [f64; 4] = [11.35, 47.35, 11.95, 47.8];

fn style(name: &str) -> String {
    let path = format!("{}/../../styles/{name}.json", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(path).unwrap()
}

fn fill(name: &str, bounds: Option<[f64; 4]>, attribution: &[String], contours: Contours) -> Value {
    let options = StyleOptions {
        tiles_base: BASE,
        bounds,
        attribution,
        contours,
    };
    serde_json::from_str(&offline_style(&style(name), &options).unwrap()).unwrap()
}

fn layers_of<'a>(style: &'a Value, source: &str) -> Vec<&'a Value> {
    style["layers"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|layer| layer["source"] == source)
        .collect()
}

#[test]
fn placeholders_are_replaced() {
    for name in ["topo-summer", "topo-winter"] {
        let options = StyleOptions {
            tiles_base: BASE,
            bounds: Some(BOUNDS),
            attribution: &[],
            contours: Contours::Omit,
        };
        let text = offline_style(&style(name), &options).unwrap();
        for placeholder in ["__TILES__", "__FONTS__", "__CONTOURS__"] {
            assert!(
                !text.contains(placeholder),
                "{name} still has {placeholder}"
            );
        }
        let filled: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            filled["sources"]["basemap"]["tiles"][0],
            "outdoor-tiles://localhost/basemap/{z}/{x}/{y}"
        );
        assert_eq!(
            filled["glyphs"],
            "outdoor-tiles://localhost/fonts/{fontstack}/{range}.pbf"
        );
    }
}

#[test]
fn style_bounds_and_attribution() {
    let attribution = ["A".to_string(), "B".to_string()];
    let filled = fill("topo-winter", Some(BOUNDS), &attribution, Contours::Omit);
    for source in ["basemap", "outdoor", "dem", "slope"] {
        let bounds: Vec<f64> =
            serde_json::from_value(filled["sources"][source]["bounds"].clone()).unwrap();
        assert_eq!(bounds, BOUNDS, "bounds of {source}");
    }
    assert_eq!(filled["sources"]["basemap"]["attribution"], "A · B");
}

#[test]
fn style_without_regions_keeps_only_background() {
    let filled = fill("topo-summer", None, &[], Contours::Omit);
    assert!(filled["sources"].as_object().unwrap().is_empty());
    let layers = filled["layers"].as_array().unwrap();
    assert!(!layers.is_empty());
    assert!(layers.iter().all(|layer| layer["type"] == "background"));
    // No placeholder is left either.
    let text = filled.to_string();
    assert!(
        !text.contains("__TILES__")
            && !text.contains("__FONTS__")
            && !text.contains("__CONTOURS__")
    );
}

#[test]
fn omit_contours_removes_source_and_layers() {
    let before: Value = serde_json::from_str(&style("topo-summer")).unwrap();
    let ids: Vec<&str> = layers_of(&before, "contours")
        .iter()
        .map(|l| l["id"].as_str().unwrap())
        .collect();
    assert!(
        ids.contains(&"contour-minor")
            && ids.contains(&"contour-major")
            && ids.contains(&"contour-label")
    );
    let filled = fill("topo-summer", Some(BOUNDS), &[], Contours::Omit);
    assert!(filled["sources"].get("contours").is_none());
    assert!(layers_of(&filled, "contours").is_empty());
}

#[test]
fn generator_contours_keep_layers() {
    let filled = fill(
        "topo-summer",
        Some(BOUNDS),
        &[],
        Contours::Generator("x://c/{z}/{x}/{y}"),
    );
    assert_eq!(
        filled["sources"]["contours"]["tiles"][0],
        "x://c/{z}/{x}/{y}"
    );
    assert!(!layers_of(&filled, "contours").is_empty());
}

#[test]
fn invalid_json_is_an_error() {
    let options = StyleOptions {
        tiles_base: BASE,
        bounds: Some(BOUNDS),
        attribution: &[],
        contours: Contours::Omit,
    };
    let error = offline_style("{", &options).unwrap_err();
    assert_eq!(error.code(), "map_style");
}

#[test]
fn tiles_contours_point_to_core() {
    let filled = fill("topo-summer", Some(BOUNDS), &[], Contours::Tiles);
    assert_eq!(
        filled["sources"]["contours"]["tiles"][0],
        "outdoor-tiles://localhost/contours/{z}/{x}/{y}"
    );
    let bounds: Vec<f64> =
        serde_json::from_value(filled["sources"]["contours"]["bounds"].clone()).unwrap();
    assert_eq!(bounds, BOUNDS);
    // The packages carry contours up to this zoom; the map enlarges them above.
    assert_eq!(
        filled["sources"]["contours"]["maxzoom"],
        core_maps::CONTOUR_MAX_ZOOM
    );
    assert_eq!(core_maps::CONTOUR_MAX_ZOOM, 14);
    let ids: Vec<&str> = layers_of(&filled, "contours")
        .iter()
        .map(|l| l["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids.len(), 3, "{ids:?}");
}
