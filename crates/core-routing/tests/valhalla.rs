//! Routing on real Valhalla tiles (`fixtures/routing/tegernsee.tar`, built by
//! `scripts/build-valhalla-tiles.sh fixture-tegernsee` with the default
//! `--max-mtb-scale 2`).
#![cfg(feature = "valhalla")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use core_routing::{
    LatLon, MatrixRequest, Profile, RouteOptions, RouteRequest, Router, RoutingError,
    ValhallaRouter,
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
fn route_has_turns() {
    let route = router()
        .route(&request(Profile::Hike, STATION, NEUREUTH))
        .unwrap();
    let turns = route.turns.expect("turns are computed");
    assert!(!turns.is_empty());
    assert!(
        turns.iter().all(|turn| turn.at < route.line.len()),
        "{turns:?}"
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

fn matrix_request(profile: Profile, from: LatLon, to: Vec<LatLon>) -> MatrixRequest {
    MatrixRequest {
        from,
        to,
        profile,
        options: RouteOptions::default(),
    }
}

#[test]
fn matrix_matches_single_routes() {
    let router = router();
    let targets = [NEUREUTH, SOMMERWEG[0]];
    let entries = router
        .matrix(&matrix_request(Profile::Hike, STATION, targets.to_vec()))
        .unwrap();
    assert_eq!(entries.len(), 2);
    for (entry, target) in entries.iter().zip(targets) {
        let entry = entry.expect("routable");
        let route = router
            .route(&request(Profile::Hike, STATION, target))
            .unwrap();
        assert!(
            (entry.distance_m - route.distance_m).abs() <= route.distance_m * 0.1,
            "{} m vs {} m",
            entry.distance_m,
            route.distance_m
        );
        assert!(entry.duration_s > 0.0);
    }
}

#[test]
fn target_at_the_position_counts_as_routed() {
    let entries = router()
        .matrix(&matrix_request(Profile::Hike, STATION, vec![STATION]))
        .unwrap();
    let entry = entries[0].expect("routed");
    assert!(entry.distance_m < 100.0, "{} m", entry.distance_m);
}

#[test]
fn matrix_for_every_valhalla_profile() {
    let router = router();
    for profile in core_routing::VALHALLA_PROFILES {
        let entries = router.matrix(&matrix_request(profile, STATION, vec![NEUREUTH]));
        assert_eq!(entries.map(|e| e.len()).ok(), Some(1), "{profile:?}");
    }
}

/// `count` points evenly on a circle of `radius_km` around `centre`.
fn ring(centre: LatLon, radius_km: f64, count: u32) -> Vec<LatLon> {
    (0..count)
        .map(|i| {
            let angle = f64::from(i) / f64::from(count) * std::f64::consts::TAU;
            LatLon {
                lat: centre.lat + radius_km / 111.195 * angle.cos(),
                lon: centre.lon
                    + radius_km / (111.195 * centre.lat.to_radians().cos()) * angle.sin(),
            }
        })
        .collect()
}

/// Timing on a real region: `ZG_MATRIX_TILES=<routing.tar> cargo test --release
/// -p core-routing --features valhalla --test valhalla measure_matrix -- --ignored --nocapture`.
#[test]
#[ignore = "needs a real region in ZG_MATRIX_TILES"]
fn measure_matrix_on_real_tiles() {
    use std::time::Instant;

    let tiles = std::env::var("ZG_MATRIX_TILES").expect("ZG_MATRIX_TILES");
    let router = ValhallaRouter::open(&PathBuf::from(tiles)).unwrap();
    let bolzano = LatLon {
        lat: 46.4983,
        lon: 11.3548,
    };
    println!("profile  radius_km  median_ms  routed/30");
    let mut too_slow = Vec::new();
    for profile in [
        Profile::Hike,
        Profile::Trekking,
        Profile::Mtb,
        Profile::RoadBike,
        Profile::Car,
    ] {
        for radius_km in [10.0_f64, 25.0, 50.0] {
            let to = ring(bolzano, radius_km, 30);
            let req = matrix_request(profile, bolzano, to);
            router.matrix(&req).unwrap();
            let mut times = Vec::new();
            let mut routed = 0;
            for _ in 0..3 {
                let start = Instant::now();
                let entries = router.matrix(&req).unwrap();
                times.push(start.elapsed().as_secs_f64() * 1000.0);
                routed = entries.iter().flatten().count();
            }
            times.sort_by(f64::total_cmp);
            println!("{profile:?}  {radius_km}  {:.0}  {routed}", times[1]);
            // Targets beyond the router's reach come back as None, so the
            // routed count is only printed.
            if times[1] > 300.0 {
                too_slow.push(format!("{profile:?} at {radius_km} km: {:.0} ms", times[1]));
            }
        }
    }
    assert!(too_slow.is_empty(), "{too_slow:?}");
}

#[test]
fn target_outside_the_tiles_does_not_blank_the_others() {
    let outside = LatLon {
        lat: 48.5,
        lon: 12.5,
    };
    let entries = router()
        .matrix(&matrix_request(
            Profile::Hike,
            STATION,
            vec![NEUREUTH, outside],
        ))
        .unwrap();
    assert_eq!(entries.len(), 2);
    assert!(entries[0].is_some());
    assert!(entries[1].is_none(), "{:?}", entries[1]);
}

/// A search for a target without a way must not run on and on: batch of 29 targets at
/// 50 km around the source plus one point that bikes cannot reach.
///
/// Environment: `ZG_MATRIX_TILES` (routing.tar), optional `ZG_MATRIX_SOURCE` and
/// `ZG_MATRIX_UNREACHABLE` as `lat,lon` (default: Bozen and a point in the Dolomites, for
/// Südtirol). With `ZG_MATRIX_PRINT_ONLY=1` the point may be routable and nothing is asserted,
/// the timings are only printed (to estimate the worst case on a larger region).
#[test]
#[ignore = "needs a real region in ZG_MATRIX_TILES"]
fn matrix_with_an_unreachable_target_stays_fast() {
    use std::time::Instant;

    fn point(name: &str, default: LatLon) -> LatLon {
        std::env::var(name).map_or(default, |value| {
            let (lat, lon) = value.split_once(',').expect("lat,lon");
            LatLon {
                lat: lat.trim().parse().unwrap(),
                lon: lon.trim().parse().unwrap(),
            }
        })
    }
    let print_only = std::env::var("ZG_MATRIX_PRINT_ONLY").is_ok();
    let tiles = std::env::var("ZG_MATRIX_TILES").expect("ZG_MATRIX_TILES");
    let router = ValhallaRouter::open(&PathBuf::from(tiles)).unwrap();
    let bolzano = point(
        "ZG_MATRIX_SOURCE",
        LatLon {
            lat: 46.4983,
            lon: 11.3548,
        },
    );
    let unreachable = point(
        "ZG_MATRIX_UNREACHABLE",
        LatLon {
            lat: 46.65,
            lon: 12.27,
        },
    );
    println!("profile  median_ms  routed/30");
    for profile in [Profile::Trekking, Profile::Mtb] {
        let mut to = ring(bolzano, 50.0, 29);
        to.push(unreachable);
        let req = matrix_request(profile, bolzano, to);
        router.matrix(&req).unwrap();
        let mut times = Vec::new();
        let mut entries = Vec::new();
        for _ in 0..3 {
            let start = Instant::now();
            entries = router.matrix(&req).unwrap();
            times.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        times.sort_by(f64::total_cmp);
        println!(
            "{profile:?}  {:.0}  {}",
            times[1],
            entries.iter().flatten().count()
        );
        if !print_only {
            assert!(entries[29].is_none(), "{profile:?} reaches the point");
            assert!(times[1] < 300.0, "{profile:?}: {} ms", times[1]);
        }
    }
}
