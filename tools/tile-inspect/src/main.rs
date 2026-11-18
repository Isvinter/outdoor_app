//! Prints the features of a PMTiles vector archive within a box at one zoom
//! level as JSON lines, or a count per layer and kind with `--summary`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use core_maps::Archive;
use serde_json::{Value, json};
use tile_inspect::mvt::{Geometry, decode};
use tile_inspect::tile_range;

#[derive(Parser)]
#[command(about = "Prints the features of PMTiles vector tiles as JSON lines")]
struct Args {
    /// PMTiles archive with vector tiles.
    archive: PathBuf,
    /// Zoom level to read.
    #[arg(long)]
    zoom: u8,
    /// West,south,east,north in degrees.
    #[arg(
        long,
        value_delimiter = ',',
        required = true,
        allow_negative_numbers = true
    )]
    bbox: Vec<f64>,
    /// Only this layer.
    #[arg(long)]
    layer: Option<String>,
    /// One line per layer and kind with the number of features.
    #[arg(long)]
    summary: bool,
}

fn geometry_name(geometry: Geometry) -> &'static str {
    match geometry {
        Geometry::Point => "point",
        Geometry::Line => "line",
        Geometry::Polygon => "polygon",
        Geometry::Unknown => "unknown",
    }
}

fn run(args: &Args) -> Result<(), String> {
    let bbox: [f64; 4] = args
        .bbox
        .as_slice()
        .try_into()
        .map_err(|_| "--bbox needs four numbers".to_string())?;
    if args.zoom > 30 {
        return Err("--zoom must be at most 30".into());
    }
    let archive = Archive::open(&args.archive).map_err(|e| e.to_string())?;
    let (x0, y0, x1, y1) = tile_range(bbox, args.zoom);
    let mut counts: BTreeMap<(String, String), u64> = BTreeMap::new();
    for x in x0..=x1 {
        for y in y0..=y1 {
            let Some(tile) = archive.tile(args.zoom, x, y).map_err(|e| e.to_string())? else {
                continue;
            };
            for layer in decode(&tile) {
                if args.layer.as_ref().is_some_and(|name| *name != layer.name) {
                    continue;
                }
                for feature in layer.features {
                    if args.summary {
                        let kind = feature
                            .props
                            .get("kind")
                            .or_else(|| feature.props.get("class"))
                            .map_or("null".to_string(), |v| match v {
                                Value::String(s) => s.clone(),
                                other => other.to_string(),
                            });
                        *counts.entry((layer.name.clone(), kind)).or_default() += 1;
                    } else {
                        let line = json!({
                            "layer": layer.name,
                            "geometry": geometry_name(feature.geometry),
                            "props": feature.props,
                        });
                        println!("{line}");
                    }
                }
            }
        }
    }
    for ((layer, kind), count) in counts {
        println!("{}", json!({"layer": layer, "kind": kind, "count": count}));
    }
    Ok(())
}

fn main() -> ExitCode {
    match run(&Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("tile-inspect: {message}");
            ExitCode::FAILURE
        }
    }
}
