//! Translation between our route model and Valhalla's JSON API. Pure functions,
//! usable (and tested) without the Valhalla library.

use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::model::{LatLon, Profile, Route, RouteRequest, RoutingError};

/// The profiles Valhalla computes.
pub const VALHALLA_PROFILES: [Profile; 5] = [
    Profile::RoadBike,
    Profile::Trekking,
    Profile::Mtb,
    Profile::Hike,
    Profile::Car,
];

/// Valhalla reports "no way there" as errors among all others; these are the
/// messages of its codes 170, 171 and 442.
fn is_no_route(message: &str) -> bool {
    ["No suitable edges", "No path could be found", "unreachable"]
        .iter()
        .any(|part| message.contains(part))
}

/// The error for a message of the Valhalla engine.
pub fn engine_error(message: &str) -> RoutingError {
    if is_no_route(message) {
        RoutingError::NoRoute(message.to_string())
    } else {
        RoutingError::Engine(message.to_string())
    }
}

/// The Valhalla route request for `request` as JSON.
pub fn costing_json(request: &RouteRequest) -> Result<String, RoutingError> {
    if request.waypoints.len() < 2 {
        return Err(RoutingError::TooFewWaypoints);
    }
    let (costing, bicycle_type) = match request.profile {
        Profile::RoadBike => ("bicycle", Some("road")),
        Profile::Trekking => ("bicycle", Some("hybrid")),
        Profile::Mtb => ("bicycle", Some("mountain")),
        Profile::Hike => ("pedestrian", None),
        Profile::Car => ("auto", None),
        Profile::Manual => return Err(RoutingError::NotRoutable(Profile::Manual)),
    };
    let mut options = Map::new();
    if let Some(bicycle_type) = bicycle_type {
        options.insert("bicycle_type".into(), json!(bicycle_type));
    }
    for spec in request.profile.options() {
        let value = request.options.get(spec.key).unwrap_or(spec.default);
        if !(spec.min..=spec.max).contains(&value) {
            return Err(RoutingError::InvalidOption(format!(
                "{} must be between {} and {}",
                spec.key, spec.min, spec.max
            )));
        }
        // Whole-numbered settings (difficulty grades) are integers for Valhalla.
        let value = if spec.step >= 1.0 {
            json!(value.round() as i64)
        } else {
            json!(value)
        };
        options.insert(spec.key.into(), value);
    }
    // Settings the profile does not have are still checked, so that a typo
    // in a caller cannot slip through as "ignored".
    let shares = [
        request.options.use_hills,
        request.options.use_roads,
        request.options.avoid_bad_surfaces,
    ];
    if shares
        .iter()
        .flatten()
        .any(|value| !(0.0..=1.0).contains(value))
        || request
            .options
            .max_hiking_difficulty
            .is_some_and(|grade| !(1..=6).contains(&grade))
    {
        return Err(RoutingError::InvalidOption("value out of range".into()));
    }
    let locations: Vec<Value> = request
        .waypoints
        .iter()
        .map(|point| json!({ "lat": point.lat, "lon": point.lon }))
        .collect();
    let mut costing_options = Map::new();
    costing_options.insert(costing.into(), Value::Object(options));
    Ok(json!({
        "locations": locations,
        "costing": costing,
        "costing_options": costing_options,
        "units": "kilometers",
        "directions_type": "none",
    })
    .to_string())
}

/// Decodes an encoded polyline with six decimal places, as Valhalla writes shapes.
pub fn decode_polyline6(encoded: &str) -> Result<Vec<LatLon>, RoutingError> {
    let damaged = || RoutingError::Engine("damaged route geometry".into());
    let mut bytes = encoded.bytes();
    let mut next_delta = || -> Result<Option<i64>, RoutingError> {
        let mut value = 0i64;
        for shift in (0..60).step_by(5) {
            let Some(byte) = bytes.next() else {
                return if shift == 0 { Ok(None) } else { Err(damaged()) };
            };
            if !(63..127).contains(&byte) {
                return Err(damaged());
            }
            let chunk = i64::from(byte - 63);
            value |= (chunk & 0x1f) << shift;
            if chunk < 0x20 {
                return Ok(Some(if value & 1 == 1 {
                    !(value >> 1)
                } else {
                    value >> 1
                }));
            }
        }
        Err(damaged())
    };
    let mut points = Vec::with_capacity(encoded.len() / 4);
    let (mut lat, mut lon) = (0i64, 0i64);
    while let Some(delta) = next_delta()? {
        lat += delta;
        lon += next_delta()?.ok_or_else(damaged)?;
        points.push(LatLon {
            lat: lat as f64 / 1e6,
            lon: lon as f64 / 1e6,
        });
    }
    Ok(points)
}

#[derive(Deserialize)]
struct Answer {
    trip: Trip,
}

#[derive(Deserialize)]
struct Trip {
    legs: Vec<Leg>,
    summary: Summary,
}

#[derive(Deserialize)]
struct Leg {
    shape: String,
}

#[derive(Deserialize)]
struct Summary {
    /// Kilometres.
    length: f64,
    /// Seconds.
    time: f64,
}

/// Turns Valhalla's answer to `request` into a [`Route`].
pub fn parse_valhalla_route(json: &str, request: &RouteRequest) -> Result<Route, RoutingError> {
    let answer: Answer = serde_json::from_str(json)
        .map_err(|e| RoutingError::Engine(format!("unexpected answer: {e}")))?;
    let mut line: Vec<LatLon> = Vec::new();
    for leg in &answer.trip.legs {
        for point in decode_polyline6(&leg.shape)? {
            // Consecutive legs share the waypoint between them.
            if line.last() != Some(&point) {
                line.push(point);
            }
        }
    }
    Ok(Route {
        profile: request.profile,
        waypoints: request.waypoints.clone(),
        line,
        distance_m: answer.trip.summary.length * 1000.0,
        duration_s: Some(answer.trip.summary.time),
    })
}
