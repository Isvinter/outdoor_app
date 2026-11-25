//! Plans and positions for the guide's module tests.

use core_routing::{LatLon, Profile, RouteOptions};

use crate::plan::{Leg, LegError, LegSpec, PLAN_FORMAT, PlanRequest, RoutePlan, Totals};

/// Metres per degree of latitude on the mean Earth sphere.
const M_PER_DEG: f64 = core_track::geo::EARTH_RADIUS_M * std::f64::consts::PI / 180.0;

pub const SYLVENSTEIN: &str = include_str!("../../../../fixtures/gpx/real/tour_sylvensteinsee.gpx");
pub const OBERGRIES: &str =
    include_str!("../../../../fixtures/gpx/real/von Rottach-Egern nach Obergries.gpx");

pub fn track_points(gpx: &str) -> Vec<LatLon> {
    let doc = core_gpx::parse_gpx(gpx).unwrap();
    doc.tracks
        .iter()
        .flat_map(|t| &t.segments)
        .flat_map(|s| &s.points)
        .map(|p| LatLon {
            lat: p.lat,
            lon: p.lon,
        })
        .collect()
}

pub fn line_length(line: &[LatLon]) -> f64 {
    line.windows(2).map(|w| w[0].distance_m(&w[1])).sum()
}

pub fn leg(profile: Profile, line: Vec<LatLon>) -> Leg {
    Leg {
        profile,
        options: RouteOptions::default(),
        distance_m: line_length(&line),
        line,
        duration_s: None,
        ascent_m: None,
        descent_m: None,
        time_s: Some(60.0),
        region: None,
        max_mtb_scale: None,
        turns: None,
        error: None,
    }
}

pub fn failed_leg() -> Leg {
    Leg {
        error: Some(LegError {
            code: "route_not_found".into(),
            message: "no way".into(),
        }),
        time_s: None,
        ..leg(Profile::Hike, Vec::new())
    }
}

/// A plan of `legs` between `waypoints`, without stages or totals.
pub fn plan(waypoints: Vec<LatLon>, legs: Vec<Leg>, stage_ends: Vec<usize>) -> RoutePlan {
    RoutePlan {
        format: PLAN_FORMAT,
        request: PlanRequest {
            waypoints,
            legs: legs
                .iter()
                .map(|l| LegSpec {
                    profile: l.profile,
                    options: l.options,
                })
                .collect(),
            stage_ends,
        },
        legs,
        elevation: Vec::new(),
        stages: Vec::new(),
        total: Totals {
            distance_m: 0.0,
            ascent_m: None,
            descent_m: None,
            time_s: None,
            min_ele_m: None,
            max_ele_m: None,
            failed_legs: 0,
        },
    }
}

/// A plan whose legs follow `points`, split at `cuts`.
pub fn plan_along(points: &[LatLon], cuts: &[usize], stage_ends: Vec<usize>) -> RoutePlan {
    let mut bounds = vec![0];
    bounds.extend_from_slice(cuts);
    bounds.push(points.len() - 1);
    let waypoints = bounds.iter().map(|&i| points[i]).collect();
    let legs = bounds
        .windows(2)
        .map(|b| leg(Profile::Hike, points[b[0]..=b[1]].to_vec()))
        .collect();
    plan(waypoints, legs, stage_ends)
}

pub fn north(p: LatLon, m: f64) -> LatLon {
    LatLon {
        lat: p.lat + m / M_PER_DEG,
        lon: p.lon,
    }
}

pub fn east(p: LatLon, m: f64) -> LatLon {
    LatLon {
        lat: p.lat,
        lon: p.lon + m / (M_PER_DEG * p.lat.to_radians().cos()),
    }
}
