//! The dark variant of the offline map styles, for OLED displays: a black pixel uses
//! practically no power. Colours are chosen by hand for what each one means, not
//! inverted, so that trail difficulty, pistes and obstacles keep their hue.

use serde_json::{Value, json};

use crate::MapsError;

/// How a style is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Appearance {
    #[default]
    Light,
    Dark,
}

/// The layers the relief control shows or hides. Hidden, they cost nothing to draw.
/// Contour lines are not among them: the dark map keeps them, as the light one does,
/// so that it stays usable for finding the way off roads and trails.
pub const RELIEF_LAYERS: [&str; 1] = ["hillshade"];

const BLACK: &str = "#000000";
/// The lit side of the terrain, dimmed so that slopes do not outshine the trails.
const HILLSHADE_LIGHT: &str = "#8a8a8a";

/// Colours that mean something else on one layer than everywhere else:
/// layer, property, light colour, dark colour.
const EXCEPTIONS: &[(&str, &str, &str, &str)] =
    &[("trail-track", "line-color", "#ffffff", "#b9a88c")];

/// Light colour (lowercase, without spaces) and its dark counterpart.
const PALETTE: &[(&str, &str)] = &[
    // Areas without reading value disappear; water and ice stay as a faint tint.
    ("#a9cfec", "#07202f"),
    ("#e3f0f8", "#0c2230"),
    ("#eaf5fa", "#0c2230"),
    ("#f4f3ef", BLACK),
    ("#f5f2e8", BLACK),
    ("#fbfcfd", BLACK),
    ("#bcdaa4", BLACK),
    ("#cfe3bf", BLACK),
    ("#dde8da", BLACK),
    ("#cfc6b8", BLACK),
    ("#cfe3d2", BLACK),
    ("#eaf1ee", BLACK),
    ("#dcebc2", BLACK),
    ("#f3f6f2", BLACK),
    ("#dedad3", BLACK),
    ("#e7e7e9", BLACK),
    ("#e2dccf", BLACK),
    ("#e9e3d8", BLACK),
    ("#ece9e4", BLACK),
    ("#efe5c0", BLACK),
    ("#f4f2ea", BLACK),
    ("#f1efd9", BLACK),
    ("#f6f8f6", BLACK),
    // Casings only separate a line from the light ground, which is gone.
    ("#8d7350", BLACK),
    ("#a39f98", BLACK),
    ("#a8662a", BLACK),
    ("#b08d3a", BLACK),
    ("#b59f58", BLACK),
    ("#b5a989", BLACK),
    ("#bdb5a5", BLACK),
    ("#c9c2b4", BLACK),
    // Roads, dimmer the smaller they are, so that trails stand out.
    ("#f0a65a", "#b87f45"),
    ("#f3b97a", "#b87f45"),
    ("#f6d77a", "#9a8550"),
    ("#f8d877", "#9a8550"),
    ("#fbe9a6", "#7d7250"),
    ("#fbee9c", "#7d7250"),
    ("#fdfbf2", "#605c4f"),
    ("#fffbe0", "#605c4f"),
    ("#ffffff", "#4a4a4a"),
    ("#b9b1a2", "#4d4a44"),
    ("#5f5f5f", "#7a7a7a"),
    ("#9a9a9a", "#5a5a5a"),
    // Trails and tracks.
    ("#f3ead8", "#b9a88c"),
    ("#5b4a36", "#c7ac8a"),
    ("#8b8b8b", "#a8a8a8"),
    ("#c4261d", "#ff6b60"),
    ("#1f4fa3", "#7fa8ff"),
    ("#3d6fb3", "#7fb0f0"),
    ("#9aa7b8", "#8fa0b5"),
    ("#2a2a2a", "#f0f0f0"),
    ("#2b7bd6", "#6fb0ff"),
    ("#2e9d4f", "#5fdc84"),
    ("#d9662b", "#ff9a5c"),
    // Marked routes.
    ("#0a86b8", "#4fc3f0"),
    ("#d32f2f", "#ff6b6b"),
    ("#e07b00", "#ffa033"),
    // Pistes.
    ("#222222", "#e8e8e8"),
    ("#3aa655", "#62d884"),
    ("#6b6b6b", "#b5b5b5"),
    ("#d62b2b", "#ff6464"),
    ("#0f8f8f", "#45d6d6"),
    ("#8a5fb0", "#b58ae0"),
    ("#c2187a", "#ff6fc0"),
    // Obstacles: power lines and aerialways.
    ("#2b2b2b", "#e6e6e6"),
    ("#4a4a4a", "#a8a8a8"),
    ("#333333", "#d8d8d8"),
    ("#d01c1c", "#ff4d4d"),
    // Water lines, borders and contour lines.
    ("#7fb3de", "#3f87bd"),
    ("#7a5594", "#8f6fb0"),
    ("#8a6aa0", "#6f5a80"),
    ("#7c8794", "#4f5862"),
    ("#a5713a", "#6f5230"),
    // Labels.
    ("#06658c", "#6fd0f5"),
    ("#1a1a1a", "#f0f0f0"),
    ("#1a3f8f", "#9dbcff"),
    ("#1d5e20", "#8fe39a"),
    ("#222", "#f2f2f2"),
    ("#2b2216", "#f3e9d6"),
    ("#2f6ea5", "#8cc8f5"),
    ("#3c4650", "#c5d0db"),
    ("#3d3020", "#ead9bd"),
    ("#444", "#d0d0d0"),
    ("#444444", "#c8c8c8"),
    ("#4a3b28", "#e3cfb0"),
    ("#555", "#bcbcbc"),
    ("#5d6874", "#aab4bf"),
    ("#6b4e00", "#e8c35a"),
    ("#7a1f1f", "#ff9a9a"),
    ("#8a5a26", "#d9a86c"),
    ("#96105d", "#ff8ccb"),
    ("#a01414", "#ff8a8a"),
    ("#a81f1f", "#ff9c9c"),
    ("#a85c00", "#ffb95c"),
];

fn dark_color(layer: &str, property: &str, color: &str) -> Result<&'static str, MapsError> {
    if property == "text-halo-color" {
        return Ok(BLACK);
    }
    let key: String = color
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    EXCEPTIONS
        .iter()
        .find(|(id, name, light, _)| *id == layer && *name == property && *light == key)
        .map(|(.., dark)| *dark)
        .or_else(|| {
            PALETTE
                .iter()
                .find(|(light, _)| *light == key)
                .map(|(_, dark)| *dark)
        })
        .ok_or_else(|| {
            MapsError::Style(format!("no dark colour for {color} ({layer}, {property})"))
        })
}

/// Replaces every colour in a paint value, also inside expressions.
fn recolor(value: &mut Value, layer: &str, property: &str) -> Result<(), MapsError> {
    match value {
        Value::String(text) if text.starts_with('#') || text.starts_with("rgb") => {
            *text = dark_color(layer, property, text)?.to_string();
        }
        Value::Array(items) => {
            for item in items {
                recolor(item, layer, property)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Turns a light style into its dark variant, with the relief (the hillshade) hidden. Raster layers keep
/// their look: the colours of the slope classes are in the tiles and carry a meaning.
pub(crate) fn apply_dark(style: &mut Value) -> Result<(), MapsError> {
    let Some(Value::Array(layers)) = style.get_mut("layers") else {
        return Ok(());
    };
    for layer in layers {
        let id = layer["id"].as_str().unwrap_or_default().to_string();
        if layer["type"] == "hillshade" {
            let paint = &mut layer["paint"];
            paint["hillshade-shadow-color"] = json!(BLACK);
            paint["hillshade-accent-color"] = json!(BLACK);
            paint["hillshade-highlight-color"] = json!(HILLSHADE_LIGHT);
        } else if let Some(Value::Object(paint)) = layer.get_mut("paint") {
            for (property, value) in paint.iter_mut() {
                if property.ends_with("-color") {
                    recolor(value, &id, property)?;
                }
            }
        }
        if RELIEF_LAYERS.contains(&id.as_str()) {
            layer["layout"]["visibility"] = json!("none");
        }
    }
    Ok(())
}
