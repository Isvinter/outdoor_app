//! Routes without a router and time estimates for travel on foot.

use crate::model::{LatLon, Profile, Route, RoutingError};

/// Longest stretch between two points of a manual route, in metres.
const MANUAL_STEP_M: f64 = 50.0;

/// A route of straight lines between the waypoints. Long stretches are
/// subdivided so that elevation can be sampled along them.
pub fn manual_route(waypoints: &[LatLon]) -> Result<Route, RoutingError> {
    let Some(first) = waypoints.first().filter(|_| waypoints.len() >= 2) else {
        return Err(RoutingError::TooFewWaypoints);
    };
    let mut line = vec![*first];
    let mut distance_m = 0.0;
    for pair in waypoints.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        let length = from.distance_m(&to);
        distance_m += length;
        let steps = (length / MANUAL_STEP_M).ceil().max(1.0) as usize;
        for step in 1..steps {
            let t = step as f64 / steps as f64;
            line.push(LatLon {
                lat: from.lat + (to.lat - from.lat) * t,
                lon: from.lon + (to.lon - from.lon) * t,
            });
        }
        line.push(to);
    }
    Ok(Route {
        profile: Profile::Manual,
        waypoints: waypoints.to_vec(),
        line,
        distance_m,
        duration_s: None,
    })
}

/// Walking performance for [`din_time_s`]. The defaults are those of DIN 33466.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DinParams {
    pub horizontal_m_per_h: f64,
    pub ascent_m_per_h: f64,
    pub descent_m_per_h: f64,
}

impl Default for DinParams {
    fn default() -> Self {
        DinParams {
            horizontal_m_per_h: 4000.0,
            ascent_m_per_h: 300.0,
            descent_m_per_h: 500.0,
        }
    }
}

/// Hiking time in seconds after DIN 33466: the time for the distance and the
/// time for the vertical metres are computed separately; the larger one
/// counts fully, the smaller one by half.
pub fn din_time_s(distance_m: f64, ascent_m: f64, descent_m: f64, params: &DinParams) -> f64 {
    let horizontal_h = distance_m / params.horizontal_m_per_h;
    let vertical_h = ascent_m / params.ascent_m_per_h + descent_m / params.descent_m_per_h;
    (horizontal_h.max(vertical_h) + horizontal_h.min(vertical_h) / 2.0) * 3600.0
}
