//! Command line: elevation tiles in, slope angle tiles out.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use clap::Parser;
use slope_tiles::{Options, run};

/// Derives slope angle tiles from Terrarium elevation tiles (PMTiles in, PMTiles out).
///
/// Terrain is coloured in the classes of avalanche maps: 30-35° yellow,
/// 35-40° orange, 40-45° red, steeper violet; flatter terrain stays transparent.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// Elevation archive (Terrarium encoding, WebP or PNG tiles)
    input: PathBuf,
    /// Slope archive to write
    output: PathBuf,
    /// Zoom level to compute slope at; 13 means about 6.5 m per pixel at 47° N with 512 px tiles
    #[arg(long, default_value_t = 13)]
    zoom: u8,
    /// Lowest zoom level to write
    #[arg(long, default_value_t = 9)]
    min_zoom: u8,
    /// Worker threads (default: all cores)
    #[arg(long)]
    threads: Option<usize>,
}

fn main() -> ExitCode {
    let args = Args::parse();
    let started = Instant::now();
    let options = Options {
        zoom: args.zoom,
        min_zoom: args.min_zoom,
        threads: args.threads,
    };
    match run(&args.input, &args.output, &options) {
        Ok(summary) => {
            println!(
                "slope at zoom {} from {} elevation tiles; wrote zoom {}-{}, {} tiles, {:.1} MB in {:.1} s",
                summary.zoom,
                summary.tiles_computed,
                summary.min_zoom,
                summary.zoom,
                summary.tiles_written,
                summary.bytes as f64 / 1e6,
                started.elapsed().as_secs_f64()
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
