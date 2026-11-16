#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{app_with_mini, position};
use core_app::{LegSpec, PlanRequest, RoutePlan};
use core_maps::Catalog;
use core_routing::{Profile, RouteOptions};

fn manual() -> PlanRequest {
    PlanRequest {
        waypoints: vec![position(10.0, 32.0), position(50.0, 32.0)],
        legs: vec![LegSpec {
            profile: Profile::Manual,
            options: RouteOptions::default(),
        }],
        stage_ends: Vec::new(),
    }
}

#[test]
fn catalog_lists_complete_regions() {
    let (app, dir) = app_with_mini("share-catalog");
    let catalog = app.shared_catalog();
    assert_eq!(catalog.format, core_maps::FORMAT);
    assert_eq!(catalog.regions.len(), 1);
    let entry = &catalog.regions[0];
    assert_eq!(entry.id, "mini");
    assert_eq!(entry.path, "mini");
    assert_eq!(entry.size, 842_866);

    std::fs::remove_file(dir.join("regions/mini/outdoor.pmtiles")).unwrap();
    assert!(app.shared_catalog().regions.is_empty());
}

#[test]
fn catalog_lists_disabled_regions_too() {
    let (app, _) = app_with_mini("share-disabled");
    app.set_region_enabled("mini", false).unwrap();
    assert_eq!(app.shared_catalog().regions.len(), 1);
}

#[test]
fn catalog_roundtrips_through_parse() {
    let (app, _) = app_with_mini("share-roundtrip");
    let catalog = app.shared_catalog();
    let json = serde_json::to_string(&catalog).unwrap();
    assert_eq!(Catalog::parse(&json, "x").unwrap(), catalog);
}

#[test]
fn region_file_is_found_by_name() {
    let (app, _) = app_with_mini("share-file");
    let (path, len) = app.shared_region_file("mini", "manifest.json").unwrap();
    assert!(path.is_file());
    assert_eq!(len, std::fs::metadata(&path).unwrap().len());
    assert!(app.shared_region_file("mini", "nope").is_none());
}

#[test]
fn shared_routes_skip_deleted() {
    let (app, _) = app_with_mini("share-routes");
    app.plan_route(&manual()).unwrap();
    let first = app.save_planned_route_as_new("Eins").unwrap();
    app.plan_route(&manual()).unwrap();
    let second = app.save_planned_route_as_new("Zwei").unwrap();
    app.delete_route(first.id).unwrap();

    let routes = app.shared_routes().unwrap();

    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0].id, second.id);
    assert_eq!(routes[0].name, "Zwei");
    assert_eq!(routes[0].rev, second.rev);
}

#[test]
fn shared_route_carries_plan() {
    let (app, _) = app_with_mini("share-route");
    app.plan_route(&manual()).unwrap();
    let saved = app.save_planned_route_as_new("Eins").unwrap();

    let route = app.shared_route(saved.id).unwrap();

    assert!(route.plan.get("format").is_some());
    assert_eq!(route.info.id, saved.id);
    let plan: RoutePlan = serde_json::from_value(route.plan).unwrap();
    assert_eq!(plan.legs.len(), 1);

    app.delete_route(saved.id).unwrap();
    assert!(app.shared_route(saved.id).is_err());
}
