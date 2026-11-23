//! Writes the search file of a region package from an osmium GeoJSON-sequence export.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use clap::Parser;
use search_index::build;

/// Input is the output of `osmium export --add-unique-id=type_id -f geojsonseq
/// --geometry-types=point,linestring,polygon`.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// GeoJSON-sequence export of the region
    input: PathBuf,
    /// Search file to write (SQLite)
    output: PathBuf,
    /// Where to write the counts for the manifest (`search.json`)
    #[arg(long)]
    info: PathBuf,
    /// Time stamp of the OSM data, stored in the file
    #[arg(long)]
    osm_timestamp: Option<String>,
}

fn run(args: &Args) -> Result<String, String> {
    let started = Instant::now();
    let info = build(&args.input, &args.output, args.osm_timestamp.as_deref())
        .map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(&info).map_err(|e| e.to_string())?;
    std::fs::write(&args.info, json)
        .map_err(|e| format!("cannot write {}: {e}", args.info.display()))?;
    Ok(format!(
        "search file: {} places, {} streets, {} POIs ({} named) in {:.1} s",
        info.places,
        info.streets,
        info.pois,
        info.named_pois,
        started.elapsed().as_secs_f64()
    ))
}

fn main() -> ExitCode {
    match run(&Args::parse()) {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
