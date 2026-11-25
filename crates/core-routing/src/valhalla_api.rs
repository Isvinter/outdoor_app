//! Translation between our route model and Valhalla's JSON API. Pure functions,
//! usable (and tested) without the Valhalla library.

use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::model::{LatLon, Profile, Route, RouteRequest, RoutingError, Turn, TurnKind};
use crate::turns::{angle_at, kind_for_angle};

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
        .enumerate()
        .map(|(index, point)| {
            let mut location = json!({ "lat": point.lat, "lon": point.lon });
            if let (0, Some(heading)) = (index, request.heading_deg) {
                location["heading"] = json!(heading.round() as i64);
                location["heading_tolerance"] = json!(60);
            }
            location
        })
        .collect();
    let mut costing_options = Map::new();
    costing_options.insert(costing.into(), Value::Object(options));
    Ok(json!({
        "locations": locations,
        "costing": costing,
        "costing_options": costing_options,
        "units": "kilometers",
        "directions_type": "maneuvers",
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
    #[serde(default)]
    maneuvers: Vec<Maneuver>,
}

/// The parts of a Valhalla maneuver we use.
#[derive(Deserialize)]
struct Maneuver {
    /// `DirectionsLeg_Maneuver_Type` in Valhalla's `directions.proto`.
    #[serde(rename = "type")]
    kind: u32,
    /// Index into the shape of the leg.
    begin_shape_index: usize,
    #[serde(default)]
    roundabout_exit_count: Option<u32>,
}

/// How far before and after a maneuver the direction is measured, in metres.
const TURN_REACH_M: f64 = 20.0;

/// Smallest bend at which Valhalla's "straight on" still counts as a turn.
const STRAIGHT_TURN_DEG: f64 = 45.0;

/// The turn for a Valhalla maneuver at `line[at]`, if it is one.
fn turn_for(maneuver: &Maneuver, line: &[LatLon], at: usize) -> Option<Turn> {
    // Numbers of `DirectionsLeg_Maneuver_Type` (Valhalla 3.x).
    let kind = match maneuver.kind {
        9 => TurnKind::SlightRight,
        16 => TurnKind::SlightLeft,
        10 => TurnKind::Right,
        15 => TurnKind::Left,
        11 => TurnKind::SharpRight,
        14 => TurnKind::SharpLeft,
        12 | 13 => TurnKind::UTurn,
        // Ramp, exit, stay, merge to the right.
        18 | 20 | 23 | 37 => TurnKind::KeepRight,
        19 | 21 | 24 | 38 => TurnKind::KeepLeft,
        26 => TurnKind::Roundabout,
        // Becomes, continue, ramp straight, stay straight, merge: a turn only
        // if the line bends noticeably there.
        7 | 8 | 17 | 22 | 25 => {
            let angle = angle_at(line, at, TURN_REACH_M)?;
            if angle.abs() < STRAIGHT_TURN_DEG {
                return None;
            }
            kind_for_angle(angle)?
        }
        _ => return None,
    };
    let exit = (kind == TurnKind::Roundabout)
        .then_some(maneuver.roundabout_exit_count)
        .flatten()
        .and_then(|count| u8::try_from(count).ok());
    Some(Turn { at, kind, exit })
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
    // Per leg: for each point of its shape, its index in `line`.
    let mut indices: Vec<Vec<usize>> = Vec::with_capacity(answer.trip.legs.len());
    for leg in &answer.trip.legs {
        let shape = decode_polyline6(&leg.shape)?;
        let mut leg_indices = Vec::with_capacity(shape.len());
        for point in shape {
            // Consecutive legs share the waypoint between them.
            if line.last() != Some(&point) {
                line.push(point);
            }
            leg_indices.push(line.len() - 1);
        }
        indices.push(leg_indices);
    }
    let turns = answer
        .trip
        .legs
        .iter()
        .zip(&indices)
        .flat_map(|(leg, leg_indices)| {
            leg.maneuvers.iter().filter_map(|maneuver| {
                let at = *leg_indices.get(maneuver.begin_shape_index)?;
                turn_for(maneuver, &line, at)
            })
        })
        .collect();
    Ok(Route {
        profile: request.profile,
        waypoints: request.waypoints.clone(),
        line,
        distance_m: answer.trip.summary.length * 1000.0,
        duration_s: Some(answer.trip.summary.time),
        turns: Some(turns),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{RouteOptions, Turn, TurnKind};

    /// A point `north_m` north and `east_m` east of a fixed origin.
    fn at(north_m: f64, east_m: f64) -> LatLon {
        let lat0: f64 = 47.7;
        LatLon {
            lat: lat0 + north_m / 111_195.0,
            lon: 11.75 + east_m / (111_195.0 * lat0.to_radians().cos()),
        }
    }

    /// `count` points in 10 m steps from `from` in the direction `heading_deg`
    /// (clockwise from north), without `from` itself.
    fn walk(from: (f64, f64), heading_deg: f64, count: u32) -> Vec<(f64, f64)> {
        let (sin, cos) = heading_deg.to_radians().sin_cos();
        (1..=count)
            .map(|i| {
                let d = f64::from(i) * 10.0;
                (from.0 + d * cos, from.1 + d * sin)
            })
            .collect()
    }

    /// Points from (north, east) metre pairs.
    fn points(pairs: &[(f64, f64)]) -> Vec<LatLon> {
        pairs.iter().map(|&(n, e)| at(n, e)).collect()
    }

    fn encode_polyline6(line: &[LatLon]) -> String {
        fn push(out: &mut String, delta: i64) {
            let mut value = if delta < 0 { !(delta << 1) } else { delta << 1 };
            while value >= 0x20 {
                out.push(char::from((0x20 | (value & 0x1f)) as u8 + 63));
                value >>= 5;
            }
            out.push(char::from(value as u8 + 63));
        }
        let mut out = String::new();
        let (mut lat, mut lon) = (0i64, 0i64);
        for point in line {
            let (plat, plon) = (
                (point.lat * 1e6).round() as i64,
                (point.lon * 1e6).round() as i64,
            );
            push(&mut out, plat - lat);
            push(&mut out, plon - lon);
            (lat, lon) = (plat, plon);
        }
        out
    }

    fn maneuver(kind: u32, begin: usize) -> Value {
        json!({ "type": kind, "begin_shape_index": begin })
    }

    /// A Valhalla answer with one leg per entry.
    fn answer(legs: &[(Vec<LatLon>, Option<Vec<Value>>)]) -> String {
        let legs: Vec<Value> = legs
            .iter()
            .map(|(line, maneuvers)| {
                let mut leg = json!({ "shape": encode_polyline6(line) });
                if let Some(maneuvers) = maneuvers {
                    leg["maneuvers"] = json!(maneuvers);
                }
                leg
            })
            .collect();
        json!({ "trip": { "legs": legs, "summary": { "length": 1.0, "time": 600.0 } } }).to_string()
    }

    fn request() -> RouteRequest {
        RouteRequest {
            waypoints: vec![at(0.0, 0.0), at(50.0, 100.0)],
            profile: Profile::Hike,
            options: RouteOptions::default(),
            heading_deg: None,
        }
    }

    fn turns_of(json: &str) -> Vec<Turn> {
        parse_valhalla_route(json, &request())
            .unwrap()
            .turns
            .unwrap()
    }

    /// 50 m north, then turning by `turn_deg` and going on for 50 m.
    fn bend(turn_deg: f64) -> Vec<LatLon> {
        let mut pairs = vec![(0.0, 0.0)];
        pairs.extend(walk((0.0, 0.0), 0.0, 5));
        pairs.extend(walk((50.0, 0.0), turn_deg, 5));
        points(&pairs)
    }

    #[test]
    fn costing_json_asks_for_maneuvers() {
        let json: Value = serde_json::from_str(&costing_json(&request()).unwrap()).unwrap();
        assert_eq!(json["directions_type"], "maneuvers");
    }

    #[test]
    fn maneuvers_become_turns() {
        // North to index 5, east to index 15.
        let mut pairs = vec![(0.0, 0.0)];
        pairs.extend(walk((0.0, 0.0), 0.0, 5));
        pairs.extend(walk((50.0, 0.0), 90.0, 10));
        let line = points(&pairs);
        let mut roundabout = maneuver(26, 10);
        roundabout["roundabout_exit_count"] = json!(2);
        let maneuvers = vec![
            maneuver(1, 0),
            maneuver(10, 5),
            roundabout,
            maneuver(8, 12),
            maneuver(4, 15),
        ];
        assert_eq!(
            turns_of(&answer(&[(line, Some(maneuvers))])),
            vec![
                Turn {
                    at: 5,
                    kind: TurnKind::Right,
                    exit: None
                },
                Turn {
                    at: 10,
                    kind: TurnKind::Roundabout,
                    exit: Some(2)
                },
            ]
        );
    }

    #[test]
    fn continue_with_sharp_angle_is_a_turn() {
        let maneuvers = || Some(vec![maneuver(1, 0), maneuver(8, 5), maneuver(4, 10)]);
        assert_eq!(
            turns_of(&answer(&[(bend(-90.0), maneuvers())])),
            vec![Turn {
                at: 5,
                kind: TurnKind::Left,
                exit: None
            }]
        );
        assert_eq!(turns_of(&answer(&[(bend(20.0), maneuvers())])), vec![]);
    }

    #[test]
    fn turn_indices_follow_deduplicated_line() {
        let first = points(&[(0.0, 0.0), (25.0, 0.0), (50.0, 0.0)]);
        // The second leg starts where the first ends, goes north to (80, 0)
        // and turns right there.
        let mut pairs = vec![(50.0, 0.0)];
        pairs.extend(walk((50.0, 0.0), 0.0, 3));
        pairs.extend(walk((80.0, 0.0), 90.0, 3));
        let second = points(&pairs);
        let json = answer(&[
            (first, Some(vec![maneuver(1, 0), maneuver(4, 2)])),
            (
                second,
                Some(vec![maneuver(1, 0), maneuver(10, 3), maneuver(4, 6)]),
            ),
        ]);
        let route = parse_valhalla_route(&json, &request()).unwrap();
        assert_eq!(route.line.len(), 9);
        let turns = route.turns.unwrap();
        assert_eq!(
            turns,
            vec![Turn {
                at: 5,
                kind: TurnKind::Right,
                exit: None
            }]
        );
        assert!(route.line[5].distance_m(&at(80.0, 0.0)) < 0.5);
    }

    #[test]
    fn answer_without_maneuvers_has_empty_turns() {
        assert_eq!(turns_of(&answer(&[(bend(90.0), None)])), vec![]);
    }
}
