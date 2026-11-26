#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::{
    app_with_basemap_only, app_with_ramp, app_with_ramp_routing, open, position, temp_dir,
};
use core_app::{App, AppError, KnownPosition, LegSpec, PlanRequest, RouteStart};
use core_routing::{
    DinParams, LatLon, Profile, Route, RouteOptions, RouteRequest, Router, RoutingError, din_time_s,
};
use uuid::Uuid;

/// A route through `waypoints` with every leg in `profile`.
fn route(profile: Profile, waypoints: Vec<LatLon>) -> PlanRequest {
    let legs = waypoints.len().saturating_sub(1);
    PlanRequest {
        waypoints,
        legs: vec![
            LegSpec {
                profile,
                options: RouteOptions::default(),
            };
            legs
        ],
        stage_ends: Vec::new(),
    }
}

fn manual(waypoints: Vec<LatLon>) -> PlanRequest {
    route(Profile::Manual, waypoints)
}

/// 40 pixels east along the ramp: 80 m up.
fn uphill() -> PlanRequest {
    manual(vec![position(10.0, 32.0), position(50.0, 32.0)])
}

/// Three points along the ramp, 20 pixels apart: 40 m up per leg.
fn three_points() -> PlanRequest {
    manual(vec![
        position(10.0, 32.0),
        position(30.0, 32.0),
        position(50.0, 32.0),
    ])
}

#[test]
fn manual_route_gets_elevation_and_hiking_time_from_the_terrain() {
    let app = app_with_ramp("manual");

    let plan = app.plan_route(&uphill()).unwrap();

    assert_eq!(plan.legs.len(), 1);
    let leg = &plan.legs[0];
    assert_eq!(leg.profile, Profile::Manual);
    assert!((leg.distance_m - 4090.0).abs() < 60.0, "{}", leg.distance_m);
    assert_eq!(leg.duration_s, None);
    assert_eq!(plan.total.distance_m, leg.distance_m);
    let (ascent, descent) = (plan.total.ascent_m.unwrap(), plan.total.descent_m.unwrap());
    assert!((ascent - 80.0).abs() < 1.0, "ascent {ascent}");
    assert!(descent < 0.5, "descent {descent}");
    assert!((plan.total.min_ele_m.unwrap() - 1020.0).abs() < 1.0);
    assert!((plan.total.max_ele_m.unwrap() - 1100.0).abs() < 1.0);
    let expected = din_time_s(leg.distance_m, ascent, descent, &DinParams::default());
    assert!((plan.total.time_s.unwrap() - expected).abs() < 1e-6);
    assert_eq!(plan.total.failed_legs, 0);

    // The profile runs along the whole route, densely enough to draw a chart.
    let last = plan.elevation.last().unwrap();
    assert!((last.distance_m - plan.total.distance_m).abs() < 1.0);
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

    assert!(plan.total.distance_m > 4000.0);
    assert_eq!(plan.total.ascent_m, None);
    assert_eq!(plan.total.descent_m, None);
    assert_eq!(plan.total.min_ele_m, None);
    assert_eq!(plan.total.time_s, None);
    assert_eq!(plan.stages[0].ascent_m, None);
    assert!(plan.elevation.iter().all(|point| point.ele.is_none()));
}

/// A car router that answers with straight lines and a time of 10 m/s.
struct CarRouter;

impl Router for CarRouter {
    fn route(&self, request: &RouteRequest) -> Result<Route, RoutingError> {
        let mut route = core_routing::manual_route(&request.waypoints)?;
        route.profile = request.profile;
        route.duration_s = Some(route.distance_m / 10.0);
        Ok(route)
    }

    fn profiles(&self) -> Vec<Profile> {
        vec![Profile::Car]
    }
}

#[test]
fn route_without_dem_has_no_ascent() {
    let app = app_with_basemap_only("basemap-only");
    app.set_router_for_tests("flat", Arc::new(CarRouter));
    let request = route(
        Profile::Car,
        vec![position(10.0, 32.0), position(50.0, 32.0)],
    );

    let plan = app.plan_route(&request).unwrap();

    assert_eq!(plan.total.failed_legs, 0);
    assert_eq!(plan.legs[0].profile, Profile::Car);
    assert_eq!(plan.total.ascent_m, None);
    assert_eq!(plan.legs[0].ascent_m, None);
    assert!(plan.elevation.iter().all(|point| point.ele.is_none()));
    let time = plan.legs[0].time_s.unwrap();
    assert!((time - plan.total.distance_m / 10.0).abs() < 1e-6);
    assert_eq!(plan.total.time_s, Some(time));
}

#[test]
fn routed_leg_without_routing_data_is_marked_failed() {
    let app = app_with_ramp("unavailable");

    let plan = app
        .plan_route(&route(Profile::Hike, uphill().waypoints))
        .unwrap();

    let error = plan.legs[0].error.as_ref().unwrap();
    assert_eq!(error.code, "routing_unavailable");
    assert!(plan.legs[0].line.is_empty());
    assert_eq!(plan.total.failed_legs, 1);
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
fn legs_must_match_the_waypoints() {
    let app = app_with_ramp("leg-count");
    let mut request = three_points();
    request.legs.pop();
    let error = app.plan_route(&request).unwrap_err();
    assert_eq!(error.code(), "route_plan");
}

#[test]
fn stage_ends_must_be_inner_ascending_waypoints() {
    let app = app_with_ramp("stage-ends");
    let four = manual(vec![
        position(10.0, 32.0),
        position(20.0, 32.0),
        position(30.0, 32.0),
        position(40.0, 32.0),
    ]);
    for ends in [vec![0], vec![3], vec![2, 1], vec![1, 1]] {
        let request = PlanRequest {
            stage_ends: ends.clone(),
            ..four.clone()
        };
        let error = app.plan_route(&request).unwrap_err();
        assert_eq!(error.code(), "route_plan", "{ends:?}");
    }
    let request = PlanRequest {
        stage_ends: vec![1, 2],
        ..four
    };
    assert_eq!(app.plan_route(&request).unwrap().stages.len(), 3);
}

#[test]
fn each_stage_has_its_statistics_and_the_total_is_their_sum() {
    let app = app_with_ramp("stages");
    let request = PlanRequest {
        stage_ends: vec![1],
        ..three_points()
    };

    let plan = app.plan_route(&request).unwrap();

    assert_eq!(plan.stages.len(), 2);
    let (first, second) = (&plan.stages[0], &plan.stages[1]);
    assert_eq!((first.from, first.to), (0, 1));
    assert_eq!((second.from, second.to), (1, 2));
    assert_eq!(first.profiles, vec![Profile::Manual]);
    assert!((first.ascent_m.unwrap() - 40.0).abs() < 1.5);
    let sum = |f: fn(&core_app::Stage) -> f64| plan.stages.iter().map(f).sum::<f64>();
    assert!((sum(|s| s.distance_m) - plan.total.distance_m).abs() < 1e-6);
    assert!((sum(|s| s.ascent_m.unwrap()) - plan.total.ascent_m.unwrap()).abs() < 1e-6);
    assert!((sum(|s| s.time_s.unwrap()) - plan.total.time_s.unwrap()).abs() < 1e-6);
}

#[test]
fn elevation_points_know_their_leg() {
    let app = app_with_ramp("legs-elevation");

    let plan = app.plan_route(&three_points()).unwrap();

    assert_eq!(plan.elevation.first().unwrap().leg, 0);
    assert_eq!(plan.elevation.last().unwrap().leg, 1);
    assert!(plan.elevation.windows(2).all(|w| w[0].leg <= w[1].leg));
    assert!(
        plan.elevation
            .windows(2)
            .all(|w| w[1].distance_m >= w[0].distance_m)
    );
    let last = plan.elevation.last().unwrap();
    assert!((last.distance_m - plan.total.distance_m).abs() < 1.0);
}

/// Straight lines, counting how often it was asked; fails towards the far east.
struct CountingRouter {
    calls: AtomicUsize,
}

impl Router for CountingRouter {
    fn route(&self, request: &RouteRequest) -> Result<Route, RoutingError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if request
            .waypoints
            .iter()
            .any(|p| p.lon > position(55.0, 0.0).lon)
        {
            return Err(RoutingError::NoRoute("too far east".into()));
        }
        let mut route = core_routing::manual_route(&request.waypoints)?;
        route.profile = request.profile;
        route.duration_s = Some(route.distance_m / 4.0);
        Ok(route)
    }

    fn profiles(&self) -> Vec<Profile> {
        vec![Profile::Hike, Profile::Mtb]
    }
}

fn counting_app(name: &str) -> (App, Arc<CountingRouter>) {
    let app = app_with_ramp_routing(name);
    let router = Arc::new(CountingRouter {
        calls: AtomicUsize::new(0),
    });
    app.set_router_for_tests("ramp", router.clone());
    (app, router)
}

#[test]
fn only_changed_legs_are_computed() {
    let (app, router) = counting_app("cache");
    let calls = || router.calls.load(Ordering::SeqCst);
    let mut request = route(
        Profile::Hike,
        vec![
            position(10.0, 32.0),
            position(20.0, 32.0),
            position(30.0, 32.0),
        ],
    );

    app.plan_route(&request).unwrap();
    assert_eq!(calls(), 2);

    request.waypoints.push(position(40.0, 32.0));
    request.legs.push(request.legs[0]);
    app.plan_route(&request).unwrap();
    assert_eq!(calls(), 3, "a new point computes only the new leg");

    request.waypoints[3] = position(40.0, 40.0);
    app.plan_route(&request).unwrap();
    assert_eq!(calls(), 4, "moving the last point computes only its leg");

    request.legs[2].profile = Profile::Mtb;
    let plan = app.plan_route(&request).unwrap();
    assert_eq!(calls(), 5, "a new profile computes only that leg");
    assert_eq!(plan.stages[0].profiles, vec![Profile::Hike, Profile::Mtb]);

    request.waypoints.pop();
    request.legs.pop();
    app.plan_route(&request).unwrap();
    assert_eq!(calls(), 5, "undo computes nothing");
}

#[test]
fn splitting_a_leg_computes_only_its_two_halves() {
    let (app, router) = counting_app("split");
    let calls = || router.calls.load(Ordering::SeqCst);
    let request = route(
        Profile::Hike,
        vec![
            position(10.0, 32.0),
            position(20.0, 32.0),
            position(30.0, 32.0),
        ],
    );
    let plan = app.plan_route(&request).unwrap();
    assert_eq!(calls(), 2);

    let split = request.split_leg(1, &plan.legs[1].line);
    let result = app.plan_route(&split).unwrap();
    assert_eq!(calls(), 4, "splitting computes only the two halves");
    assert_eq!(result.legs.len(), 3);
    let halves = result.legs[1].distance_m + result.legs[2].distance_m;
    assert!(
        (halves - plan.legs[1].distance_m).abs() < 1.0,
        "the route stays the same"
    );
}

#[test]
fn a_failing_leg_keeps_the_others() {
    let (app, _router) = counting_app("failing-leg");
    let request = route(
        Profile::Hike,
        vec![
            position(10.0, 32.0),
            position(30.0, 32.0),
            position(60.0, 32.0),
            position(40.0, 32.0),
        ],
    );

    let plan = app.plan_route(&request).unwrap();

    assert!(plan.legs[0].error.is_none());
    assert!(!plan.legs[0].line.is_empty());
    let failed = plan.legs[1].error.as_ref().unwrap();
    assert_eq!(failed.code, "route_not_found");
    assert!(plan.legs[1].line.is_empty());
    assert_eq!(plan.total.failed_legs, 2);
    assert!(
        (plan.total.distance_m - plan.legs[0].distance_m).abs() < 1e-6,
        "failed legs add nothing"
    );
}

#[test]
fn routed_and_manual_legs_mix_with_their_own_times() {
    let (app, _router) = counting_app("mixed");
    let mut request = three_points();
    request.legs[0].profile = Profile::Mtb;

    let plan = app.plan_route(&request).unwrap();

    assert_eq!(plan.legs[0].time_s, plan.legs[0].duration_s);
    let manual = &plan.legs[1];
    let din = din_time_s(
        manual.distance_m,
        manual.ascent_m.unwrap(),
        manual.descent_m.unwrap(),
        &DinParams::default(),
    );
    assert!((manual.time_s.unwrap() - din).abs() < 1e-6);
    assert_eq!(plan.stages[0].profiles, vec![Profile::Mtb, Profile::Manual]);
    let total = plan.legs[0].time_s.unwrap() + manual.time_s.unwrap();
    assert!((plan.total.time_s.unwrap() - total).abs() < 1e-6);
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
    assert_eq!(record.distance_m, plan.total.distance_m);
    assert_eq!(record.ascent_m, plan.total.ascent_m);
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

/// Detached, the plan stays but "save" no longer overwrites the saved route.
#[test]
fn detached_route_is_saved_as_a_new_one() {
    let app = app_with_ramp("detach");
    let plan = app.plan_route(&uphill()).unwrap();
    let first = app.save_planned_route("Rampe").unwrap();
    assert_eq!(app.loaded_route(), Some(first.id));

    app.detach_planned_route();

    assert_eq!(app.planned_route(), Some(plan));
    assert_eq!(app.loaded_route(), None);
    let second = app.save_planned_route("Rampe umgeplant").unwrap();
    assert_ne!(second.id, first.id);
    assert_eq!(app.saved_routes().unwrap().len(), 2);
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
    use std::path::PathBuf;

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

    fn hike(to: LatLon) -> PlanRequest {
        route(Profile::Hike, vec![STATION, to])
    }

    #[test]
    fn hike_is_routed_on_the_tiles_of_the_region() {
        let app = app_with_routing("valhalla-hike");

        let plan = app.plan_route(&hike(NEUREUTH)).unwrap();

        let leg = &plan.legs[0];
        assert!(leg.error.is_none(), "{:?}", leg.error);
        assert!((leg.distance_m - 3630.0).abs() < 50.0, "{}", leg.distance_m);
        assert!(leg.duration_s.unwrap() > 1800.0);
        assert_eq!(leg.region.as_deref(), Some("tegernsee"));
        assert_eq!(leg.max_mtb_scale, Some(2));
        // No elevation tiles in this region: no DIN time, the router's time instead.
        assert_eq!(leg.ascent_m, None);
        assert_eq!(leg.time_s, leg.duration_s);
        // The second request reuses the router.
        assert!(app.plan_route(&hike(NEUREUTH)).is_ok());
    }

    #[test]
    fn mixed_hike_and_mtb_legs_on_the_fixture() {
        let app = app_with_routing("valhalla-mixed");
        let mut request = route(Profile::Hike, vec![STATION, NEUREUTH, STATION]);
        request.legs[1].profile = Profile::Mtb;

        let plan = app.plan_route(&request).unwrap();

        assert!(
            plan.legs.iter().all(|leg| leg.error.is_none()),
            "{:?}",
            plan.legs
        );
        assert_eq!(plan.stages[0].profiles, vec![Profile::Hike, Profile::Mtb]);
    }

    #[test]
    fn destination_outside_of_the_region_has_no_routing_data() {
        let app = app_with_routing("valhalla-outside");
        let far = LatLon {
            lat: 47.60,
            lon: 11.50,
        };
        let plan = app.plan_route(&hike(far)).unwrap();
        assert_eq!(
            plan.legs[0].error.as_ref().unwrap().code,
            "routing_unavailable"
        );
    }

    #[test]
    fn unreachable_destination_is_reported_as_no_route() {
        let app = app_with_routing("valhalla-car");
        let plan = app
            .plan_route(&route(Profile::Car, vec![STATION, NEUREUTH]))
            .unwrap();
        assert_eq!(plan.legs[0].error.as_ref().unwrap().code, "route_not_found");
    }

    #[test]
    fn deleting_the_region_removes_its_router_and_legs() {
        let app = app_with_routing("valhalla-delete");
        app.plan_route(&hike(NEUREUTH)).unwrap();
        app.delete_region("tegernsee").unwrap();
        let plan = app.plan_route(&hike(NEUREUTH)).unwrap();
        assert_eq!(
            plan.legs[0].error.as_ref().unwrap().code,
            "routing_unavailable"
        );
    }
}

#[test]
fn mixed_route_is_listed_with_all_profiles() {
    let (app, _router) = counting_app("save-mixed");
    let mut request = three_points();
    request.legs[0].profile = Profile::Hike;
    let plan = app.plan_route(&request).unwrap();

    let record = app.save_planned_route("Gemischt").unwrap();

    assert_eq!(record.profile, "hike+manual");
    assert_eq!(app.saved_route(record.id).unwrap(), plan);
}

/// A route as saved before routes had legs.
fn old_plan_json(profile: &str, from: LatLon, to: LatLon) -> String {
    serde_json::json!({
        "route": {
            "profile": profile,
            "waypoints": [from, to],
            "line": [from, to],
            "distance_m": 4000.0,
            "duration_s": null,
        },
        "options": {},
        "elevation": [],
        "ascent_m": null, "descent_m": null, "min_ele_m": null, "max_ele_m": null,
        "hiking_time_s": null, "region": null, "max_mtb_scale": null,
    })
    .to_string()
}

#[test]
fn old_saved_plans_are_loaded_and_recomputed() {
    let dir = temp_dir("old-plans");
    let app = open(&dir);
    let (from, to) = (position(10.0, 32.0), position(50.0, 32.0));
    let (manual_id, hike_id) = {
        let mut storage = core_storage::Storage::open(&dir.join("app.sqlite")).unwrap();
        let manual = old_plan_json("manual", from, to);
        let hike = old_plan_json("hike", from, to);
        (
            storage
                .save_route("Alt", "manual", 4000.0, None, &manual, 1)
                .unwrap(),
            storage
                .save_route("Alt zu Fuß", "hike", 4000.0, None, &hike, 1)
                .unwrap(),
        )
    };

    let plan = app.saved_route(manual_id).unwrap();
    assert_eq!(plan.request.waypoints, vec![from, to]);
    assert_eq!(plan.request.legs.len(), 1);
    assert_eq!(plan.legs[0].profile, Profile::Manual);
    assert!((plan.total.distance_m - 4090.0).abs() < 60.0);

    // Without routing data the leg fails, but the route still opens.
    let plan = app.saved_route(hike_id).unwrap();
    assert_eq!(plan.legs[0].profile, Profile::Hike);
    assert!(plan.legs[0].error.is_some());
}

#[test]
fn routes_are_exported_as_gpx_with_stages() {
    let app = app_with_ramp("export-stages");
    let request = PlanRequest {
        stage_ends: vec![1],
        ..three_points()
    };
    let plan = app.plan_route(&request).unwrap();

    let xml = app.export_planned_route_gpx("Zwei Tage").unwrap();

    let document = core_gpx::parse_gpx(&xml).unwrap();
    let names: Vec<_> = document
        .tracks
        .iter()
        .map(|t| t.name.clone().unwrap())
        .collect();
    assert_eq!(names, vec!["Zwei Tage – Etappe 1", "Zwei Tage – Etappe 2"]);
    assert_eq!(
        document.tracks[0].activity,
        Some(core_track::Activity::SkiTour)
    );
    let points: usize = document
        .tracks
        .iter()
        .flat_map(|t| &t.segments)
        .map(|s| s.points.len())
        .sum();
    assert_eq!(points, plan.elevation.len());
    assert_eq!(xml.matches("<wpt").count(), 1);
    assert!(xml.contains("<name>Etappe 1 Ende</name>"));
}

#[test]
fn legs_of_different_activity_are_separate_segments() {
    let (app, _router) = counting_app("export-mixed");
    let mut request = three_points();
    request.legs[0].profile = Profile::Mtb;
    app.plan_route(&request).unwrap();

    let document = core_gpx::parse_gpx(&app.export_planned_route_gpx("Mix").unwrap()).unwrap();

    assert_eq!(document.tracks.len(), 1);
    assert_eq!(document.tracks[0].name.as_deref(), Some("Mix"));
    assert_eq!(document.tracks[0].segments.len(), 2);
    assert_eq!(document.tracks[0].activity, None);
}

#[test]
fn a_leg_of_no_length_does_not_hide_the_ascent() {
    let app = app_with_ramp("zero-leg");
    let at = position(30.0, 32.0);
    let request = manual(vec![position(10.0, 32.0), at, at]);

    let plan = app.plan_route(&request).unwrap();

    assert_eq!(plan.legs[1].distance_m, 0.0);
    assert_eq!(plan.legs[1].ascent_m, Some(0.0));
    assert!((plan.total.ascent_m.unwrap() - 40.0).abs() < 1.5);
}

#[test]
fn the_plan_for_display_has_a_thinned_elevation_profile() {
    let app = app_with_ramp("thinned");
    let plan = app.plan_route(&three_points()).unwrap();
    assert!(plan.elevation.len() > 100);

    let shown = plan.for_display(20);

    assert!(
        shown.elevation.len() <= 20 + plan.legs.len(),
        "{}",
        shown.elevation.len()
    );
    assert_eq!(shown.elevation.first(), plan.elevation.first());
    for leg in 0..plan.legs.len() {
        let last =
            |points: &[core_app::RoutePoint]| points.iter().rev().find(|p| p.leg == leg).copied();
        assert_eq!(
            last(&shown.elevation),
            last(&plan.elevation),
            "end of leg {leg}"
        );
    }
    assert_eq!(shown.legs, plan.legs);
    assert_eq!(shown.total, plan.total);
    assert_eq!(plan.for_display(10_000), plan);
}

/// 40 pixels east along the ramp, then 10 more.
fn longer() -> PlanRequest {
    manual(vec![
        position(10.0, 32.0),
        position(50.0, 32.0),
        position(60.0, 32.0),
    ])
}

#[test]
fn save_twice_overwrites() {
    let app = app_with_ramp("save-twice");
    app.plan_route(&uphill()).unwrap();
    let first = app.save_planned_route("Rampe").unwrap();
    assert_eq!(app.loaded_route(), Some(first.id));

    let plan = app.plan_route(&longer()).unwrap();
    let second = app.save_planned_route("Rampe lang").unwrap();

    assert_eq!(second.id, first.id);
    assert_eq!(second.rev, first.rev + 1);
    assert_eq!(second.name, "Rampe lang");
    assert_eq!(second.distance_m, plan.total.distance_m);
    assert_eq!(app.saved_routes().unwrap(), vec![second.clone()]);
    assert_eq!(app.saved_route(second.id).unwrap(), plan);
}

#[test]
fn save_loaded_overwrites() {
    let dir = temp_dir("save-loaded");
    let id = {
        let app = open(&dir);
        app.plan_route(&uphill()).unwrap();
        app.save_planned_route("Rampe").unwrap().id
    };
    let app = open(&dir);
    assert_eq!(app.loaded_route(), None);
    app.saved_route(id).unwrap();
    assert_eq!(app.loaded_route(), Some(id));
    app.plan_route(&longer()).unwrap();

    let saved = app.save_planned_route("Rampe").unwrap();

    assert_eq!(saved.id, id);
    assert_eq!(app.saved_routes().unwrap().len(), 1);
}

#[test]
fn save_as_new_keeps_old() {
    let app = app_with_ramp("save-new");
    app.plan_route(&uphill()).unwrap();
    let old = app.save_planned_route("Rampe").unwrap();
    app.plan_route(&longer()).unwrap();

    let new = app.save_planned_route_as_new("Rampe lang").unwrap();

    assert_ne!(new.id, old.id);
    assert_eq!(app.loaded_route(), Some(new.id));
    let routes = app.saved_routes().unwrap();
    assert_eq!(routes.len(), 2);
    assert!(routes.contains(&old), "the old route is unchanged");
    // Saving again overwrites the new one.
    assert_eq!(app.save_planned_route("Rampe lang").unwrap().id, new.id);
    assert_eq!(app.saved_routes().unwrap().len(), 2);
}

#[test]
fn new_route_then_save_creates_new() {
    let app = app_with_ramp("clear-save");
    app.plan_route(&uphill()).unwrap();
    let first = app.save_planned_route("Eins").unwrap();
    app.start_new_route();
    assert!(app.planned_route().is_none());
    assert_eq!(app.loaded_route(), None);
    app.plan_route(&longer()).unwrap();

    let second = app.save_planned_route("Zwei").unwrap();

    assert_ne!(second.id, first.id);
    assert_eq!(app.saved_routes().unwrap().len(), 2);
}

#[test]
fn save_after_deleting_loaded_creates_new() {
    let app = app_with_ramp("delete-save");
    app.plan_route(&uphill()).unwrap();
    let first = app.save_planned_route("Eins").unwrap();
    app.delete_route(first.id).unwrap();
    assert_eq!(app.loaded_route(), None);

    let second = app.save_planned_route("Eins").unwrap();

    assert_ne!(second.id, first.id);
    assert_eq!(app.saved_routes().unwrap(), vec![second]);
}

#[test]
fn deleting_another_route_keeps_the_loaded_one() {
    let app = app_with_ramp("delete-other");
    app.plan_route(&uphill()).unwrap();
    let other = app.save_planned_route_as_new("Andere").unwrap();
    let loaded = app.save_planned_route_as_new("Geladen").unwrap();
    app.delete_route(other.id).unwrap();
    assert_eq!(app.loaded_route(), Some(loaded.id));
}

/// Fewer than two points leave nothing to plan, but the route being edited
/// is still the loaded one (e.g. while its start is set anew).
#[test]
fn clearing_the_plan_keeps_the_loaded_route() {
    let app = app_with_ramp("clear-keeps");
    app.plan_route(&uphill()).unwrap();
    let first = app.save_planned_route("Eins").unwrap();
    app.clear_planned_route();
    assert!(app.planned_route().is_none());
    assert_eq!(app.loaded_route(), Some(first.id));
    app.plan_route(&longer()).unwrap();

    let saved = app.save_planned_route("Eins").unwrap();

    assert_eq!(saved.id, first.id);
    assert_eq!(app.saved_routes().unwrap().len(), 1);
}

/// Waits in `route` until told to go on, for a call that ends after a newer one.
struct SlowRouter {
    entered: std::sync::Mutex<bool>,
    go: std::sync::Condvar,
    released: std::sync::Mutex<bool>,
    arrived: std::sync::Condvar,
}

impl Router for SlowRouter {
    fn route(&self, request: &RouteRequest) -> Result<Route, RoutingError> {
        *self.entered.lock().unwrap() = true;
        self.arrived.notify_all();
        let mut released = self.released.lock().unwrap();
        while !*released {
            released = self.go.wait(released).unwrap();
        }
        let mut route = core_routing::manual_route(&request.waypoints)?;
        route.profile = request.profile;
        Ok(route)
    }

    fn profiles(&self) -> Vec<Profile> {
        vec![Profile::Hike]
    }
}

#[test]
fn an_older_plan_finishing_late_does_not_replace_a_newer_one() {
    let app = app_with_ramp_routing("stale");
    let router = Arc::new(SlowRouter {
        entered: std::sync::Mutex::new(false),
        go: std::sync::Condvar::new(),
        released: std::sync::Mutex::new(false),
        arrived: std::sync::Condvar::new(),
    });
    app.set_router_for_tests("ramp", router.clone());
    let old = route(
        Profile::Hike,
        vec![position(10.0, 32.0), position(50.0, 32.0)],
    );

    std::thread::scope(|scope| {
        let slow = scope.spawn(|| app.plan_route(&old).unwrap());
        let mut entered = router.entered.lock().unwrap();
        while !*entered {
            entered = router.arrived.wait(entered).unwrap();
        }
        drop(entered);
        // Meanwhile the user set another route (no routing needed).
        let newer = app.plan_route(&longer()).unwrap();
        *router.released.lock().unwrap() = true;
        router.go.notify_all();
        let late = slow.join().unwrap();
        assert_eq!(late.request, old, "the caller still gets its answer");
        assert_eq!(app.planned_route(), Some(newer));
    });
}

#[test]
fn a_plan_in_flight_does_not_bring_back_a_cleared_one() {
    let app = app_with_ramp_routing("stale-clear");
    let router = Arc::new(SlowRouter {
        entered: std::sync::Mutex::new(false),
        go: std::sync::Condvar::new(),
        released: std::sync::Mutex::new(false),
        arrived: std::sync::Condvar::new(),
    });
    app.set_router_for_tests("ramp", router.clone());
    let old = route(
        Profile::Hike,
        vec![position(10.0, 32.0), position(50.0, 32.0)],
    );

    std::thread::scope(|scope| {
        let slow = scope.spawn(|| app.plan_route(&old).unwrap());
        let mut entered = router.entered.lock().unwrap();
        while !*entered {
            entered = router.arrived.wait(entered).unwrap();
        }
        drop(entered);
        app.start_new_route();
        *router.released.lock().unwrap() = true;
        router.go.notify_all();
        slow.join().unwrap();
        assert_eq!(app.planned_route(), None);
    });
}

fn known(at: LatLon, age_s: f64) -> KnownPosition {
    KnownPosition {
        lat: at.lat,
        lon: at.lon,
        age_s,
    }
}

const ELSEWHERE: LatLon = LatLon { lat: 1.0, lon: 1.0 };

#[test]
fn fresh_position_in_same_region_is_ready() {
    let app = app_with_ramp_routing("start-ready");
    let (from, to) = (position(10.0, 32.0), position(50.0, 32.0));
    assert_eq!(
        app.route_start(Some(known(from, 5.0)), to),
        RouteStart::Ready
    );
}

#[test]
fn position_older_than_two_minutes_is_no_position() {
    let app = app_with_ramp_routing("start-old");
    let (from, to) = (position(10.0, 32.0), position(50.0, 32.0));
    assert_eq!(
        app.route_start(Some(known(from, 120.0)), to),
        RouteStart::Ready
    );
    assert_eq!(
        app.route_start(Some(known(from, 120.1)), to),
        RouteStart::NoPosition
    );
    assert_eq!(app.route_start(None, to), RouteStart::NoPosition);
}

#[test]
fn position_in_other_region_is_other_map() {
    let app = app_with_ramp_routing("start-other");
    let to = position(50.0, 32.0);
    assert_eq!(
        app.route_start(Some(known(ELSEWHERE, 1.0)), to),
        RouteStart::OtherMap
    );
}

#[test]
fn target_without_routing_is_other_map() {
    let app = app_with_ramp("start-no-routing");
    let (from, to) = (position(10.0, 32.0), position(50.0, 32.0));
    assert_eq!(
        app.route_start(Some(known(from, 1.0)), to),
        RouteStart::OtherMap
    );
}

#[test]
fn disabled_region_does_not_count() {
    let app = app_with_ramp_routing("start-disabled");
    app.set_region_enabled("ramp", false).unwrap();
    let (from, to) = (position(10.0, 32.0), position(50.0, 32.0));
    assert_eq!(
        app.route_start(Some(known(from, 1.0)), to),
        RouteStart::OtherMap
    );
}

#[test]
fn empty_route_is_not_unsaved() {
    let app = app_with_ramp("unsaved-empty");
    assert!(!app.route_is_unsaved(&PlanRequest::empty()));
}

#[test]
fn new_route_is_unsaved() {
    let app = app_with_ramp("unsaved-new");
    assert!(app.route_is_unsaved(&uphill()));
    let one_point = manual(vec![position(10.0, 32.0)]);
    assert!(app.route_is_unsaved(&one_point));
}

#[test]
fn saved_route_is_not_unsaved_until_changed() {
    let app = app_with_ramp("unsaved-saved");
    let request = uphill();
    app.plan_route(&request).unwrap();
    app.save_planned_route("Tour").unwrap();
    assert!(!app.route_is_unsaved(&request));
    assert!(app.route_is_unsaved(&three_points()));
}

#[test]
fn loaded_route_is_not_unsaved_until_changed() {
    let app = app_with_ramp("unsaved-loaded");
    let request = uphill();
    app.plan_route(&request).unwrap();
    let id = app.save_planned_route("Tour").unwrap().id;
    app.start_new_route();
    assert!(app.route_is_unsaved(&request));
    let loaded = app.saved_route(id).unwrap();
    assert!(!app.route_is_unsaved(&loaded.request));
    assert!(app.route_is_unsaved(&three_points()));
}

#[test]
fn deleted_saved_route_counts_as_unsaved() {
    let app = app_with_ramp("unsaved-deleted");
    let request = uphill();
    app.plan_route(&request).unwrap();
    let id = app.save_planned_route("Tour").unwrap().id;
    app.delete_route(id).unwrap();
    assert!(app.route_is_unsaved(&request));
}
