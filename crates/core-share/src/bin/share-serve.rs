//! Serves a data directory like the desktop share does, for tests of the
//! mobile apps against a real server.
//!
//! `share-serve --data <dir> [--port 8765] [--seed-region <package>]
//! [--seed-route <name>=<lat>,<lon>;<lat>,<lon>[;…]] [--throttle <KiB/s>]`

use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use core_app::{App, AppListener, LegSpec, PlanRequest, RecordingStatus};
use core_routing::{LatLon, Profile, RouteOptions};
use core_share::{ShareOptions, ShareServer, share_url};

const USAGE: &str = "usage: share-serve --data <dir> [--port 8765] [--seed-region <package>] \
                     [--seed-route <name>=<lat>,<lon>;<lat>,<lon>[;...]] [--throttle <KiB/s>]";

#[derive(Default)]
struct Args {
    data: Option<PathBuf>,
    port: Option<u16>,
    regions: Vec<PathBuf>,
    routes: Vec<(String, Vec<LatLon>)>,
    throttle_kib_s: Option<u64>,
}

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut parsed = Args::default();
    while let Some(flag) = args.next() {
        let mut value = || args.next().ok_or(format!("{flag} needs a value"));
        match flag.as_str() {
            "--data" => parsed.data = Some(PathBuf::from(value()?)),
            "--port" => {
                parsed.port = Some(value()?.parse().map_err(|e| format!("--port: {e}"))?);
            }
            "--seed-region" => parsed.regions.push(PathBuf::from(value()?)),
            "--seed-route" => parsed.routes.push(parse_route(&value()?)?),
            "--throttle" => {
                parsed.throttle_kib_s =
                    Some(value()?.parse().map_err(|e| format!("--throttle: {e}"))?);
            }
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    Ok(parsed)
}

/// `<name>=<lat>,<lon>;<lat>,<lon>[;…]`
fn parse_route(spec: &str) -> Result<(String, Vec<LatLon>), String> {
    let fail = || format!("--seed-route {spec:?}: expected <name>=<lat>,<lon>;<lat>,<lon>");
    let (name, points) = spec.split_once('=').ok_or_else(fail)?;
    let points = points
        .split(';')
        .map(|point| {
            let (lat, lon) = point.split_once(',').ok_or_else(fail)?;
            Ok(LatLon {
                lat: lat.trim().parse().map_err(|_| fail())?,
                lon: lon.trim().parse().map_err(|_| fail())?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    if name.is_empty() || points.len() < 2 {
        return Err(fail());
    }
    Ok((name.to_string(), points))
}

struct Listener;

impl AppListener for Listener {
    fn on_position(&self, _: &core_location::Fix) {}
    fn on_recording(&self, _: &RecordingStatus) {}
    fn on_source_status(&self, _: &core_location::SourceStatus) {}
    fn on_error(&self, message: &str) {
        eprintln!("error: {message}");
    }
}

/// Copies a region package, so that installing it (which moves the files)
/// leaves the original in place.
fn copy_package(from: &Path, to: &Path) -> std::io::Result<()> {
    let _ = std::fs::remove_dir_all(to);
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            std::fs::copy(entry.path(), to.join(entry.file_name()))?;
        }
    }
    Ok(())
}

fn run(args: Args) -> Result<(), String> {
    let data = args.data.ok_or_else(|| USAGE.to_string())?;
    std::fs::create_dir_all(&data).map_err(|e| format!("{}: {e}", data.display()))?;
    let app = App::open(&data.join("outdoor.sqlite"), Arc::new(Listener))
        .map_err(|e| format!("opening the app: {e}"))?;

    for package in &args.regions {
        let copy = data.join("seed-package");
        copy_package(package, &copy).map_err(|e| format!("{}: {e}", package.display()))?;
        let region = app
            .install_local_region(&copy)
            .map_err(|e| format!("installing {}: {e}", package.display()))?;
        let _ = std::fs::remove_dir_all(&copy);
        println!("region {}", region.manifest.id);
    }

    for (name, waypoints) in args.routes {
        app.start_new_route();
        let legs = waypoints
            .windows(2)
            .map(|_| LegSpec {
                profile: Profile::Manual,
                options: RouteOptions::default(),
            })
            .collect();
        app.plan_route(&PlanRequest {
            waypoints,
            legs,
            stage_ends: Vec::new(),
        })
        .map_err(|e| format!("planning {name}: {e}"))?;
        let saved = app
            .save_planned_route(&name)
            .map_err(|e| format!("saving {name}: {e}"))?;
        println!("route {} {name}", saved.id);
    }

    let port = args.port.unwrap_or(8765);
    let options = ShareOptions {
        throttle_bytes_per_s: args.throttle_kib_s.map(|kib| kib * 1024),
    };
    let server = ShareServer::start(
        SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)),
        Arc::new(app),
        options,
    )
    .map_err(|e| format!("listening on port {port}: {e}"))?;
    println!(
        "serving {}",
        share_url(Ipv4Addr::LOCALHOST, server.local_addr().port())
    );
    // Runs until the process is ended (Ctrl-C).
    loop {
        std::thread::park();
    }
}

fn main() -> ExitCode {
    let result = parse_args(std::env::args().skip(1)).and_then(run);
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
