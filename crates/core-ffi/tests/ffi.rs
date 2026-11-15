//! The core as the mobile apps see it.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use outdoor_core::{CoreApp, CoreError, CoreListener};

struct Quiet;
impl CoreListener for Quiet {
    fn on_error(&self, _message: String) {}
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
        .offline_style(json, "outdoor-tiles://localhost/".into())
        .unwrap();
    assert!(style.contains("outdoor-tiles://localhost/basemap/"));
    let parsed: serde_json::Value = serde_json::from_str(&style).unwrap();
    assert!(parsed["sources"].get("contours").is_none());
}
