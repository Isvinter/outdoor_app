//! The offline map styles of `styles/`, filled in for one platform: tile and
//! font URLs, the area covered by the installed regions, attribution.

use serde_json::{Value, json};

use crate::MapsError;

/// Where contour lines come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Contours<'a> {
    /// Tile URL of a generator computing them from the elevation tiles.
    Generator(&'a str),
    /// None: the contour source and its layers are removed.
    Omit,
}

/// What to fill into a style.
#[derive(Debug, Clone, Copy)]
pub struct StyleOptions<'a> {
    /// Base of the tile URL, ending with '/', e.g. `outdoor-tiles://localhost/`.
    pub tiles_base: &'a str,
    /// Smallest box around the installed regions (west, south, east, north);
    /// `None` if nothing is installed.
    pub bounds: Option<[f64; 4]>,
    /// Attribution lines of the installed regions.
    pub attribution: &'a [String],
    pub contours: Contours<'a>,
}

const CONTOURS: &str = "contours";

fn invalid(reason: impl Into<String>) -> MapsError {
    MapsError::Style(reason.into())
}

/// Fills a style of `styles/` (MapLibre style JSON) for the installed regions.
/// Without installed regions only the background layers remain, so that
/// nothing is requested.
pub fn offline_style(style_json: &str, options: &StyleOptions) -> Result<String, MapsError> {
    let mut style: Value = serde_json::from_str(style_json).map_err(|e| invalid(e.to_string()))?;
    let root = style
        .as_object_mut()
        .ok_or_else(|| invalid("not an object"))?;
    // The description of the placeholders, which a filled style no longer has.
    if let Some(Value::Object(metadata)) = root.get_mut("metadata") {
        metadata.remove("placeholders");
    }
    if let Some(Value::String(glyphs)) = root.get_mut("glyphs") {
        *glyphs = glyphs.replace("__FONTS__", &format!("{}fonts/", options.tiles_base));
    }
    let Some(bounds) = options.bounds else {
        root.insert("sources".into(), json!({}));
        if let Some(Value::Array(layers)) = root.get_mut("layers") {
            layers.retain(|layer| layer["type"] == "background");
        }
        return Ok(style.to_string());
    };
    if options.contours == Contours::Omit {
        if let Some(Value::Object(sources)) = root.get_mut("sources") {
            sources.remove(CONTOURS);
        }
        if let Some(Value::Array(layers)) = root.get_mut("layers") {
            layers.retain(|layer| layer["source"] != CONTOURS);
        }
    }
    let generator = match options.contours {
        Contours::Generator(url) => url,
        Contours::Omit => "",
    };
    let Some(Value::Object(sources)) = root.get_mut("sources") else {
        return Err(invalid("no sources"));
    };
    for (id, source) in sources.iter_mut() {
        if !matches!(
            source["type"].as_str(),
            Some("vector" | "raster" | "raster-dem")
        ) {
            continue;
        }
        if let Some(Value::Array(tiles)) = source.get_mut("tiles") {
            for tile in tiles.iter_mut() {
                if let Value::String(url) = tile {
                    *url = url
                        .replace("__TILES__", options.tiles_base)
                        .replace("__CONTOURS__", generator);
                }
            }
        }
        // There are no tiles outside the installed regions; do not ask for them.
        source["bounds"] = json!(bounds);
        if id == "basemap" {
            source["attribution"] = json!(options.attribution.join(" · "));
        }
    }
    Ok(style.to_string())
}
