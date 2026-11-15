//! Derives contour line vector tiles from Terrarium elevation tiles (PMTiles in, PMTiles out).

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use clap::Parser;
use contour_tiles::{Options, run};

/// Contour lines as Mapbox Vector Tiles: layer `contours`, properties `ele`
/// (metres) and `level` (1 = major line), at the intervals the desktop draws.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// Elevation archive (Terrarium encoding, WebP or PNG tiles)
    input: PathBuf,
    /// Contour archive to write
    output: PathBuf,
    /// Lowest zoom level to write
    #[arg(long, default_value_t = 11)]
    min_zoom: u8,
    /// Highest zoom level to write (the map styles expect 14 and enlarge above)
    #[arg(long, default_value_t = core_maps::CONTOUR_MAX_ZOOM)]
    max_zoom: u8,
    /// Only this box: west,south,east,north
    #[arg(long, value_delimiter = ',')]
    bounds: Option<Vec<f64>>,
    /// Worker threads (default: all cores)
    #[arg(long)]
    threads: Option<usize>,
}

fn main() -> ExitCode {
    let args = Args::parse();
    let started = Instant::now();
    let bounds = match args.bounds.as_deref() {
        None => None,
        Some(&[west, south, east, north]) => Some([west, south, east, north]),
        Some(_) => {
            eprintln!("error: --bounds needs four numbers: west,south,east,north");
            return ExitCode::FAILURE;
        }
    };
    let options = Options {
        min_zoom: args.min_zoom,
        max_zoom: args.max_zoom,
        threads: args.threads,
        bounds,
    };
    match run(&args.input, &args.output, &options) {
        Ok(summary) => {
            println!(
                "contours at zoom {}-{} (elevation up to {}): {} tiles, {:.1} MB in {:.1} s",
                args.min_zoom,
                args.max_zoom,
                summary.max_dem_zoom,
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
