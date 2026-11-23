//! The dark variant of the offline map styles.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use core_maps::{Appearance, Contours, RELIEF_LAYERS, StyleOptions, offline_style};
use serde_json::Value;

const BASE: &str = "outdoor-tiles://localhost/";
const BOUNDS: [f64; 4] = [11.35, 47.35, 11.95, 47.8];
const STYLES: [&str; 3] = [
    "styles/topo-summer.json",
    "styles/topo-winter.json",
    "apps/sky/src-tauri/map-styles/flying.json",
];

fn template(path: &str) -> String {
    std::fs::read_to_string(format!("{}/../../{path}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn fill(json: &str, appearance: Appearance, bounds: Option<[f64; 4]>, contours: Contours) -> Value {
    let options = StyleOptions {
        tiles_base: BASE,
        bounds,
        attribution: &[],
        contours,
        appearance,
    };
    serde_json::from_str(&offline_style(json, &options).unwrap()).unwrap()
}

fn dark(path: &str) -> Value {
    fill(
        &template(path),
        Appearance::Dark,
        Some(BOUNDS),
        Contours::Tiles,
    )
}

fn layers(style: &Value) -> &Vec<Value> {
    style["layers"].as_array().unwrap()
}

fn layer<'a>(style: &'a Value, id: &str) -> Option<&'a Value> {
    layers(style).iter().find(|layer| layer["id"] == id)
}

/// Every colour in a paint value, also inside expressions.
fn colors(value: &Value, found: &mut Vec<String>) {
    match value {
        Value::String(text) if text.starts_with('#') || text.starts_with("rgb") => {
            found.push(text.clone());
        }
        Value::Array(items) => items.iter().for_each(|item| colors(item, found)),
        _ => {}
    }
}

fn colors_of(layer: &Value, property: &str) -> Vec<String> {
    let mut found = Vec::new();
    colors(&layer["paint"][property], &mut found);
    found
}

/// Red, green and blue of `#rgb`, `#rrggbb` or `rgb(a)(r,g,b…)`, 0–255.
fn channels(color: &str) -> [f64; 3] {
    if let Some(hex) = color.strip_prefix('#') {
        let full: String = if hex.len() == 3 {
            hex.chars().flat_map(|c| [c, c]).collect()
        } else {
            hex.to_string()
        };
        return [0, 2, 4].map(|at| f64::from(u8::from_str_radix(&full[at..at + 2], 16).unwrap()));
    }
    let inner = color.split_once('(').unwrap().1.trim_end_matches(')');
    let parts: Vec<f64> = inner
        .split(',')
        .map(|p| p.trim().parse().unwrap())
        .collect();
    [parts[0], parts[1], parts[2]]
}

/// WCAG relative luminance.
fn luminance(color: &str) -> f64 {
    let [r, g, b] = channels(color).map(|channel| {
        let c = channel / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    });
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

fn contrast_on_black(color: &str) -> f64 {
    (luminance(color) + 0.05) / 0.05
}

#[test]
fn background_is_black() {
    for path in STYLES {
        for bounds in [Some(BOUNDS), None] {
            let style = fill(&template(path), Appearance::Dark, bounds, Contours::Tiles);
            let background = layers(&style)
                .iter()
                .find(|layer| layer["type"] == "background")
                .unwrap();
            assert_eq!(background["paint"]["background-color"], "#000000");
        }
    }
}

#[test]
fn every_color_has_a_dark_value() {
    for path in STYLES {
        assert!(!layers(&dark(path)).is_empty());
    }
}

#[test]
fn an_unknown_color_is_an_error() {
    let json = r##"{"version":8,"sources":{},"layers":[
        {"id":"x","type":"background","paint":{"background-color":"#123456"}}]}"##;
    let options = StyleOptions {
        tiles_base: BASE,
        bounds: None,
        attribution: &[],
        contours: Contours::Omit,
        appearance: Appearance::Dark,
    };
    let error = offline_style(json, &options).unwrap_err();
    assert!(error.to_string().contains("#123456"), "{error}");
}

#[test]
fn light_stays_as_it_was() {
    for path in STYLES {
        let json = template(path);
        let source: Value = serde_json::from_str(&json).unwrap();
        let filled = fill(&json, Appearance::Light, Some(BOUNDS), Contours::Tiles);
        assert_eq!(layers(&filled), layers(&source), "{path}");
    }
}

#[test]
fn slope_is_untouched() {
    let json = template("styles/topo-winter.json");
    let light = fill(&json, Appearance::Light, Some(BOUNDS), Contours::Tiles);
    let dark = fill(&json, Appearance::Dark, Some(BOUNDS), Contours::Tiles);
    assert_eq!(layer(&light, "slope"), layer(&dark, "slope"));
}

#[test]
fn relief_starts_hidden() {
    for path in STYLES {
        let style = dark(path);
        for id in RELIEF_LAYERS {
            let layer = layer(&style, id).unwrap();
            assert_eq!(layer["layout"]["visibility"], "none", "{path} {id}");
        }
    }
}

#[test]
fn contours_stay_visible_without_relief() {
    for path in STYLES {
        let style = dark(path);
        for id in ["contour-minor", "contour-major", "contour-label"] {
            let layer = layer(&style, id).unwrap();
            assert_ne!(layer["layout"]["visibility"], "none", "{path} {id}");
        }
    }
}

#[test]
fn relief_without_contours() {
    for path in STYLES {
        let style = fill(
            &template(path),
            Appearance::Dark,
            Some(BOUNDS),
            Contours::Omit,
        );
        assert_eq!(
            layer(&style, "hillshade").unwrap()["layout"]["visibility"],
            "none"
        );
        for id in ["contour-minor", "contour-major", "contour-label"] {
            assert!(layer(&style, id).is_none(), "{path} {id}");
        }
    }
}

#[test]
fn hillshade_is_dark() {
    for path in STYLES {
        let style = dark(path);
        let paint = &layer(&style, "hillshade").unwrap()["paint"];
        assert_eq!(paint["hillshade-shadow-color"], "#000000");
        assert_eq!(paint["hillshade-accent-color"], "#000000");
    }
}

#[test]
fn no_large_area_stays_bright() {
    for path in STYLES {
        let style = dark(path);
        for layer in layers(&style).iter().filter(|l| l["type"] == "fill") {
            for color in colors_of(layer, "fill-color") {
                assert!(luminance(&color) <= 0.03, "{path} {}: {color}", layer["id"]);
            }
        }
    }
}

#[test]
fn labels_are_readable() {
    for path in STYLES {
        let style = dark(path);
        for layer in layers(&style).iter().filter(|l| l["type"] == "symbol") {
            for color in colors_of(layer, "text-color") {
                assert!(
                    contrast_on_black(&color) >= 7.0,
                    "{path} {}: {color}",
                    layer["id"]
                );
            }
            for color in colors_of(layer, "text-halo-color") {
                assert_eq!(luminance(&color), 0.0, "{path} {}", layer["id"]);
            }
        }
    }
}

#[test]
fn trails_and_obstacles_are_readable() {
    for path in STYLES {
        let style = dark(path);
        let lines = layers(&style).iter().filter(|layer| {
            layer["type"] == "line"
                && matches!(layer["source-layer"].as_str(), Some("trail" | "flying"))
                // A casing only separates its line from what lies below.
                && !layer["id"].as_str().unwrap().ends_with("-casing")
        });
        for layer in lines {
            for color in colors_of(layer, "line-color") {
                assert!(
                    contrast_on_black(&color) >= 4.5,
                    "{path} {}: {color}",
                    layer["id"]
                );
            }
        }
    }
}
