//! Route planning as the mobile apps see it.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use outdoor_core::{
    CoreApp, CoreError, CoreListener, EngineFailure, LatLon, LegInput, OptionValue, PlanInput,
    ValhallaEngine, failed_leg_color, plan_add_waypoint, plan_empty, plan_move_waypoint,
    plan_remove_waypoint, plan_toggle_stage_end,
};
use sha2::{Digest, Sha256};

struct Quiet;
impl CoreListener for Quiet {
    fn on_error(&self, _message: String) {}
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A data directory with the region `mini` installed; with `routing`, the
/// package also has (dummy) routing data built by Valhalla 3.9.1.
fn core_with_mini(name: &str, routing: bool) -> Arc<CoreApp> {
    let base = std::env::temp_dir().join(format!("core-ffi-plan-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let (data, package) = (base.join("data"), base.join("import/mini"));
    fs::create_dir_all(&data).unwrap();
    fs::create_dir_all(&package).unwrap();
    for entry in fs::read_dir(repo().join("fixtures/regions/mini")).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), package.join(entry.file_name())).unwrap();
    }
    if routing {
        let tar = b"dummy routing tiles";
        fs::write(package.join("routing.tar"), tar).unwrap();
        let path = package.join("manifest.json");
        let mut manifest: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        manifest["files"].as_array_mut().unwrap().push(serde_json::json!({
            "name": "routing.tar", "size": tar.len(),
            "sha256": Sha256::digest(tar).iter().map(|b| format!("{b:02x}")).collect::<String>(),
        }));
        manifest["size"] = serde_json::json!(manifest["size"].as_u64().unwrap() + tar.len() as u64);
        manifest["routing"] = serde_json::json!({ "valhalla": "3.9.1", "max_mtb_scale": 2 });
        fs::write(&path, manifest.to_string()).unwrap();
    }
    let core = CoreApp::open(data.display().to_string(), Arc::new(Quiet)).unwrap();
    core.install_local_region(package.display().to_string())
        .unwrap();
    core
}

fn ll(lat: f64, lon: f64) -> LatLon {
    LatLon { lat, lon }
}

fn leg(profile: &str) -> LegInput {
    LegInput {
        profile: profile.into(),
        options: Vec::new(),
    }
}

/// Points inside `mini` (11.54–11.56 E, 47.57–47.58 N).
fn points() -> [LatLon; 3] {
    [ll(47.572, 11.543), ll(47.578, 11.55), ll(47.573, 11.558)]
}

fn plan(profile: &str, waypoints: &[LatLon]) -> PlanInput {
    waypoints.iter().fold(plan_empty(), |request, at| {
        plan_add_waypoint(request, *at, leg(profile))
    })
}

fn encode_polyline6(points: &[(f64, f64)]) -> String {
    let mut out = String::new();
    let (mut last_lat, mut last_lon) = (0i64, 0i64);
    for (lat, lon) in points {
        let (lat, lon) = ((lat * 1e6).round() as i64, (lon * 1e6).round() as i64);
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

/// Answers every request with a straight line between its locations, 1 km long.
struct FakeEngine {
    version: String,
    asked: Mutex<Vec<(String, String)>>,
}

impl ValhallaEngine for FakeEngine {
    fn version(&self) -> String {
        self.version.clone()
    }

    fn route(&self, tiles_path: String, request_json: String) -> Result<String, EngineFailure> {
        let request: serde_json::Value = serde_json::from_str(&request_json).unwrap();
        self.asked.lock().unwrap().push((tiles_path, request_json));
        let line: Vec<(f64, f64)> = request["locations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| (l["lat"].as_f64().unwrap(), l["lon"].as_f64().unwrap()))
            .collect();
        Ok(serde_json::json!({ "trip": {
            "legs": [{ "shape": encode_polyline6(&line), "summary": { "length": 1.0, "time": 600.0 } }],
            "summary": { "length": 1.0, "time": 600.0 }
        }})
        .to_string())
    }
}

fn engine(version: &str) -> Arc<FakeEngine> {
    Arc::new(FakeEngine {
        version: version.into(),
        asked: Mutex::new(Vec::new()),
    })
}

fn code(error: CoreError) -> String {
    let CoreError::Failed { code, .. } = error;
    code
}

#[test]
fn manual_plan_without_engine() {
    let core = core_with_mini("manual", false);
    let result = core.plan_route(plan("manual", &points())).unwrap();
    assert_eq!(result.legs.len(), 2);
    assert!(result.total.distance_m > 0.0);
    assert_eq!(result.total.failed_legs, 0);
    assert!(result.elevation.iter().all(|sample| sample.ele.is_some()));
    assert_eq!(result.legs[0].color, "#6a1b9a");
    assert_eq!(result.request, plan("manual", &points()));
    assert_eq!(result.stages.len(), 1);
}

#[test]
fn engine_receives_costing_json() {
    let core = core_with_mini("engine", true);
    let engine = engine("3.9.1");
    core.set_valhalla_engine(engine.clone());
    let result = core.plan_route(plan("hike", &points()[..2])).unwrap();
    assert_eq!(
        result.total.failed_legs, 0,
        "{:?}",
        result.legs[0].error_message
    );
    assert!((result.legs[0].distance_m - 1000.0).abs() < 1e-6);
    assert_eq!(result.legs[0].line.len(), 2);
    let asked = engine.asked.lock().unwrap();
    assert_eq!(asked.len(), 1);
    assert!(asked[0].0.ends_with("routing.tar"), "{}", asked[0].0);
    assert!(Path::new(&asked[0].0).is_file());
    assert!(
        asked[0].1.contains("\"costing\":\"pedestrian\""),
        "{}",
        asked[0].1
    );
}

#[test]
fn incompatible_engine_is_reported_per_leg() {
    let core = core_with_mini("incompatible", true);
    core.set_valhalla_engine(engine("3.8.0"));
    let result = core.plan_route(plan("hike", &points()[..2])).unwrap();
    assert_eq!(
        result.legs[0].error_code.as_deref(),
        Some("routing_incompatible")
    );
    assert_eq!(result.total.failed_legs, 1);
}

#[test]
fn leg_outside_region_fails_alone() {
    let core = core_with_mini("outside", true);
    core.set_valhalla_engine(engine("3.9.1"));
    let mut waypoints = points()[..2].to_vec();
    waypoints.push(ll(48.5, 12.5));
    let result = core.plan_route(plan("hike", &waypoints)).unwrap();
    assert_eq!(result.legs[0].error_code, None);
    assert_eq!(
        result.legs[1].error_code.as_deref(),
        Some("routing_unavailable")
    );
    assert!(result.legs[1].error_message.is_some());
    assert_eq!(result.total.failed_legs, 1);
    assert!((result.total.distance_m - 1000.0).abs() < 1e-6);
}

#[test]
fn edit_functions_roundtrip() {
    let request = plan("manual", &points());
    let marked = plan_toggle_stage_end(request.clone(), 1);
    assert_eq!(marked.stage_ends, vec![1]);
    let removed = plan_remove_waypoint(marked, 1);
    assert_eq!(removed.stage_ends, Vec::<u32>::new());
    assert_eq!(removed.waypoints.len(), 2);
    let moved = plan_move_waypoint(request, 2, ll(47.575, 11.545));
    assert_eq!(moved.waypoints[2], ll(47.575, 11.545));
    assert_eq!(failed_leg_color(), "#d93025");
}

#[test]
fn options_go_through() {
    let core = core_with_mini("options", true);
    let engine = engine("3.9.1");
    core.set_valhalla_engine(engine.clone());
    let mut request = plan("hike", &points()[..2]);
    request.legs[0].options = vec![
        OptionValue {
            key: "max_hiking_difficulty".into(),
            value: 5.0,
        },
        OptionValue {
            key: "use_hills".into(),
            value: 0.3,
        },
    ];
    let result = core.plan_route(request.clone()).unwrap();
    let asked = &engine.asked.lock().unwrap()[0].1;
    assert!(asked.contains("\"max_hiking_difficulty\":5"), "{asked}");
    assert!(asked.contains("\"use_hills\":0.3"), "{asked}");
    // Back in the core's order of the settings.
    let mut back = result.request.legs[0].options.clone();
    back.sort_by(|a, b| a.key.cmp(&b.key));
    assert_eq!(back, request.legs[0].options, "options come back as given");

    let mut unknown = plan("hike", &points()[..2]);
    unknown.legs[0].options = vec![OptionValue {
        key: "speed".into(),
        value: 1.0,
    }];
    assert_eq!(code(core.plan_route(unknown).unwrap_err()), "route_plan");
    let mut hovercraft = plan("hike", &points()[..2]);
    hovercraft.legs[0].profile = "hovercraft".into();
    assert_eq!(code(core.plan_route(hovercraft).unwrap_err()), "route_plan");
    // The edit functions leave a request they cannot read as it is.
    let request = plan("hike", &points()[..2]);
    assert_eq!(
        plan_add_waypoint(request.clone(), points()[2], leg("hovercraft")),
        request
    );
}

#[test]
fn save_load_overwrite_via_ffi() {
    let core = core_with_mini("save", false);
    assert_eq!(core.loaded_route_id(), None);
    core.plan_route(plan("manual", &points()[..2])).unwrap();
    let first = core.save_route("Sylvenstein".into()).unwrap();
    assert_eq!(
        (first.rev, first.name.as_str(), first.profile.as_str()),
        (1, "Sylvenstein", "manual")
    );
    assert_eq!(core.loaded_route_id(), Some(first.id.clone()));

    core.clear_planned_route();
    assert_eq!(
        core.loaded_route_id(),
        Some(first.id.clone()),
        "only cleared"
    );
    core.start_new_route();
    assert_eq!(core.loaded_route_id(), None);
    let loaded = core.load_route(first.id.clone()).unwrap();
    assert_eq!(loaded.request.waypoints.len(), 2);
    core.plan_route(plan("manual", &points())).unwrap();
    let second = core.save_route("Sylvenstein".into()).unwrap();
    assert_eq!((second.id.as_str(), second.rev), (first.id.as_str(), 2));

    let copy = core.save_route_as_new("Kopie".into()).unwrap();
    assert_ne!(copy.id, first.id);
    core.rename_route(copy.id.clone(), "Kopie 2".into())
        .unwrap();
    let names: Vec<String> = core
        .saved_routes()
        .unwrap()
        .into_iter()
        .map(|r| r.name)
        .collect();
    assert!(names.contains(&"Kopie 2".to_string()) && names.len() == 2);
    core.delete_route(copy.id).unwrap();
    assert_eq!(core.saved_routes().unwrap().len(), 1);
    assert_eq!(
        code(core.load_route("not-a-uuid".into()).unwrap_err()),
        "route_id"
    );
}

#[test]
fn profiles_have_colors() {
    let core = core_with_mini("profiles", false);
    let profiles = core.route_profiles();
    let hike = profiles.iter().find(|p| p.profile == "hike").unwrap();
    assert_eq!(hike.color, "#c62828");
    // Without an engine "hike" is available only where core-app has the
    // built-in Valhalla (feature `valhalla`, as in a workspace build with the
    // desktop); the iOS build has not.
    assert!(
        hike.options
            .iter()
            .any(|o| o.key == "max_hiking_difficulty")
    );
    assert!(
        profiles
            .iter()
            .find(|p| p.profile == "manual")
            .unwrap()
            .available
    );
    core.set_valhalla_engine(engine("3.9.1"));
    assert!(core.route_profiles().iter().all(|p| p.available));
}
