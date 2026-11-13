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
    let manifest = serde_json::json!({
        "format": 1, "id": "ramp", "name": "Ramp", "bbox": bounds, "version": "1", "attribution": [],
        "files": [{ "name": "dem.pmtiles", "size": size, "sha256": "0".repeat(64) }], "size": size,
    });
    std::fs::write(region.join("manifest.json"), manifest.to_string()).unwrap();
    open(&dir)
}
