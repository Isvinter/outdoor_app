//! A router that hands Valhalla's JSON API to an engine outside the core.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use core_routing::{
    JsonEngine, JsonRouter, LatLon, Profile, RouteOptions, RouteRequest, Router, RoutingError,
    costing_json, engine_error,
};

fn ll(lat: f64, lon: f64) -> LatLon {
    LatLon { lat, lon }
}

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

struct Recorder {
    asked: Mutex<Vec<String>>,
    answer: Result<String, String>,
}

impl JsonEngine for Recorder {
    fn route_json(&self, request: &str) -> Result<String, String> {
        self.asked.lock().unwrap().push(request.to_string());
        self.answer.clone()
    }
}

fn request() -> RouteRequest {
    RouteRequest {
        waypoints: vec![ll(47.7128, 11.758), ll(47.7294, 11.7797)],
        profile: Profile::Hike,
        options: RouteOptions::default(),
        heading_deg: None,
    }
}

fn json_router(answer: Result<String, String>) -> (JsonRouter, Arc<Recorder>) {
    let engine = Arc::new(Recorder {
        asked: Mutex::new(Vec::new()),
        answer,
    });
    (JsonRouter::new(engine.clone()), engine)
}

#[test]
fn routes_through_engine() {
    let line = [ll(47.7128, 11.758), ll(47.72, 11.77), ll(47.7294, 11.7797)];
    let answer = serde_json::json!({
        "trip": {
            "legs": [
                { "shape": encode_polyline6(&line), "summary": { "length": 2.5, "time": 900.0 } }
            ],
            "summary": { "length": 2.5, "time": 900.0 }
        }
    });
    let (router, engine) = json_router(Ok(answer.to_string()));
    let route = router.route(&request()).unwrap();
    assert_eq!(
        engine.asked.lock().unwrap().as_slice(),
        [costing_json(&request()).unwrap()]
    );
    assert!((route.distance_m - 2500.0).abs() < 1e-6);
    assert_eq!(route.line.len(), 3);
    assert_eq!(route.duration_s, Some(900.0));
    assert!(router.profiles().contains(&Profile::Mtb));
    assert!(!router.profiles().contains(&Profile::Manual));
}

#[test]
fn engine_errors_are_mapped() {
    let (router, _) = json_router(Err("No path could be found for input".into()));
    assert!(matches!(
        router.route(&request()),
        Err(RoutingError::NoRoute(_))
    ));
    let (router, _) = json_router(Err("boom".into()));
    assert!(matches!(
        router.route(&request()),
        Err(RoutingError::Engine(_))
    ));
    assert!(matches!(
        engine_error("No suitable edges near location"),
        RoutingError::NoRoute(_)
    ));
}

#[test]
fn manual_profile_never_reaches_the_engine() {
    let (router, engine) = json_router(Ok("{}".into()));
    let mut manual = request();
    manual.profile = Profile::Manual;
    assert!(router.route(&manual).is_err());
    assert!(engine.asked.lock().unwrap().is_empty());
}
