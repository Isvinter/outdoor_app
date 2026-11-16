#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::f64::consts::PI;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use core_app::{App, AppListener, RecordingStatus};
use core_location::{Fix, SourceStatus};
use core_maps::{ArchiveInfo, TileType, write_archive};
use core_routing::LatLon;

pub struct Listener;

impl AppListener for Listener {
    fn on_position(&self, _: &Fix) {}
    fn on_recording(&self, _: &RecordingStatus) {}
    fn on_source_status(&self, _: &SourceStatus) {}
    fn on_error(&self, _: &str) {}
}

pub const SIZE: usize = 64;
pub const Z: u8 = 12;
pub const X: u32 = 2179;
pub const Y: u32 = 1431;

/// Position of a pixel (0..SIZE) inside the elevation tile X/Y.
pub fn position(col: f64, row: f64) -> LatLon {
    let n = f64::from(1u32 << Z);
    let lon = (f64::from(X) + col / SIZE as f64) / n * 360.0 - 180.0;
    let lat = (PI * (1.0 - 2.0 * (f64::from(Y) + row / SIZE as f64) / n))
        .sinh()
        .atan()
        .to_degrees();
    LatLon { lat, lon }
}

/// A terrarium tile rising 2 m per pixel towards the east from 1000 m.
pub fn ramp_png() -> Vec<u8> {
    let mut rgb = Vec::with_capacity(SIZE * SIZE * 3);
    for _row in 0..SIZE {
        for col in 0..SIZE {
            let v = 1000.0 + 2.0 * (col as f64 + 0.5) + 32768.0;
            rgb.extend_from_slice(&[
                (v / 256.0) as u8,
                (v % 256.0) as u8,
                (v.fract() * 256.0) as u8,
            ]);
        }
    }
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, SIZE as u32, SIZE as u32);
    encoder.set_color(png::ColorType::Rgb);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&rgb).unwrap();
    writer.finish().unwrap();
    out
}

pub fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("core-app-routes-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

pub fn open(dir: &Path) -> App {
    App::open(&dir.join("app.sqlite"), Arc::new(Listener)).unwrap()
}

/// An app whose only region covers tile X/Y with the elevation ramp.
pub fn app_with_ramp(name: &str) -> App {
    open(&ramp_region(name, false))
}

/// Like [`app_with_ramp`], with a (dummy) routing file in the region, so that
/// a router given by `App::set_router_for_tests` is used there.
pub fn app_with_ramp_routing(name: &str) -> App {
    open(&ramp_region(name, true))
}

/// Like [`app_with_ramp_routing`], with routing data said to be built by
/// Valhalla `version` in the manifest.
#[allow(dead_code)]
pub fn app_with_ramp_valhalla(name: &str, version: &str) -> App {
    open(&ramp_region_with(name, true, Some(version)))
}

fn ramp_region(name: &str, routing: bool) -> PathBuf {
    ramp_region_with(name, routing, None)
}

fn ramp_region_with(name: &str, routing: bool, valhalla: Option<&str>) -> PathBuf {
    let dir = temp_dir(name);
    let region = dir.join("regions/ramp");
    std::fs::create_dir_all(&region).unwrap();
    let (north_west, south_east) = (position(0.0, 0.0), position(SIZE as f64, SIZE as f64));
    let bounds = [
        north_west.lon,
        south_east.lat,
        south_east.lon,
        north_west.lat,
    ];
    let size = write_archive(
        &region.join("dem.pmtiles"),
        vec![(Z, X, Y, ramp_png())],
        &ArchiveInfo {
            tile_type: TileType::Png,
            bounds,
        },
    )
    .unwrap();
    let mut files =
        vec![serde_json::json!({ "name": "dem.pmtiles", "size": size, "sha256": "0".repeat(64) })];
    if routing {
        std::fs::write(region.join("routing.tar"), b"dummy").unwrap();
        files.push(
            serde_json::json!({ "name": "routing.tar", "size": 5, "sha256": "0".repeat(64) }),
        );
    }
    let mut manifest = serde_json::json!({
        "format": 1, "id": "ramp", "name": "Ramp", "bbox": bounds, "version": "1", "attribution": [],
        "files": files, "size": size,
    });
    if let Some(version) = valhalla {
        manifest["routing"] = serde_json::json!({ "valhalla": version, "max_mtb_scale": 2 });
    }
    std::fs::write(region.join("manifest.json"), manifest.to_string()).unwrap();
    dir
}

/// An app with the `mini` region from the fixtures installed. Returns the
/// directory of the app data, too.
pub fn app_with_mini(name: &str) -> (App, PathBuf) {
    let dir = temp_dir(name);
    let package = dir.join("mini-package");
    std::fs::create_dir_all(&package).unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/regions/mini");
    for entry in std::fs::read_dir(fixture).unwrap() {
        let entry = entry.unwrap();
        std::fs::copy(entry.path(), package.join(entry.file_name())).unwrap();
    }
    let app = open(&dir);
    app.install_local_region(&package).unwrap();
    (app, dir)
}
