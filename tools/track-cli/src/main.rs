//! Developer CLI: reads a GPX file and prints track statistics.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use core_gpx::{GpxError, parse_gpx};
use core_track::{Activity, StatsParams, TrackStats, compute_stats};
use serde_json::json;
use thiserror::Error;

#[derive(Parser)]
#[command(version, about = "GPX in, statistics out")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the statistics of every track in a GPX file
    Stats {
        /// GPX file to read
        file: PathBuf,
        /// Activity (bike, mtb, hike, ski_tour); overrides the `<type>` in the file
        #[arg(long)]
        activity: Option<Activity>,
        /// Print JSON instead of text
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Error)]
enum CliError {
    #[error("cannot read {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path}: {source}")]
    Gpx { path: PathBuf, source: GpxError },
    #[error(
        "track {index} does not name a known activity; pass --activity <bike|mtb|hike|ski_tour>"
    )]
    UnknownActivity { index: usize },
    #[error("cannot encode output: {0}")]
    Json(#[from] serde_json::Error),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Stats {
            file,
            activity,
            json,
        } => stats(&file, activity, json),
    };
    match result {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn stats(file: &Path, activity: Option<Activity>, as_json: bool) -> Result<String, CliError> {
    let xml = std::fs::read_to_string(file).map_err(|source| CliError::Read {
        path: file.to_owned(),
        source,
    })?;
    let doc = parse_gpx(&xml).map_err(|source| CliError::Gpx {
        path: file.to_owned(),
        source,
    })?;

    let mut results = Vec::new();
    for (i, track) in doc.tracks.iter().enumerate() {
        let activity = activity
            .or(track.activity)
            .ok_or(CliError::UnknownActivity { index: i + 1 })?;
        let stats = compute_stats(track, &StatsParams::for_activity(activity));
        results.push((track.name.as_deref(), activity, stats));
    }

    if as_json {
        let items: Vec<_> = results
            .iter()
            .map(|(name, activity, stats)| json!({ "name": name, "activity": activity, "stats": stats }))
            .collect();
        return Ok(serde_json::to_string_pretty(&items)?);
    }
    if results.is_empty() {
        return Ok("no tracks in file".to_owned());
    }
    let blocks: Vec<_> = results
        .iter()
        .map(|(name, activity, stats)| render(*name, *activity, stats))
        .collect();
    Ok(blocks.join("\n\n"))
}

fn render(name: Option<&str>, activity: Activity, s: &TrackStats) -> String {
    let ele = |e: Option<f64>| e.map_or("-".to_owned(), |e| format!("{e:.0} m"));
    [
        format!("Track:         {}", name.unwrap_or("(unnamed)")),
        format!("Activity:      {}", activity.as_str()),
        format!("Distance:      {:.2} km", s.distance_m / 1000.0),
        format!("Ascent:        {:.0} m", s.ascent_m),
        format!("Descent:       {:.0} m", s.descent_m),
        format!(
            "Elevation:     {} to {}",
            ele(s.min_ele_m),
            ele(s.max_ele_m)
        ),
        format!("Total time:    {}", hms(s.total_time_s)),
        format!("Moving time:   {}", hms(s.moving_time_s)),
        format!("Avg (moving):  {:.1} km/h", s.avg_moving_speed_mps * 3.6),
        format!("Avg (total):   {:.1} km/h", s.avg_total_speed_mps * 3.6),
        format!("Max speed:     {:.1} km/h", s.max_speed_mps * 3.6),
        format!(
            "Points:        {} of {} used",
            s.points_used, s.points_total
        ),
    ]
    .join("\n")
}

fn hms(seconds: f64) -> String {
    let total = seconds.round() as u64;
    format!("{}:{:02}:{:02}", total / 3600, total / 60 % 60, total % 60)
}
