#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use common::{app_with_mini, app_with_ramp_routing, position};
use core_app::{LegSpec, PlanRequest, RegionState, RouterFactory};
use core_routing::{Profile, Route, RouteOptions, RouteRequest, Router, RoutingError};

#[test]
fn map_overview_never_reads_catalog() {
    let (app, _) = app_with_mini("offline-catalog");
    app.set_catalog_source("http://10.255.255.1:9/").unwrap();

    let started = Instant::now();
    let overview = app.map_overview();

    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(overview.regions.len(), 1);
    assert_eq!(overview.regions[0].id, "mini");
    assert_eq!(overview.regions[0].state, RegionState::Installed);
    assert!(overview.regions[0].enabled);
}

#[test]
fn disabled_region_leaves_bounds_and_attribution() {
    let (app, _) = app_with_mini("offline-disabled");
    let overview = app.map_overview();
    assert!(overview.installed_bounds.is_some());
    assert!(!overview.attribution.is_empty());

    app.set_region_enabled("mini", false).unwrap();

    let overview = app.map_overview();
    assert_eq!(overview.installed_bounds, None);
    assert!(overview.attribution.is_empty());
    assert_eq!(overview.regions.len(), 1);
    assert!(!overview.regions[0].enabled);
    let full = app.regions().unwrap();
    assert_eq!(full.installed_bounds, None);
    assert!(!full.regions[0].enabled);
}

struct Straight;

impl Router for Straight {
    fn route(&self, request: &RouteRequest) -> Result<Route, RoutingError> {
        let mut route = core_routing::manual_route(&request.waypoints)?;
        route.profile = request.profile;
        Ok(route)
    }

    fn profiles(&self) -> Vec<Profile> {
        vec![Profile::Hike]
    }
}

struct Counting(AtomicUsize);

impl RouterFactory for Counting {
    fn open(&self, _: &Path) -> Result<Arc<dyn Router>, RoutingError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(Straight))
    }

    fn engine_version(&self) -> Option<String> {
        None
    }
}

#[test]
fn disable_forgets_router() {
    let app = app_with_ramp_routing("offline-router");
    let factory = Arc::new(Counting(AtomicUsize::new(0)));
    app.set_router_factory(factory.clone());
    let plan = |col: f64| PlanRequest {
        waypoints: vec![position(10.0, 32.0), position(col, 32.0)],
        legs: vec![LegSpec {
            profile: Profile::Hike,
            options: RouteOptions::default(),
        }],
        stage_ends: Vec::new(),
    };

    app.plan_route(&plan(50.0)).unwrap();
    assert_eq!(factory.0.load(Ordering::SeqCst), 1);
    app.set_region_enabled("ramp", false).unwrap();
    app.set_region_enabled("ramp", true).unwrap();
    app.plan_route(&plan(40.0)).unwrap();

    assert_eq!(factory.0.load(Ordering::SeqCst), 2);
}

#[test]
fn share_port_defaults_to_8765() {
    let (app, _) = app_with_mini("offline-port");
    assert_eq!(app.share_port().unwrap(), 8765);
    app.set_share_port(9000).unwrap();
    assert_eq!(app.share_port().unwrap(), 9000);
}
