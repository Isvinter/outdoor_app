//! Region manager and route import as the apps see them, against a second
//! app served over HTTP.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use core_app::{App, AppListener, RecordingStatus};
use core_app::{LegSpec, PlanRequest};
use core_location::{Fix, SourceStatus};
use core_routing::{LatLon, Profile, RouteOptions};
use core_share::{ShareOptions, ShareServer};
use outdoor_core::{CoreApp, CoreListener};

#[derive(Default)]
struct Collect {
    progress: Mutex<Vec<(String, u64, u64)>>,
}

impl CoreListener for Collect {
    fn on_error(&self, _message: String) {}
    fn on_region_progress(&self, region: String, done: u64, total: u64) {
        self.progress.lock().unwrap().push((region, done, total));
    }
}

struct Quiet;
impl AppListener for Quiet {
    fn on_position(&self, _: &Fix) {}
    fn on_recording(&self, _: &RecordingStatus) {}
    fn on_source_status(&self, _: &SourceStatus) {}
    fn on_error(&self, _: &str) {}
}

fn base(name: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!("core-ffi-share-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();
    base
}

fn fixture_copy(to: &Path) -> PathBuf {
    let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/regions/mini");
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
    }
    to.to_path_buf()
}

fn phone(name: &str) -> (Arc<CoreApp>, Arc<Collect>) {
    let data = base(name).join("phone");
    fs::create_dir_all(&data).unwrap();
    let listener = Arc::new(Collect::default());
    let app = CoreApp::open(data.display().to_string(), listener.clone()).unwrap();
    (app, listener)
}

/// A desktop with the `mini` region installed, served on a free port.
fn desktop(name: &str) -> (Arc<App>, ShareServer, String) {
    let dir = base(&format!("{name}-desktop"));
    let app = Arc::new(App::open(&dir.join("outdoor.sqlite"), Arc::new(Quiet)).unwrap());
    app.install_local_region(&fixture_copy(&dir.join("pkg")))
        .unwrap();
    let server = ShareServer::start(
        "127.0.0.1:0".parse().unwrap(),
        Arc::clone(&app) as Arc<dyn core_share::ShareContent>,
        ShareOptions::default(),
    )
    .unwrap();
    let source = format!("127.0.0.1:{}", server.local_addr().port());
    (app, server, source)
}

#[test]
fn overview_returns_quickly_with_dead_source() {
    let (core, _) = phone("dead");
    core.set_catalog_source("10.255.255.1:9".into()).unwrap();
    let started = Instant::now();
    core.overview().unwrap();
    core.offline_style(
        r#"{"version":8,"sources":{},"layers":[]}"#.into(),
        "http://x".into(),
    )
    .ok();
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn source_is_normalised() {
    let (core, _) = phone("source");
    assert_eq!(
        core.set_catalog_source("192.168.1.5:8765".into()).unwrap(),
        "http://192.168.1.5:8765/"
    );
    assert_eq!(core.catalog_source().unwrap(), "http://192.168.1.5:8765/");
    assert_eq!(
        core.set_catalog_source("http://h/catalog.json".into())
            .unwrap(),
        "http://h/catalog.json"
    );
    assert_eq!(core.set_catalog_source(" ".into()).unwrap(), "");
    assert_eq!(core.catalog_source().unwrap(), "");
}

#[test]
fn install_over_http_reports_progress() {
    let (_desktop, _server, source) = desktop("install");
    let (core, listener) = phone("install");
    core.set_catalog_source(source).unwrap();
    let region = core.install_region("mini".into()).unwrap();
    assert_eq!(region.id, "mini");
    assert!(region.enabled);
    assert!(region.bytes > 0);
    let progress = listener.progress.lock().unwrap();
    assert!(
        progress
            .iter()
            .any(|(id, done, total)| id == "mini" && done == total && *total > 0)
    );
    let overview = core.overview().unwrap();
    assert!(overview.regions.iter().any(|r| r.id == "mini"));
}

#[test]
fn regions_shows_update_and_states() {
    let (_desktop, _server, source) = desktop("states");
    let (core, _) = phone("states");
    core.set_catalog_source(source).unwrap();
    let list = core.regions().unwrap();
    assert_eq!(list.catalog_error, None);
    assert_eq!(list.regions.len(), 1);
    assert_eq!(list.regions[0].state, "available");
    assert_eq!(list.regions[0].installed_version, None);
    core.install_region("mini".into()).unwrap();
    let list = core.regions().unwrap();
    assert_eq!(list.regions[0].state, "installed");
    assert!(list.regions[0].installed_version.is_some());
    assert!(list.source.starts_with("http://127.0.0.1:"));
}

#[test]
fn enable_toggle_roundtrip() {
    let (_desktop, _server, source) = desktop("toggle");
    let (core, _) = phone("toggle");
    core.set_catalog_source(source).unwrap();
    core.install_region("mini".into()).unwrap();
    core.set_region_enabled("mini".into(), false).unwrap();
    let overview = core.overview().unwrap();
    assert!(!overview.regions[0].enabled);
    assert_eq!(overview.bounds, None);
    core.set_region_enabled("mini".into(), true).unwrap();
    assert!(core.overview().unwrap().bounds.is_some());
    core.delete_region("mini".into()).unwrap();
    assert!(core.overview().unwrap().regions.is_empty());
}

#[test]
fn import_route_over_http() {
    let (desktop, _server, source) = desktop("route");
    let request = PlanRequest {
        waypoints: vec![
            LatLon {
                lat: 47.57,
                lon: 11.54,
            },
            LatLon {
                lat: 47.575,
                lon: 11.55,
            },
        ],
        legs: vec![LegSpec {
            profile: Profile::Manual,
            options: RouteOptions::default(),
        }],
        stage_ends: Vec::new(),
    };
    desktop.plan_route(&request).unwrap();
    let id = desktop.save_planned_route("Runde").unwrap().id.to_string();

    let (core, _) = phone("route");
    core.set_catalog_source(source).unwrap();
    let remote = core.remote_routes().unwrap();
    assert_eq!(remote.len(), 1);
    assert_eq!(remote[0].id, id);
    assert_eq!(remote[0].name, "Runde");
    assert_eq!(remote[0].state, "new");

    let imported = core.import_route(id.clone()).unwrap();
    assert_eq!(imported.outcome, "added");
    assert_eq!(imported.id, id);
    assert_eq!(imported.reloaded, None);
    assert_eq!(core.remote_routes().unwrap()[0].state, "up_to_date");
    assert!(core.saved_routes().unwrap().iter().any(|r| r.id == id));
}
