#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use core_app::{App, AppError, AppListener, RecordingStatus, RegionState};
use core_location::{Fix, SourceStatus};
use core_maps::{Layer, Progress};

#[derive(Default)]
struct Listener(Mutex<Vec<Progress>>);

impl AppListener for Listener {
    fn on_position(&self, _: &Fix) {}
    fn on_recording(&self, _: &RecordingStatus) {}
    fn on_source_status(&self, _: &SourceStatus) {}
    fn on_error(&self, _: &str) {}
    fn on_region_progress(&self, progress: &Progress) {
        self.0.lock().unwrap().push(progress.clone());
    }
}

fn fixtures() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/regions")
        .to_string_lossy()
        .into_owned()
}

fn temp_db(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("core-app-regions-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("app.sqlite")
}

fn open(name: &str) -> (App, Arc<Listener>, PathBuf) {
    let path = temp_db(name);
    let listener = Arc::new(Listener::default());
    (App::open(&path, listener.clone()).unwrap(), listener, path)
}

#[test]
fn without_a_catalog_source_nothing_is_offered() {
    let (app, _, _) = open("no-source");
    let overview = app.regions().unwrap();
    assert_eq!(overview.source, "");
    assert!(overview.regions.is_empty());
    assert_eq!(overview.catalog_error, None);
    assert!(matches!(
        app.install_region("mini"),
        Err(AppError::NoCatalog)
    ));
}

#[test]
fn catalog_regions_are_offered_for_download() {
    let (app, _, _) = open("offered");
    app.set_catalog_source(&fixtures()).unwrap();

    let overview = app.regions().unwrap();

    assert_eq!(overview.source, fixtures());
    assert_eq!(overview.regions.len(), 1);
    let region = &overview.regions[0];
    assert_eq!(region.id, "mini");
    assert_eq!(region.name, "Sylvenstein (Test)");
    assert_eq!(region.state, RegionState::Available);
    assert_eq!(region.size, 842_866);
    assert_eq!(region.installed_version, None);
    assert!(overview.attribution.is_empty());
    assert_eq!(overview.installed_bounds, None);
}

#[test]
fn installed_region_serves_tiles_and_is_reported_as_installed() {
    let (app, listener, _) = open("install");
    app.set_catalog_source(&fixtures()).unwrap();
    assert!(app.map_tile(Layer::Basemap, 0, 0, 0).unwrap().is_none());

    app.install_region("mini").unwrap();

    assert!(app.map_tile(Layer::Basemap, 0, 0, 0).unwrap().is_some());
    let overview = app.regions().unwrap();
    assert_eq!(overview.regions[0].state, RegionState::Installed);
    assert_eq!(
        overview.regions[0].installed_version.as_deref(),
        Some("202611130000")
    );
    assert!(
        overview
            .attribution
            .iter()
            .any(|line| line.contains("OpenStreetMap"))
    );
    assert_eq!(
        overview.installed_bounds,
        Some([11.54, 47.57, 11.56, 47.58])
    );

    let progress = listener.0.lock().unwrap().clone();
    assert!(
        progress.len() >= 2,
        "first and last progress are always reported"
    );
    assert_eq!(
        progress.last().map(|p| (p.done, p.total)),
        Some((842_866, 842_866))
    );
}

#[test]
fn installed_regions_stay_usable_when_the_catalog_is_unreachable() {
    let (app, _, _) = open("offline");
    app.set_catalog_source(&fixtures()).unwrap();
    app.install_region("mini").unwrap();
    app.set_catalog_source("/nonexistent/catalog").unwrap();

    let overview = app.regions().unwrap();

    assert!(overview.catalog_error.is_some());
    assert_eq!(overview.regions.len(), 1);
    assert_eq!(overview.regions[0].state, RegionState::Installed);
    assert!(app.map_tile(Layer::Basemap, 0, 0, 0).unwrap().is_some());
}

#[test]
fn newer_catalog_version_is_reported_as_update() {
    let (app, _, path) = open("update");
    app.set_catalog_source(&fixtures()).unwrap();
    app.install_region("mini").unwrap();

    // A catalog that announces a newer build of the same region.
    let newer = path.parent().unwrap().join("newer");
    std::fs::create_dir_all(&newer).unwrap();
    let catalog = std::fs::read_to_string(format!("{}/catalog.json", fixtures())).unwrap();
    std::fs::write(
        newer.join("catalog.json"),
        catalog.replace("202611130000", "202701010000"),
    )
    .unwrap();
    app.set_catalog_source(&newer.to_string_lossy()).unwrap();

    let overview = app.regions().unwrap();
    assert_eq!(overview.regions[0].state, RegionState::UpdateAvailable);
    assert_eq!(
        overview.regions[0].installed_version.as_deref(),
        Some("202611130000")
    );
    assert_eq!(overview.regions[0].version, "202701010000");
}

#[test]
fn deleted_region_is_offered_again() {
    let (app, _, _) = open("delete");
    app.set_catalog_source(&fixtures()).unwrap();
    app.install_region("mini").unwrap();

    app.delete_region("mini").unwrap();

    assert!(app.map_tile(Layer::Basemap, 0, 0, 0).unwrap().is_none());
    assert_eq!(
        app.regions().unwrap().regions[0].state,
        RegionState::Available
    );
}

#[test]
fn catalog_source_and_regions_survive_a_restart() {
    let (app, listener, path) = open("restart");
    app.set_catalog_source(&fixtures()).unwrap();
    app.install_region("mini").unwrap();
    drop(app);

    let app = App::open(&path, listener).unwrap();
    let overview = app.regions().unwrap();
    assert_eq!(overview.source, fixtures());
    assert_eq!(overview.regions[0].state, RegionState::Installed);
    assert!(
        app.map_tile(Layer::Outdoor, 13, 4358, 2862)
            .unwrap()
            .is_some()
    );
}

#[test]
fn cancelling_without_a_download_is_harmless() {
    let (app, _, _) = open("cancel");
    app.cancel_region_download("mini");
}

/// Writes a region definition next to the database, as the desktop does for
/// regions chosen on the map.
fn define(db: &std::path::Path, id: &str, json: &str) -> PathBuf {
    let dir = db.parent().unwrap().join("region-defs");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{id}.json"));
    std::fs::write(&path, json).unwrap();
    path
}

const MINI_DEFINITION: &str = r#"{"id": "mini", "name": "Sylvenstein (selbst gebaut)", "bbox": [11.54, 47.57, 11.56, 47.58], "osm": []}"#;

/// A copy of the fixture region, as a freshly built package.
fn package(name: &str) -> PathBuf {
    let dir = temp_db(name).parent().unwrap().join("mini");
    std::fs::create_dir_all(&dir).unwrap();
    let fixture = PathBuf::from(fixtures()).join("mini");
    for entry in std::fs::read_dir(fixture).unwrap() {
        let entry = entry.unwrap();
        std::fs::copy(entry.path(), dir.join(entry.file_name())).unwrap();
    }
    dir
}

#[test]
fn defined_region_is_listed_as_not_built() {
    let (app, _, db) = open("defined");
    define(&db, "mini", MINI_DEFINITION);

    let overview = app.regions().unwrap();

    assert_eq!(overview.regions.len(), 1);
    let region = &overview.regions[0];
    assert_eq!(region.id, "mini");
    assert_eq!(region.name, "Sylvenstein (selbst gebaut)");
    assert_eq!(region.bbox, [11.54, 47.57, 11.56, 47.58]);
    assert_eq!(region.state, RegionState::NotBuilt);
    assert_eq!(region.size, 0);
    assert!(region.buildable);
    assert_eq!(overview.installed_bounds, None);
}

#[test]
fn local_package_is_installed() {
    let (app, _, db) = open("local");
    define(&db, "mini", MINI_DEFINITION);

    let installed = app.install_local_region(&package("local-pkg")).unwrap();

    assert_eq!(installed.manifest.id, "mini");
    let overview = app.regions().unwrap();
    assert_eq!(overview.regions.len(), 1);
    assert_eq!(overview.regions[0].state, RegionState::Installed);
    assert!(overview.regions[0].buildable);
    assert!(app.map_tile(Layer::Basemap, 0, 0, 0).unwrap().is_some());
}

#[test]
fn region_without_definition_is_not_buildable() {
    let (app, _, _) = open("not-buildable");
    app.set_catalog_source(&fixtures()).unwrap();
    assert!(!app.regions().unwrap().regions[0].buildable);
    app.install_region("mini").unwrap();
    assert!(!app.regions().unwrap().regions[0].buildable);
}

#[test]
fn deleting_removes_the_definition() {
    let (app, _, db) = open("delete-def");
    let definition = define(&db, "mini", MINI_DEFINITION);
    app.install_local_region(&package("delete-def-pkg"))
        .unwrap();

    app.delete_region("mini").unwrap();

    assert!(!definition.exists());
    assert!(app.regions().unwrap().regions.is_empty());
}

#[test]
fn deleting_a_region_that_was_never_built_removes_its_definition() {
    let (app, _, db) = open("delete-unbuilt");
    let definition = define(&db, "mini", MINI_DEFINITION);

    app.delete_region("mini").unwrap();

    assert!(!definition.exists());
    assert!(matches!(
        app.delete_region("mini"),
        Err(AppError::Maps(core_maps::MapsError::NotInstalled(_)))
    ));
}

#[test]
fn broken_definitions_are_ignored() {
    let (app, _, db) = open("broken-def");
    define(&db, "broken", "{");
    define(
        &db,
        "other",
        r#"{"id": "mismatch", "name": "x", "bbox": [1, 2, 3, 4]}"#,
    );
    define(&db, "nobox", r#"{"id": "nobox", "name": "x"}"#);
    std::fs::write(db.parent().unwrap().join("region-defs/notes.txt"), "x").unwrap();
    assert!(app.regions().unwrap().regions.is_empty());
}
