//! Routers made by a factory given from outside (the mobile apps).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::{app_with_ramp_routing, app_with_ramp_valhalla, position};
use core_app::{LegSpec, PlanRequest, RouterFactory};
use core_routing::{Profile, Route, RouteOptions, RouteRequest, Router, RoutingError};

struct StraightRouter;

impl Router for StraightRouter {
    fn route(&self, request: &RouteRequest) -> Result<Route, RoutingError> {
        let mut route = core_routing::manual_route(&request.waypoints)?;
        route.profile = request.profile;
        route.duration_s = Some(route.distance_m);
        Ok(route)
    }

    fn profiles(&self) -> Vec<Profile> {
        vec![Profile::Hike]
    }
}

struct Factory {
    opened: AtomicUsize,
    version: Option<String>,
}

impl RouterFactory for Factory {
    fn open(&self, tiles: &Path) -> Result<Arc<dyn Router>, RoutingError> {
        assert!(tiles.ends_with("routing.tar"), "{}", tiles.display());
        self.opened.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(StraightRouter))
    }

    fn engine_version(&self) -> Option<String> {
        self.version.clone()
    }
}

fn factory(version: Option<&str>) -> Arc<Factory> {
    Arc::new(Factory {
        opened: AtomicUsize::new(0),
        version: version.map(str::to_string),
    })
}

fn hike() -> PlanRequest {
    PlanRequest {
        waypoints: vec![position(10.0, 32.0), position(50.0, 32.0)],
        legs: vec![LegSpec {
            profile: Profile::Hike,
            options: RouteOptions::default(),
        }],
        stage_ends: Vec::new(),
    }
}

#[test]
fn factory_is_used_for_routed_legs() {
    let app = app_with_ramp_routing("factory");
    let factory = factory(Some("3.9.1"));
    app.set_router_factory(factory.clone());
    let plan = app.plan_route(&hike()).unwrap();
    assert_eq!(plan.total.failed_legs, 0, "{:?}", plan.legs[0].error);
    assert!(plan.total.distance_m > 0.0);
    let mut moved = hike();
    moved.waypoints[1] = position(40.0, 32.0);
    app.plan_route(&moved).unwrap();
    assert_eq!(factory.opened.load(Ordering::SeqCst), 1, "one router per region");
    assert!(app.route_profiles().iter().all(|p| p.available));
}

#[test]
fn matching_engine_version_routes() {
    let app = app_with_ramp_valhalla("same-version", "3.9.1");
    app.set_router_factory(factory(Some("3.9.1")));
    assert_eq!(app.plan_route(&hike()).unwrap().total.failed_legs, 0);
}

#[test]
fn incompatible_engine_fails_leg() {
    let app = app_with_ramp_valhalla("incompatible", "3.9.1");
    let factory = factory(Some("3.8.0"));
    app.set_router_factory(factory.clone());
    let plan = app.plan_route(&hike()).unwrap();
    let error = plan.legs[0].error.as_ref().expect("leg fails");
    assert_eq!(error.code, "routing_incompatible");
    assert!(error.message.contains("3.9.1") && error.message.contains("3.8.0"));
    assert_eq!(factory.opened.load(Ordering::SeqCst), 0, "not even opened");
}

#[test]
fn a_new_factory_replaces_open_routers() {
    let app = app_with_ramp_routing("replace");
    let first = factory(None);
    app.set_router_factory(first.clone());
    app.plan_route(&hike()).unwrap();
    let second = factory(None);
    app.set_router_factory(second.clone());
    app.plan_route(&hike()).unwrap();
    assert_eq!(
        (first.opened.load(Ordering::SeqCst), second.opened.load(Ordering::SeqCst)),
        (1, 1)
    );
}

#[cfg(not(feature = "valhalla"))]
#[test]
fn no_factory_no_routing() {
    let app = app_with_ramp_routing("no-factory");
    let plan = app.plan_route(&hike()).unwrap();
    assert_eq!(plan.legs[0].error.as_ref().unwrap().code, "routing_unavailable");
    for profile in app.route_profiles() {
        assert_eq!(profile.available, profile.profile == Profile::Manual);
    }
}
