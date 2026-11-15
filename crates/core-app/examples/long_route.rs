//! Measures planning a long route leg by leg on a real region:
//! `cargo run --release -p core-app --features valhalla --example long_route -- <data dir> [points]`.
//! The data directory needs `regions/<id>` with routing and elevation data;
//! the example writes its own database there (use a scratch copy or symlinks).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use core_app::{App, AppListener, LegSpec, PlanRequest, RecordingStatus};
use core_location::{Fix, SourceStatus};
use core_routing::{LatLon, Profile, RouteOptions};

struct Quiet;
impl AppListener for Quiet {
    fn on_position(&self, _: &Fix) {}
    fn on_recording(&self, _: &RecordingStatus) {}
    fn on_source_status(&self, _: &SourceStatus) {}
    fn on_error(&self, _: &str) {}
}

/// Munich – Garmisch – Innsbruck – Bozen – Trento – Belluno – Cortina – Lienz – Villach – Salzburg.
const VIA: [(f64, f64); 10] = [
    (48.137, 11.575),
    (47.492, 11.095),
    (47.268, 11.393),
    (46.498, 11.354),
    (46.067, 11.121),
    (46.140, 12.217),
    (46.538, 12.136),
    (46.829, 12.769),
    (46.611, 13.846),
    (47.800, 13.044),
];

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().expect("data directory"));
    let count: usize = args.next().map_or(300, |n| n.parse().unwrap());
    let app = App::open(&dir.join("measure.sqlite"), Arc::new(Quiet)).unwrap();

    // `count` points evenly along the via points.
    let mut waypoints = Vec::with_capacity(count);
    let per = (count - 1) as f64 / (VIA.len() - 1) as f64;
    for i in 0..count {
        let at = i as f64 / per;
        let k = (at.floor() as usize).min(VIA.len() - 2);
        let f = at - k as f64;
        let (a, b) = (VIA[k], VIA[k + 1]);
        waypoints.push(LatLon {
            lat: a.0 + (b.0 - a.0) * f,
            lon: a.1 + (b.1 - a.1) * f,
        });
    }
    let leg = LegSpec {
        profile: Profile::Trekking,
        options: RouteOptions::default(),
    };
    let mut request = PlanRequest {
        legs: vec![leg; waypoints.len() - 1],
        waypoints,
        stage_ends: vec![count / 3, 2 * count / 3],
    };
    let last = request.waypoints.pop().unwrap();
    request.legs.pop();

    let started = Instant::now();
    let plan = app.plan_route(&request).unwrap();
    let cold = started.elapsed();
    request.waypoints.push(last);
    request.legs.push(leg);
    let started = Instant::now();
    let plan_more = app.plan_route(&request).unwrap();
    let warm = started.elapsed();
    let json = serde_json::to_string(&plan_more).unwrap();
    let started = Instant::now();
    let shown = serde_json::to_string(&plan_more.for_display(2000)).unwrap();
    let thinning = started.elapsed();

    println!(
        "points {} legs {}",
        request.waypoints.len(),
        request.legs.len()
    );
    println!(
        "cold: {:.2} s for {} legs",
        cold.as_secs_f64(),
        plan.legs.len()
    );
    println!("warm: {:.3} s for one more point", warm.as_secs_f64());
    println!(
        "route {:.0} km, ascent {:?} m, time {:?} h, failed legs {}",
        plan_more.total.distance_m / 1000.0,
        plan_more.total.ascent_m.map(f64::round),
        plan_more.total.time_s.map(|s| (s / 360.0).round() / 10.0),
        plan_more.total.failed_legs
    );
    println!(
        "elevation samples {}, line points {}, answer {:.1} MB",
        plan_more.elevation.len(),
        plan_more.legs.iter().map(|l| l.line.len()).sum::<usize>(),
        json.len() as f64 / 1e6
    );
    println!(
        "for display (2000 samples): {:.1} MB, thinned and serialised in {:.3} s",
        shown.len() as f64 / 1e6,
        thinning.as_secs_f64()
    );
    let no_ele: Vec<usize> = (0..plan_more.legs.len())
        .filter(|&i| plan_more.legs[i].error.is_none() && plan_more.legs[i].ascent_m.is_none())
        .collect();
    println!("legs without elevation: {no_ele:?}");
    if let Some(&i) = no_ele.first() {
        let missing = plan_more
            .elevation
            .iter()
            .filter(|p| p.leg == i && p.ele.is_none())
            .count();
        let all = plan_more.elevation.iter().filter(|p| p.leg == i).count();
        println!(
            "  leg {i}: {missing} of {all} samples without elevation, from {:?}",
            request.waypoints[i]
        );
    }
    for (i, leg) in plan_more
        .legs
        .iter()
        .enumerate()
        .filter(|(_, l)| l.error.is_some())
        .take(5)
    {
        println!("  leg {i}: {:?}", leg.error);
    }
}
