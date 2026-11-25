//! Guidance in the app: fixes, the planned route and the recording drive the
//! guide.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::Path;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use common::{Listener, open_with, position, ramp_routing_dir};
use core_app::{
    App, AppError, AppListener, GuidePhase, GuideStatus, LegSpec, PlanRequest, RecordingStatus,
};
use core_location::{Fix, SourceStatus};
use core_routing::{LatLon, Profile, Route, RouteOptions, RouteRequest, Router, RoutingError};
use core_track::Activity;

/// Straight lines on foot. When armed, the next call waits: it says it has
/// begun and goes on when told to.
#[derive(Default)]
struct StraightRouter {
    gate: Mutex<Option<(Sender<()>, Receiver<()>)>>,
}

impl StraightRouter {
    /// Arms the gate; returns where the router says it has begun and where
    /// it is told to go on.
    fn hold_next(&self) -> (Receiver<()>, Sender<()>) {
        let (begun_tx, begun_rx) = channel();
        let (go_tx, go_rx) = channel();
        *self.gate.lock().unwrap() = Some((begun_tx, go_rx));
        (begun_rx, go_tx)
    }
}

impl Router for StraightRouter {
    fn route(&self, request: &RouteRequest) -> Result<Route, RoutingError> {
        let gate = self.gate.lock().unwrap().take();
        if let Some((begun, go)) = gate {
            begun.send(()).unwrap();
            go.recv().unwrap();
        }
        let mut route = core_routing::manual_route(&request.waypoints)?;
        route.profile = request.profile;
        route.duration_s = Some(route.distance_m / 1.2);
        Ok(route)
    }

    fn profiles(&self) -> Vec<Profile> {
        vec![Profile::Hike]
    }
}

/// What the app told about guidance: the phase, or `None` when it ended.
#[derive(Default)]
struct Events {
    guidance: Mutex<Vec<Option<GuidePhase>>>,
}

impl Events {
    fn guidance(&self) -> Vec<Option<GuidePhase>> {
        self.guidance.lock().unwrap().clone()
    }
}

impl AppListener for Events {
    fn on_position(&self, _: &Fix) {}
    fn on_recording(&self, _: &RecordingStatus) {}
    fn on_source_status(&self, _: &SourceStatus) {}
    fn on_error(&self, _: &str) {}
    fn on_guidance(&self, status: Option<&GuideStatus>) {
        self.guidance.lock().unwrap().push(status.map(|s| s.phase));
    }
}

/// An app on the ramp region routing with `router`, telling `listener`.
fn app_on(dir: &Path, router: &Arc<StraightRouter>, listener: Arc<dyn AppListener>) -> App {
    let app = open_with(dir, listener);
    app.set_router_for_tests("ramp", router.clone());
    app
}

fn app(name: &str) -> (App, Arc<StraightRouter>) {
    let router = Arc::new(StraightRouter::default());
    (
        app_on(&ramp_routing_dir(name), &router, Arc::new(Listener)),
        router,
    )
}

/// A hike through `waypoints`.
fn hike(waypoints: Vec<LatLon>) -> PlanRequest {
    let legs = waypoints.len() - 1;
    PlanRequest {
        waypoints,
        legs: vec![
            LegSpec {
                profile: Profile::Hike,
                options: RouteOptions::default(),
            };
            legs
        ],
        stage_ends: Vec::new(),
    }
}

/// Four waypoints eastwards along row 32, about a kilometre apart.
fn route_east() -> PlanRequest {
    hike(
        (1..=4)
            .map(|i| position(f64::from(i) * 10.0, 32.0))
            .collect(),
    )
}

/// Times of the fixes: shortly before now, so that they count as recent.
fn start_ms() -> i64 {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    i64::try_from(now.as_millis()).unwrap() - 60_000
}

/// A fix at `at`, `s` seconds after `t0`, accurate to `accuracy_m`.
fn fix(at: LatLon, t0: i64, s: i64, accuracy_m: f32) -> Fix {
    Fix {
        lat: at.lat,
        lon: at.lon,
        ele: None,
        time_ms: t0 + s * 1000,
        h_accuracy_m: Some(accuracy_m),
        speed_mps: None,
        course_deg: None,
    }
}

/// On the route east, then a row (about 100 m) north of it for 10 s: a
/// deviation on the last fix.
fn leave_route_east(t0: i64) -> Vec<Fix> {
    vec![
        fix(position(12.0, 32.0), t0, 0, 8.0),
        fix(position(13.0, 31.0), t0, 5, 8.0),
        fix(position(14.0, 31.0), t0, 10, 8.0),
        fix(position(15.0, 31.0), t0, 15, 8.0),
    ]
}

fn phase(app: &App) -> Option<GuidePhase> {
    app.guidance_status().map(|s| s.phase)
}

#[test]
fn start_without_route_fails() {
    let (app, _) = app("no-route");

    let error = app.start_guidance().unwrap_err();

    assert!(matches!(error, AppError::NoRoutePlanned), "{error}");
    assert_eq!(error.code(), "no_route_planned");
    assert_eq!(app.guidance_status(), None);
}

#[test]
fn starts_on_first_usable_fix() {
    let (app, _) = app("first-fix");
    app.plan_route(&route_east()).unwrap();
    let t0 = start_ms();

    assert_eq!(app.start_guidance().unwrap().phase, GuidePhase::Waiting);

    app.handle_fix(fix(position(29.0, 32.0), t0, 0, 120.0));
    assert_eq!(phase(&app), Some(GuidePhase::Waiting), "too inaccurate");

    app.handle_fix(fix(position(29.5, 32.0), t0, 1, 8.0));
    let status = app.guidance_status().unwrap();
    assert_eq!(status.phase, GuidePhase::OnRoute);
    assert_eq!(status.next_waypoint, Some(2));
}

/// Recording and guidance are independent: a recording never starts the
/// guidance, also not when a route is loaded during it.
#[test]
fn recording_does_not_start_guidance() {
    let (app, _) = app("record-route");
    app.plan_route(&route_east()).unwrap();
    let route = app.save_planned_route("Ost").unwrap();

    app.start_recording(Activity::Hike).unwrap();
    assert_eq!(app.guidance_status(), None);

    app.start_new_route();
    app.saved_route(route.id).unwrap();
    assert_eq!(app.guidance_status(), None, "loaded");
    app.plan_route(&route_east()).unwrap();
    assert_eq!(app.guidance_status(), None, "planned");
}

/// Ending one leaves the other running.
#[test]
fn recording_and_guidance_end_independently() {
    let (app, _) = app("independent");
    app.plan_route(&route_east()).unwrap();
    app.start_recording(Activity::Hike).unwrap();
    app.start_guidance().unwrap();

    app.stop_guidance();
    assert_eq!(app.guidance_status(), None);
    assert!(app.recording_status().track_id.is_some());

    app.start_guidance().unwrap();
    app.stop_recording().unwrap();
    assert_eq!(phase(&app), Some(GuidePhase::Waiting));
}

#[test]
fn plan_change_drops_stale_way_back() {
    let (app, router) = app("stale-way-back");
    app.plan_route(&route_east()).unwrap();
    app.start_guidance().unwrap();
    let fixes = leave_route_east(start_ms());
    // Through the last fix, where the user left the old route.
    let new_route = hike(vec![
        position(10.0, 31.0),
        position(20.0, 31.0),
        position(30.0, 31.0),
    ]);

    let (begun, go) = router.hold_next();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            for fix in &fixes {
                app.handle_fix(*fix);
            }
        });
        begun.recv().unwrap();
        app.plan_route(&new_route).unwrap();
        go.send(()).unwrap();
    });

    let status = app.guidance_status().unwrap();
    assert_eq!(status.way_back, None);
    assert_eq!(status.phase, GuidePhase::OnRoute);
    assert!(status.offset_m.unwrap() < 20.0, "{:?}", status.offset_m);
    let left_m = position(15.0, 31.0).distance_m(&position(30.0, 31.0));
    let to_go = status.to_destination.distance_m;
    assert!((to_go - left_m).abs() < 30.0, "{to_go} vs {left_m}");
}

#[test]
fn clearing_the_route_ends_guidance() {
    let events = Arc::new(Events::default());
    let router = Arc::new(StraightRouter::default());
    let app = app_on(&ramp_routing_dir("clear"), &router, events.clone());
    app.plan_route(&route_east()).unwrap();
    app.start_guidance().unwrap();

    app.clear_planned_route();

    assert_eq!(app.guidance_status(), None);
    assert_eq!(
        events.guidance(),
        vec![Some(GuidePhase::Waiting), None],
        "started, then ended"
    );
}

#[test]
fn guidance_never_changes_the_plan() {
    let (app, _) = app("unchanged");
    app.plan_route(&route_east()).unwrap();
    let before = app.planned_route();
    app.start_guidance().unwrap();

    for fix in leave_route_east(start_ms()) {
        app.handle_fix(fix);
    }

    let status = app.guidance_status().unwrap();
    assert_eq!(status.phase, GuidePhase::OffRoute);
    assert!(status.way_back.is_some());
    assert_eq!(app.planned_route(), before);
}

/// Records the guidance it is told about; when armed, it holds the next
/// status inside `on_guidance` until told to go on, before recording it.
#[derive(Default)]
struct HoldingEvents {
    told: Mutex<Vec<Option<GuidePhase>>>,
    gate: Mutex<Option<(Sender<()>, Receiver<()>)>>,
}

impl AppListener for HoldingEvents {
    fn on_position(&self, _: &Fix) {}
    fn on_recording(&self, _: &RecordingStatus) {}
    fn on_source_status(&self, _: &SourceStatus) {}
    fn on_error(&self, _: &str) {}
    fn on_guidance(&self, status: Option<&GuideStatus>) {
        let gate = if status.is_some() {
            self.gate.lock().unwrap().take()
        } else {
            None
        };
        if let Some((begun, go)) = gate {
            begun.send(()).unwrap();
            go.recv().unwrap();
        }
        self.told.lock().unwrap().push(status.map(|s| s.phase));
    }
}

/// A status of a fix that is still being told when the guidance is stopped
/// reaches the listener before the end, never after it.
#[test]
fn listener_hears_the_end_last() {
    let router = Arc::new(StraightRouter::default());
    let events = Arc::new(HoldingEvents::default());
    let app = Arc::new(app_on(
        &ramp_routing_dir("told-order"),
        &router,
        events.clone(),
    ));
    app.plan_route(&route_east()).unwrap();
    app.start_guidance().unwrap();
    let (begun_tx, begun_rx) = channel();
    let (go_tx, go_rx) = channel();
    *events.gate.lock().unwrap() = Some((begun_tx, go_rx));

    let t0 = start_ms();
    let fixing = {
        let app = app.clone();
        std::thread::spawn(move || app.handle_fix(fix(position(12.0, 32.0), t0, 0, 8.0)))
    };
    begun_rx.recv().unwrap();
    // The fix's status is being told; the guide itself is free again.
    let stopping = {
        let app = app.clone();
        std::thread::spawn(move || app.stop_guidance())
    };
    std::thread::sleep(std::time::Duration::from_millis(100));
    go_tx.send(()).unwrap();
    fixing.join().unwrap();
    stopping.join().unwrap();

    assert_eq!(app.guidance_status(), None);
    let told = events.told.lock().unwrap().clone();
    assert_eq!(told.last(), Some(&None), "{told:?}");
    assert_eq!(told[told.len() - 2], Some(GuidePhase::OnRoute), "{told:?}");
}
