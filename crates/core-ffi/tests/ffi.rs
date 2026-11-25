//! The core as the mobile apps see it.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use outdoor_core::{
    CoreApp, CoreError, CoreListener, FixInput, GuidanceInfo, LegInput, RecordingInfo,
    plan_add_waypoint, plan_empty,
};

struct Quiet;
impl CoreListener for Quiet {
    fn on_error(&self, _message: String) {}
    fn on_region_progress(&self, _region: String, _done: u64, _total: u64) {}
    fn on_recording(&self, _status: RecordingInfo) {}
    fn on_guidance(&self, _status: Option<GuidanceInfo>) {}
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A fresh data directory and a copy of the `mini` fixture region beside it.
fn setup(name: &str) -> (PathBuf, PathBuf) {
    let base = std::env::temp_dir().join(format!("core-ffi-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let data = base.join("data");
    let package = base.join("import/mini");
    fs::create_dir_all(&data).unwrap();
    fs::create_dir_all(&package).unwrap();
    for entry in fs::read_dir(repo().join("fixtures/regions/mini")).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), package.join(entry.file_name())).unwrap();
    }
    (data, package)
}

fn open(data: &Path) -> Arc<CoreApp> {
    CoreApp::open(data.display().to_string(), Arc::new(Quiet)).unwrap()
}

/// The slippy-map tile containing a position.
fn tile_of(lat: f64, lon: f64, z: u8) -> (u32, u32) {
    let n = f64::from(1u32 << z);
    let x = ((lon + 180.0) / 360.0 * n).floor();
    let lat = lat.to_radians();
    let y = ((1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / std::f64::consts::PI) / 2.0 * n).floor();
    (x as u32, y as u32)
}

#[test]
fn open_creates_database() {
    let (data, _) = setup("open");
    let core = open(&data);
    assert!(data.join("outdoor.sqlite").is_file());
    let overview = core.overview().unwrap();
    assert!(overview.regions.is_empty());
    assert_eq!(overview.bounds, None);
}

#[test]
fn install_local_then_overview() {
    let (data, package) = setup("install");
    let core = open(&data);
    let region = core
        .install_local_region(package.display().to_string())
        .unwrap();
    assert_eq!(region.id, "mini");
    assert_eq!(region.name, "Sylvenstein (Test)");
    assert_eq!(region.version, "202611130000");
    let overview = core.overview().unwrap();
    assert_eq!(overview.regions.len(), 1);
    assert_eq!(overview.bounds, Some(vec![11.54, 47.57, 11.56, 47.58]));
    assert!(overview.attribution.contains(&"© OpenMapTiles".to_string()));
}

#[test]
fn tile_inside_and_outside() {
    let (data, package) = setup("tile");
    let core = open(&data);
    core.install_local_region(package.display().to_string())
        .unwrap();
    let (x, y) = tile_of(47.575, 11.55, 12);
    let tile = core
        .tile(format!("/basemap/12/{x}/{y}"))
        .unwrap()
        .expect("tile inside mini");
    assert!(!tile.data.is_empty());
    assert_eq!(tile.mime, "application/x-protobuf");
    // Tile 0/0/0 covers the whole world, mini included; take one far away.
    let (x, y) = tile_of(0.0, 0.0, 12);
    assert!(core.tile(format!("/basemap/12/{x}/{y}")).unwrap().is_none());
    match core.tile("/nope/1/2/3".into()) {
        Err(CoreError::Failed { code, .. }) => assert_eq!(code, "tile_path"),
        other => panic!("expected tile_path error, got {other:?}"),
    }
}

#[test]
fn style_has_tiles_base() {
    let (data, package) = setup("style");
    let core = open(&data);
    core.install_local_region(package.display().to_string())
        .unwrap();
    let json = fs::read_to_string(repo().join("styles/topo-summer.json")).unwrap();
    let style = core
        .offline_style(json, "outdoor-tiles://localhost/".into(), false)
        .unwrap();
    assert!(style.contains("outdoor-tiles://localhost/basemap/"));
    let parsed: serde_json::Value = serde_json::from_str(&style).unwrap();
    assert!(parsed["sources"].get("contours").is_none());
}

#[test]
fn style_can_be_dark() {
    let (data, package) = setup("dark-style");
    let core = open(&data);
    core.install_local_region(package.display().to_string())
        .unwrap();
    let json = fs::read_to_string(repo().join("styles/topo-summer.json")).unwrap();
    let template: serde_json::Value = serde_json::from_str(&json).unwrap();
    let background =
        |style: &serde_json::Value| style["layers"][0]["paint"]["background-color"].clone();
    let light: serde_json::Value = serde_json::from_str(
        &core
            .offline_style(json.clone(), "outdoor-tiles://localhost/".into(), false)
            .unwrap(),
    )
    .unwrap();
    let dark: serde_json::Value = serde_json::from_str(
        &core
            .offline_style(json, "outdoor-tiles://localhost/".into(), true)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(background(&light), background(&template));
    assert_eq!(background(&dark), "#000000");
}

#[test]
fn style_uses_contour_tiles_when_installed() {
    let (data, package) = setup("contours");
    let core = open(&data);
    let json = fs::read_to_string(repo().join("styles/topo-summer.json")).unwrap();
    core.install_local_region(package.display().to_string())
        .unwrap();
    let plain: serde_json::Value = serde_json::from_str(
        &core
            .offline_style(json.clone(), "outdoor-tiles://localhost/".into(), false)
            .unwrap(),
    )
    .unwrap();
    assert!(plain["sources"].get("contours").is_none());

    // A second region, with contour lines (fixture `mini-contours`).
    let with_contours = data.parent().unwrap().join("import/mini-contours");
    fs::create_dir_all(&with_contours).unwrap();
    for entry in fs::read_dir(repo().join("fixtures/regions/mini-contours")).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), with_contours.join(entry.file_name())).unwrap();
    }
    core.install_local_region(with_contours.display().to_string())
        .unwrap();

    let style: serde_json::Value = serde_json::from_str(
        &core
            .offline_style(json, "outdoor-tiles://localhost/".into(), false)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        style["sources"]["contours"]["tiles"][0],
        "outdoor-tiles://localhost/contours/{z}/{x}/{y}"
    );
}

fn fix(i: i64) -> FixInput {
    FixInput {
        lat: 47.572 + i as f64 * 0.0002,
        lon: 11.543 + i as f64 * 0.0002,
        ele: Some(800.0 + i as f64),
        time_ms: 1_700_000_000_000 + i * 4000,
        h_accuracy_m: Some(5.0),
        speed_mps: Some(1.4),
        course_deg: Some(45.0),
    }
}

#[test]
fn recording_round_trip_over_ffi() {
    let (data, _) = setup("recording");
    let core = open(&data);
    let started = core.start_recording("hike".into()).unwrap();
    assert_eq!(started.phase, "recording");
    assert_eq!(started.activity.as_deref(), Some("hike"));
    for i in 0..20 {
        core.handle_fix(fix(i));
    }
    let status = core.recording_status();
    assert_eq!(status.phase, "recording");
    assert!(status.distance_m > 100.0, "{status:?}");
    assert_eq!(
        core.recording_line().iter().map(Vec::len).sum::<usize>(),
        20
    );
    let summary = core.stop_recording().unwrap().unwrap();
    assert_eq!(summary.activity, "hike");
    let tracks = core.list_tracks().unwrap();
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].id, summary.id);
    let view = core.track_detail(summary.id.clone()).unwrap();
    assert_eq!(view.segments.iter().map(Vec::len).sum::<usize>(), 20);
    assert!(view.elapsed_s > 70.0);
    core.rename_track(summary.id.clone(), "Morning".into())
        .unwrap();
    assert_eq!(
        core.list_tracks().unwrap()[0].name.as_deref(),
        Some("Morning")
    );
    assert!(
        core.export_track_gpx(summary.id.clone())
            .unwrap()
            .contains("<trkpt")
    );
    core.delete_track(summary.id).unwrap();
    assert!(core.list_tracks().unwrap().is_empty());
}

#[test]
fn unknown_activity_is_an_error() {
    let (data, _) = setup("activity");
    let core = open(&data);
    let CoreError::Failed { code, .. } = core.start_recording("swim".into()).unwrap_err();
    assert_eq!(code, "unknown_activity");
    assert_eq!(core.recording_status().phase, "idle");
}

#[derive(Default)]
struct Counting {
    recording: Mutex<u32>,
    guidance: Mutex<Vec<Option<String>>>,
}

impl CoreListener for Counting {
    fn on_error(&self, _message: String) {}
    fn on_region_progress(&self, _region: String, _done: u64, _total: u64) {}
    fn on_recording(&self, _status: RecordingInfo) {
        *self.recording.lock().unwrap() += 1;
    }
    fn on_guidance(&self, status: Option<GuidanceInfo>) {
        self.guidance.lock().unwrap().push(status.map(|s| s.phase));
    }
}

#[test]
fn listener_gets_recording_and_guidance() {
    let (data, package) = setup("listener");
    let listener = Arc::new(Counting::default());
    let core = CoreApp::open(data.display().to_string(), listener.clone()).unwrap();
    core.install_local_region(package.display().to_string())
        .unwrap();
    let waypoints = [fix(0), fix(10)].map(|f| outdoor_core::LatLon {
        lat: f.lat,
        lon: f.lon,
    });
    let request = waypoints.iter().fold(plan_empty(), |request, at| {
        plan_add_waypoint(
            request,
            *at,
            LegInput {
                profile: "manual".into(),
                options: Vec::new(),
            },
        )
    });
    core.plan_route(request).unwrap();

    core.start_recording("hike".into()).unwrap();
    for i in 0..5 {
        core.handle_fix(fix(i));
    }
    assert!(*listener.recording.lock().unwrap() > 0);
    // A recording does not start the guidance; it is started on its own.
    assert!(core.guidance_status().is_none());
    core.start_guidance().unwrap();
    assert!(core.guidance_status().is_some());
    assert!(
        listener
            .guidance
            .lock()
            .unwrap()
            .iter()
            .any(Option::is_some)
    );

    core.stop_guidance();
    assert_eq!(core.guidance_status(), None);
    assert_eq!(listener.guidance.lock().unwrap().last(), Some(&None));
}
