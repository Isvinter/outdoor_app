#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::PathBuf;

use common::{app_with_ramp, open, position, temp_dir};
use core_app::{App, AppError};
use core_routing::{
    DinParams, LatLon, Profile, RouteOptions, RouteRequest, RoutingError, din_time_s,
};
use uuid::Uuid;

fn manual(waypoints: Vec<LatLon>) -> RouteRequest {
    RouteRequest {
        waypoints,
        profile: Profile::Manual,
        options: RouteOptions::default(),
    }
}

/// 40 pixels east along the ramp: 80 m up.
fn uphill() -> RouteRequest {
    manual(vec![position(10.0, 32.0), position(50.0, 32.0)])
}

#[test]
fn manual_route_gets_elevation_and_hiking_time_from_the_terrain() {
    let app = app_with_ramp("manual");

    let plan = app.plan_route(&uphill()).unwrap();

    assert_eq!(plan.route.profile, Profile::Manual);
    assert!(
        (plan.route.distance_m - 4090.0).abs() < 60.0,
        "{}",
        plan.route.distance_m
    );
    assert_eq!(plan.route.duration_s, None);
    let (ascent, descent) = (plan.ascent_m.unwrap(), plan.descent_m.unwrap());
    assert!((ascent - 80.0).abs() < 1.0, "ascent {ascent}");
    assert!(descent < 0.5, "descent {descent}");
    assert!((plan.min_ele_m.unwrap() - 1020.0).abs() < 1.0);
    assert!((plan.max_ele_m.unwrap() - 1100.0).abs() < 1.0);
    let expected = din_time_s(
        plan.route.distance_m,
        ascent,
        descent,
        &DinParams::default(),
    );
    assert!((plan.hiking_time_s.unwrap() - expected).abs() < 1e-6);

    // The profile runs along the whole route, densely enough to draw a chart.
    let last = plan.elevation.last().unwrap();
    assert!((last.distance_m - plan.route.distance_m).abs() < 1.0);
    assert!(plan.elevation.len() > 150);
    assert!(
        plan.elevation
            .windows(2)
            .all(|w| w[1].distance_m - w[0].distance_m <= 25.5)
    );
}

#[test]
fn route_outside_of_elevation_data_has_no_ascent() {
    let app = open(&temp_dir("no-dem"));

    let plan = app.plan_route(&uphill()).unwrap();

    assert!(plan.route.distance_m > 4000.0);
    assert_eq!(plan.ascent_m, None);
    assert_eq!(plan.descent_m, None);
    assert_eq!(plan.min_ele_m, None);
    assert_eq!(plan.hiking_time_s, None);
    assert!(plan.elevation.iter().all(|point| point.ele.is_none()));
}

#[test]
fn routed_profiles_need_routing_data() {
    let app = app_with_ramp("unavailable");
    let request = RouteRequest {
        profile: Profile::Hike,
        ..uphill()
    };

    let error = app.plan_route(&request).unwrap_err();

    assert!(
        matches!(error, AppError::Routing(RoutingError::Unavailable(_))),
        "{error}"
    );
    assert_eq!(error.code(), "routing_unavailable");
}

#[test]
fn a_single_waypoint_is_not_a_route() {
    let app = app_with_ramp("too-few");
    let error = app
        .plan_route(&manual(vec![position(1.0, 1.0)]))
        .unwrap_err();
    assert_eq!(error.code(), "route_too_few_waypoints");
}

#[test]
fn profiles_list_their_options() {
    let app = open(&temp_dir("profiles"));

    let profiles = app.route_profiles();

    assert_eq!(profiles.len(), Profile::ALL.len());
    let hike = profiles
        .iter()
        .find(|p| p.profile == Profile::Hike)
        .unwrap();
    assert_eq!(hike.options[0].key, "max_hiking_difficulty");
    assert_eq!(hike.available, cfg!(feature = "valhalla"));
    let manual = profiles
        .iter()
        .find(|p| p.profile == Profile::Manual)
        .unwrap();
    assert!(manual.available);
    assert!(manual.options.is_empty());
}

#[test]
fn planned_route_is_saved_listed_loaded_renamed_and_deleted() {
    let app = app_with_ramp("save");
    assert!(matches!(
        app.save_planned_route("Nothing"),
        Err(AppError::NoRoutePlanned)
    ));
    let plan = app.plan_route(&uphill()).unwrap();

    let record = app.save_planned_route("Rampe").unwrap();

    assert_eq!(record.name, "Rampe");
    assert_eq!(record.profile, "manual");
    assert_eq!(record.distance_m, plan.route.distance_m);
    assert_eq!(record.ascent_m, plan.ascent_m);
    assert_eq!(app.saved_routes().unwrap(), vec![record.clone()]);
    assert_eq!(app.saved_route(record.id).unwrap(), plan);

    app.rename_route(record.id, "Rampe Ost").unwrap();
    assert_eq!(app.saved_routes().unwrap()[0].name, "Rampe Ost");

    app.delete_route(record.id).unwrap();
    assert!(app.saved_routes().unwrap().is_empty());
    assert!(app.saved_route(record.id).is_err());
    assert!(app.saved_route(Uuid::now_v7()).is_err());
}

#[test]
fn loaded_route_becomes_the_planned_route() {
    let dir = temp_dir("reload");
    let id = {
        let app = open(&dir);
        app.plan_route(&uphill()).unwrap();
        app.save_planned_route("Rampe").unwrap().id
    };
    let app = open(&dir);
    assert!(app.planned_route().is_none());

    let plan = app.saved_route(id).unwrap();

    assert_eq!(app.planned_route(), Some(plan));
    app.clear_planned_route();
    assert!(app.planned_route().is_none());
}

#[test]
fn routes_are_exported_as_gpx_with_elevation() {
    let app = app_with_ramp("export");
    assert!(matches!(
        app.export_planned_route_gpx("x"),
        Err(AppError::NoRoutePlanned)
    ));
    let plan = app.plan_route(&uphill()).unwrap();

    let gpx = app.export_planned_route_gpx("Rampe & Co").unwrap();

    let document = core_gpx::parse_gpx(&gpx).unwrap();
    let track = &document.tracks[0];
    assert_eq!(track.name.as_deref(), Some("Rampe & Co"));
    assert_eq!(track.activity, Some(core_track::Activity::SkiTour));
    let points = &track.segments[0].points;
    assert_eq!(points.len(), plan.elevation.len());
    assert!((points[0].ele.unwrap() - 1020.0).abs() < 1.0);
    assert!((points.last().unwrap().ele.unwrap() - 1100.0).abs() < 1.0);
    assert!(points.iter().all(|point| point.time_ms.is_none()));

    let record = app.save_planned_route("Gespeichert").unwrap();
    let saved = core_gpx::parse_gpx(&app.export_route_gpx(record.id).unwrap()).unwrap();
    assert_eq!(saved.tracks[0].name.as_deref(), Some("Gespeichert"));
    assert_eq!(saved.tracks[0].segments[0].points.len(), points.len());
}

#[cfg(feature = "valhalla")]
mod valhalla {
    use super::*;

    const STATION: LatLon = LatLon {
        lat: 47.7128,
        lon: 11.7580,
    };
    const NEUREUTH: LatLon = LatLon {
        lat: 47.7294,
        lon: 11.7797,
    };

    /// An app with the Tegernsee routing fixture installed as a region.
    fn app_with_routing(name: &str) -> App {
        let dir = temp_dir(name);
        let region = dir.join("regions/tegernsee");
        std::fs::create_dir_all(&region).unwrap();
        let tiles =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/routing/tegernsee.tar");
        let size = std::fs::copy(tiles, region.join("routing.tar")).unwrap();
        let manifest = serde_json::json!({
            "format": 1, "id": "tegernsee", "name": "Tegernsee", "bbox": [11.74, 47.70, 11.80, 47.74],
            "version": "1", "attribution": [], "routing": { "valhalla": "3.9.1", "max_mtb_scale": 2 },
            "files": [{ "name": "routing.tar", "size": size, "sha256": "0".repeat(64) }], "size": size,
        });
        std::fs::write(region.join("manifest.json"), manifest.to_string()).unwrap();
        open(&dir)
    }

    fn hike(to: LatLon) -> RouteRequest {
        RouteRequest {
            waypoints: vec![STATION, to],
            profile: Profile::Hike,
            options: RouteOptions::default(),
        }
    }

    #[test]
    fn hike_is_routed_on_the_tiles_of_the_region() {
        let app = app_with_routing("valhalla-hike");

        let plan = app.plan_route(&hike(NEUREUTH)).unwrap();

        assert!(
            (plan.route.distance_m - 3630.0).abs() < 50.0,
            "{}",
            plan.route.distance_m
        );
        assert!(plan.route.duration_s.unwrap() > 1800.0);
        assert_eq!(plan.region.as_deref(), Some("tegernsee"));
        assert_eq!(plan.max_mtb_scale, Some(2));
        // No elevation tiles in this region.
        assert_eq!(plan.ascent_m, None);
        // The second request reuses the router.
        assert!(app.plan_route(&hike(NEUREUTH)).is_ok());
    }

    #[test]
    fn destination_outside_of_the_region_has_no_routing_data() {
        let app = app_with_routing("valhalla-outside");
        let far = LatLon {
            lat: 47.60,
            lon: 11.50,
        };
        let error = app.plan_route(&hike(far)).unwrap_err();
        assert_eq!(error.code(), "routing_unavailable");
    }

    #[test]
    fn unreachable_destination_is_reported_as_no_route() {
        let app = app_with_routing("valhalla-car");
        let request = RouteRequest {
            profile: Profile::Car,
            ..hike(NEUREUTH)
        };
        let error = app.plan_route(&request).unwrap_err();
        assert_eq!(error.code(), "route_not_found");
    }

    #[test]
    fn deleting_the_region_removes_its_router() {
        let app = app_with_routing("valhalla-delete");
        app.plan_route(&hike(NEUREUTH)).unwrap();
        app.delete_region("tegernsee").unwrap();
        let error = app.plan_route(&hike(NEUREUTH)).unwrap_err();
        assert_eq!(error.code(), "routing_unavailable");
    }
}
