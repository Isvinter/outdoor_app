#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use core_routing::{
    DinParams, LatLon, Profile, Route, RouteOptions, RouteRequest, RoutingError, costing_json,
    decode_polyline6, din_time_s, manual_route, parse_valhalla_route,
};

fn ll(lat: f64, lon: f64) -> LatLon {
    LatLon { lat, lon }
}

fn request(profile: Profile) -> RouteRequest {
    RouteRequest {
        waypoints: vec![ll(47.7128, 11.758), ll(47.7294, 11.7797)],
        profile,
        options: RouteOptions::default(),
    }
}

fn costing(request: &RouteRequest) -> serde_json::Value {
    serde_json::from_str(&costing_json(request).unwrap()).unwrap()
}

// --- profiles -----------------------------------------------------------------

#[test]
fn profile_names_round_trip() {
    for profile in Profile::ALL {
        assert_eq!(profile.as_str().parse::<Profile>().ok(), Some(profile));
    }
    assert!("hovercraft".parse::<Profile>().is_err());
    assert_eq!(Profile::RoadBike.as_str(), "road_bike");
}

#[test]
fn mtb_profile_asks_for_a_mountain_bike_that_likes_trails_and_hills() {
    let json = costing(&request(Profile::Mtb));
    assert_eq!(json["costing"], "bicycle");
    let options = &json["costing_options"]["bicycle"];
    assert_eq!(options["bicycle_type"], "mountain");
    assert_eq!(options["avoid_bad_surfaces"], 0.0);
    assert!(options["use_roads"].as_f64().unwrap() < 0.2);
    assert!(options["use_hills"].as_f64().unwrap() > 0.7);
    assert_eq!(json["locations"][1]["lat"], 47.7294);
    assert_eq!(json["locations"][1]["lon"], 11.7797);
    assert_eq!(json["units"], "kilometers");
}

#[test]
fn road_and_trekking_bikes_differ_in_type_and_surface_tolerance() {
    let road = costing(&request(Profile::RoadBike));
    let trekking = costing(&request(Profile::Trekking));
    assert_eq!(road["costing_options"]["bicycle"]["bicycle_type"], "road");
    assert_eq!(
        trekking["costing_options"]["bicycle"]["bicycle_type"],
        "hybrid"
    );
    let avoid = |v: &serde_json::Value| {
        v["costing_options"]["bicycle"]["avoid_bad_surfaces"]
            .as_f64()
            .unwrap()
    };
    assert!(avoid(&road) > avoid(&trekking));
}

#[test]
fn hiking_profile_allows_mountain_paths_by_default() {
    let json = costing(&request(Profile::Hike));
    assert_eq!(json["costing"], "pedestrian");
    assert_eq!(
        json["costing_options"]["pedestrian"]["max_hiking_difficulty"],
        3
    );
}

#[test]
fn car_profile_uses_the_plain_car_costing() {
    assert_eq!(costing(&request(Profile::Car))["costing"], "auto");
}

#[test]
fn options_override_the_defaults_of_a_profile() {
    let mut req = request(Profile::Hike);
    req.options = RouteOptions {
        max_hiking_difficulty: Some(5),
        use_hills: Some(0.2),
        ..Default::default()
    };
    let json = costing(&req);
    assert_eq!(
        json["costing_options"]["pedestrian"]["max_hiking_difficulty"],
        5
    );
    assert_eq!(json["costing_options"]["pedestrian"]["use_hills"], 0.2);

    let mut req = request(Profile::Mtb);
    req.options = RouteOptions {
        use_roads: Some(0.9),
        avoid_bad_surfaces: Some(0.5),
        ..Default::default()
    };
    let json = costing(&req);
    assert_eq!(json["costing_options"]["bicycle"]["use_roads"], 0.9);
    assert_eq!(
        json["costing_options"]["bicycle"]["avoid_bad_surfaces"],
        0.5
    );
    assert_eq!(
        json["costing_options"]["bicycle"]["bicycle_type"],
        "mountain"
    );
}

#[test]
fn options_outside_their_range_are_rejected() {
    for options in [
        RouteOptions {
            use_hills: Some(1.5),
            ..Default::default()
        },
        RouteOptions {
            use_roads: Some(-0.1),
            ..Default::default()
        },
        RouteOptions {
            avoid_bad_surfaces: Some(f64::NAN),
            ..Default::default()
        },
        RouteOptions {
            max_hiking_difficulty: Some(7),
            ..Default::default()
        },
    ] {
        let mut req = request(Profile::Hike);
        req.options = options;
        assert!(
            matches!(costing_json(&req), Err(RoutingError::InvalidOption(_))),
            "{:?}",
            req.options
        );
    }
}

#[test]
fn a_route_needs_two_waypoints_and_the_manual_profile_no_router() {
    let mut req = request(Profile::Mtb);
    req.waypoints.truncate(1);
    assert!(matches!(
        costing_json(&req),
        Err(RoutingError::TooFewWaypoints)
    ));
    assert!(matches!(
        costing_json(&request(Profile::Manual)),
        Err(RoutingError::NotRoutable(Profile::Manual))
    ));
}

#[test]
fn every_profile_describes_its_adjustable_options() {
    let keys = |profile: Profile| profile.options().iter().map(|o| o.key).collect::<Vec<_>>();
    assert_eq!(
        keys(Profile::Mtb),
        ["use_hills", "use_roads", "avoid_bad_surfaces"]
    );
    assert_eq!(keys(Profile::Hike), ["max_hiking_difficulty", "use_hills"]);
    assert!(keys(Profile::Manual).is_empty());
    let difficulty = Profile::Hike.options()[0];
    assert_eq!(
        (
            difficulty.min,
            difficulty.max,
            difficulty.default,
            difficulty.step
        ),
        (1.0, 6.0, 3.0, 1.0)
    );
}

// --- geometry -----------------------------------------------------------------

fn encode_polyline6(points: &[LatLon]) -> String {
    let mut out = String::new();
    let (mut last_lat, mut last_lon) = (0i64, 0i64);
    for p in points {
        let (lat, lon) = ((p.lat * 1e6).round() as i64, (p.lon * 1e6).round() as i64);
        for delta in [lat - last_lat, lon - last_lon] {
            let mut value = if delta < 0 { !(delta << 1) } else { delta << 1 };
            while value >= 0x20 {
                out.push((((value & 0x1f) | 0x20) as u8 + 63) as char);
                value >>= 5;
            }
            out.push((value as u8 + 63) as char);
        }
        (last_lat, last_lon) = (lat, lon);
    }
    out
}

#[test]
fn polylines_with_six_decimals_are_decoded() {
    let points = [
        ll(47.712791, 11.758106),
        ll(47.7131, 11.7579),
        ll(-33.5, -70.25),
        ll(0.0, 0.0),
    ];
    assert_eq!(
        decode_polyline6(&encode_polyline6(&points)).unwrap(),
        points
    );
    // Start of a shape Valhalla returned for a route from Tegernsee station.
    let first = decode_polyline6("o`d_zAw`tlU").unwrap()[0];
    assert!(
        (first.lat - 47.7128).abs() < 1e-4 && (first.lon - 11.7581).abs() < 1e-4,
        "{first:?}"
    );
    assert_eq!(decode_polyline6("").unwrap(), []);
}

#[test]
fn damaged_polylines_are_rejected() {
    assert!(
        matches!(decode_polyline6("o`d_zA"), Err(RoutingError::Engine(_))),
        "latitude without longitude"
    );
    assert!(matches!(
        decode_polyline6("o`d_zAw`tl\u{7f}"),
        Err(RoutingError::Engine(_))
    ));
    assert!(matches!(
        decode_polyline6("o`d_zAw`tl "),
        Err(RoutingError::Engine(_))
    ));
}

#[test]
fn valhalla_answer_becomes_a_route() {
    let leg1 = [
        ll(47.7128, 11.7580),
        ll(47.7150, 11.7600),
        ll(47.7200, 11.7700),
    ];
    let leg2 = [ll(47.7200, 11.7700), ll(47.7294, 11.7797)];
    let answer = serde_json::json!({
        "trip": {
            "legs": [
                { "shape": encode_polyline6(&leg1), "summary": { "length": 1.2, "time": 300.0 } },
                { "shape": encode_polyline6(&leg2), "summary": { "length": 1.3, "time": 400.5 } }
            ],
            "summary": { "length": 2.5, "time": 700.5 },
            "status": 0
        }
    });
    let mut req = request(Profile::Trekking);
    req.waypoints.insert(1, ll(47.72, 11.77));

    let route = parse_valhalla_route(&answer.to_string(), &req).unwrap();

    assert_eq!(route.profile, Profile::Trekking);
    assert_eq!(route.waypoints, req.waypoints);
    assert_eq!(route.distance_m, 2500.0);
    assert_eq!(route.duration_s, Some(700.5));
    assert_eq!(
        route.line.len(),
        4,
        "the point shared by both legs appears once"
    );
    assert_eq!(route.line[2], ll(47.72, 11.77));
    assert_eq!(route.line[3], ll(47.7294, 11.7797));
}

#[test]
fn valhalla_answer_without_a_trip_is_an_error() {
    let req = request(Profile::Mtb);
    assert!(matches!(
        parse_valhalla_route("{}", &req),
        Err(RoutingError::Engine(_))
    ));
    assert!(matches!(
        parse_valhalla_route("not json", &req),
        Err(RoutingError::Engine(_))
    ));
}

// --- manual routes and time ----------------------------------------------------

#[test]
fn manual_route_connects_its_waypoints_with_straight_lines() {
    // 0.01° of latitude is 1111.95 m on the sphere used for distances.
    let points = vec![ll(47.50, 11.5), ll(47.51, 11.5), ll(47.53, 11.5)];
    let route: Route = manual_route(&points).unwrap();
    assert_eq!(route.profile, Profile::Manual);
    assert!(
        (route.distance_m - 3335.85).abs() < 1.0,
        "{}",
        route.distance_m
    );
    assert_eq!(route.duration_s, None);
    assert_eq!(route.waypoints, points);
    assert_eq!(route.line.first(), points.first());
    assert_eq!(route.line.last(), points.last());
    // Long stretches get intermediate points so that an elevation profile can follow the terrain.
    assert!(route.line.len() > 30, "{} points", route.line.len());
    let longest = route
        .line
        .windows(2)
        .map(|w| w[0].distance_m(&w[1]))
        .fold(0.0, f64::max);
    assert!(longest <= 51.0, "{longest}");
    assert!(matches!(
        manual_route(&points[..1]),
        Err(RoutingError::TooFewWaypoints)
    ));
}

#[test]
fn din_33466_combines_horizontal_and_vertical_time() {
    let din = DinParams::default();
    // 8 km flat: 2 h.
    assert_eq!(din_time_s(8000.0, 0.0, 0.0, &din), 7200.0);
    // 4 km and 600 m up: 1 h horizontal, 2 h vertical -> 2 h + 0.5 h.
    assert_eq!(din_time_s(4000.0, 600.0, 0.0, &din), 9000.0);
    // 12 km, 300 m up, 500 m down: 3 h horizontal, 2 h vertical -> 3 h + 1 h.
    assert_eq!(din_time_s(12000.0, 300.0, 500.0, &din), 14400.0);
    let fit = DinParams {
        horizontal_m_per_h: 5000.0,
        ascent_m_per_h: 400.0,
        descent_m_per_h: 600.0,
    };
    assert!(din_time_s(4000.0, 600.0, 0.0, &fit) < 9000.0);
}

#[test]
fn colors_match_desktop() {
    let desktop = include_str!("../../../apps/base/ui/src/profile-colors.ts");
    for profile in Profile::ALL {
        let line = format!("{}: \"{}\"", profile.as_str(), profile.color());
        assert!(desktop.contains(&line), "{line} not in profile-colors.ts");
    }
    assert!(desktop.contains(&format!(
        "FAILED_COLOR = \"{}\"",
        core_routing::FAILED_LEG_COLOR
    )));
}
