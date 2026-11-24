//! Routing on real Valhalla tiles (`fixtures/routing/tegernsee.tar`, built by
//! `scripts/build-valhalla-tiles.sh fixture-tegernsee` with the default
//! `--max-mtb-scale 2`).
#![cfg(feature = "valhalla")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use core_routing::{
    LatLon, Profile, RouteOptions, RouteRequest, Router, RoutingError, ValhallaRouter,
};

const STATION: LatLon = LatLon {
    lat: 47.7128,
    lon: 11.7580,
};
const NEUREUTH: LatLon = LatLon {
    lat: 47.7294,
    lon: 11.7797,
};
/// Both ends of the "Sommerweg" path (OSM way 126301802): 663 m, `mtb:scale=3`, `bicycle=yes`.
const SOMMERWEG: [LatLon; 2] = [
    LatLon {
        lat: 47.7245978,
        lon: 11.7556286,
    },
    LatLon {
        lat: 47.7246711,
        lon: 11.7595678,
    },
];

fn router() -> ValhallaRouter {
    let tiles =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/routing/tegernsee.tar");
    ValhallaRouter::open(&tiles).unwrap()
}

fn request(profile: Profile, from: LatLon, to: LatLon) -> RouteRequest {
    RouteRequest {
        waypoints: vec![from, to],
        profile,
        options: RouteOptions::default(),
        heading_deg: None,
    }
}

#[test]
fn hike_from_the_station_to_the_neureuth_hut() {
    let route = router()
        .route(&request(Profile::Hike, STATION, NEUREUTH))
        .unwrap();
    assert!(
        (3000.0..5000.0).contains(&route.distance_m),
        "{} m",
        route.distance_m
    );
    assert!(route.duration_s.unwrap() > 1800.0);
    assert!(route.line.len() > 20);
    assert!(route.line[0].distance_m(&STATION) < 60.0);
    // The hut stands a little off the path.
    assert!(route.line[route.line.len() - 1].distance_m(&NEUREUTH) < 150.0);
    // The line is continuous: no jumps between consecutive points.
    assert!(
        route
            .line
            .windows(2)
            .all(|w| w[0].distance_m(&w[1]) < 500.0)
    );
}

#[test]
fn bike_profiles_reach_the_hut_and_cars_do_not() {
    let router = router();
    for profile in [Profile::RoadBike, Profile::Trekking, Profile::Mtb] {
        let route = router.route(&request(profile, STATION, NEUREUTH));
        assert!(route.is_ok(), "{profile:?}: {route:?}");
    }
    let car = router.route(&request(Profile::Car, STATION, NEUREUTH));
    assert!(matches!(car, Err(RoutingError::NoRoute(_))), "{car:?}");
    let town = LatLon {
        lat: 47.7075,
        lon: 11.7575,
    };
    let car = router.route(&request(Profile::Car, STATION, town)).unwrap();
    assert!(
        (300.0..3000.0).contains(&car.distance_m),
        "{} m",
        car.distance_m
    );
}

#[test]
fn hiking_grade_limit_keeps_off_harder_paths() {
    // Both ends of a 1162 m path tagged sac_scale=mountain_hiking (grade 2, OSM way 15915757).
    let (from, to) = (
        LatLon {
            lat: 47.7067589,
            lon: 11.7811793,
        },
        LatLon {
            lat: 47.704899,
            lon: 11.7671699,
        },
    );
    let router = router();
    let default = router.route(&request(Profile::Hike, from, to)).unwrap();
    assert!(
        (1100.0..1250.0).contains(&default.distance_m),
        "{} m",
        default.distance_m
    );
    let mut easy = request(Profile::Hike, from, to);
    easy.options.max_hiking_difficulty = Some(1);
    match router.route(&easy) {
        Ok(easy) => assert!(
            easy.distance_m > default.distance_m * 1.2,
            "{} m",
            easy.distance_m
        ),
        Err(RoutingError::NoRoute(_)) => {}
        Err(other) => panic!("{other}"),
    }
}

#[test]
fn liking_hills_takes_the_direct_steep_way() {
    let router = router();
    let mut steep = request(Profile::Hike, STATION, NEUREUTH);
    steep.options.use_hills = Some(1.0);
    let mut gentle = request(Profile::Hike, STATION, NEUREUTH);
    gentle.options.use_hills = Some(0.0);
    let (steep, gentle) = (
        router.route(&steep).unwrap(),
        router.route(&gentle).unwrap(),
    );
    assert!(
        gentle.distance_m > steep.distance_m + 300.0,
        "{} vs {}",
        gentle.distance_m,
        steep.distance_m
    );
}

#[test]
fn intermediate_waypoints_are_visited() {
    let via = LatLon {
        lat: 47.7190,
        lon: 11.7560,
    };
    let mut req = request(Profile::Hike, STATION, NEUREUTH);
    req.waypoints.insert(1, via);
    let route = router().route(&req).unwrap();
    let nearest = route
        .line
        .iter()
        .map(|p| p.distance_m(&via))
        .fold(f64::MAX, f64::min);
    assert!(nearest < 60.0, "{nearest} m from the intermediate point");
}

#[test]
fn trails_harder_than_s2_are_closed_to_bikes_but_open_to_hikers() {
    let router = router();
    let hike = router
        .route(&request(Profile::Hike, SOMMERWEG[0], SOMMERWEG[1]))
        .unwrap();
    assert!(
        (600.0..750.0).contains(&hike.distance_m),
        "hikers walk the path: {} m",
        hike.distance_m
    );
    match router.route(&request(Profile::Mtb, SOMMERWEG[0], SOMMERWEG[1])) {
        Ok(mtb) => assert!(
            mtb.distance_m > hike.distance_m * 1.3,
            "bikes go around: {} m",
            mtb.distance_m
        ),
        Err(RoutingError::NoRoute(_)) => {}
        Err(other) => panic!("{other}"),
    }
}

#[test]
fn places_without_routing_data_are_reported_as_no_route() {
    let far = LatLon {
        lat: 48.5,
        lon: 12.5,
    };
    let result = router().route(&request(Profile::Hike, STATION, far));
    assert!(
        matches!(result, Err(RoutingError::NoRoute(_))),
        "{result:?}"
    );
}

#[test]
fn missing_tile_file_is_reported_as_unavailable() {
    let result = ValhallaRouter::open(&PathBuf::from("/nonexistent/routing.tar"));
    assert!(matches!(result, Err(RoutingError::Unavailable(_))));
}
