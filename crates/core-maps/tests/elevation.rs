#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::f64::consts::PI;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use common::{fixtures, temp_dir};
use core_maps::{ArchiveInfo, DefaultFetcher, RegionStore, TileType, write_archive};

const SIZE: usize = 64;
const Z: u8 = 12;
const X: u32 = 2179;
const Y: u32 = 1431;

/// Elevation of the synthetic terrain at a pixel position inside tile X/Y.
fn terrain(col: f64, row: f64) -> f64 {
    1000.0 + 2.0 * col + row
}

/// Longitude and latitude of a pixel position (0..SIZE) inside tile X/Y.
fn position(col: f64, row: f64) -> (f64, f64) {
    let n = f64::from(1u32 << Z);
    let lon = (f64::from(X) + col / SIZE as f64) / n * 360.0 - 180.0;
    let lat = (PI * (1.0 - 2.0 * (f64::from(Y) + row / SIZE as f64) / n))
        .sinh()
        .atan()
        .to_degrees();
    (lat, lon)
}

fn terrarium_png(height: impl Fn(usize, usize) -> f64) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(SIZE * SIZE * 3);
    for row in 0..SIZE {
        for col in 0..SIZE {
            let v = height(col, row) + 32768.0;
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

/// A store with one region whose elevation tile X/Y holds [`terrain`]
/// (values at pixel centres) and a coarse zoom 5 tile of 500 m everywhere.
fn synthetic_store(name: &str) -> RegionStore {
    let dir = temp_dir(name);
    let region = dir.join("synth");
    std::fs::create_dir_all(&region).unwrap();
    let (north, west) = position(0.0, 0.0);
    let (south, east) = position(SIZE as f64, SIZE as f64);
    let bounds = [west, south, east, north];
    let tiles = vec![
        (
            Z,
            X,
            Y,
            terrarium_png(|col, row| terrain(col as f64 + 0.5, row as f64 + 0.5)),
        ),
        (5, X >> 7, Y >> 7, terrarium_png(|_, _| 500.0)),
    ];
    let size = write_archive(
        &region.join("dem.pmtiles"),
        tiles,
        &ArchiveInfo {
            tile_type: TileType::Png,
            bounds,
        },
    )
    .unwrap();
    let manifest = serde_json::json!({
        "format": 1, "id": "synth", "name": "Synthetic", "bbox": bounds, "version": "1", "attribution": [],
        "files": [{ "name": "dem.pmtiles", "size": size, "sha256": "0".repeat(64) }], "size": size,
    });
    std::fs::write(region.join("manifest.json"), manifest.to_string()).unwrap();
    RegionStore::open(&dir).unwrap()
}

#[test]
fn elevation_at_pixel_centres_is_the_stored_value() {
    let store = synthetic_store("centres");
    for (col, row) in [(10.5, 20.5), (0.5, 0.5), (63.5, 63.5), (40.5, 3.5)] {
        let (lat, lon) = position(col, row);
        let ele = store.elevation(lat, lon).unwrap().unwrap();
        assert!(
            (f64::from(ele) - terrain(col, row)).abs() < 0.02,
            "{col}/{row}: {ele}"
        );
    }
}

#[test]
fn elevation_between_pixels_is_interpolated() {
    let store = synthetic_store("between");
    for (col, row) in [(11.0, 20.5), (10.5, 21.0), (30.25, 30.75), (12.9, 50.1)] {
        let (lat, lon) = position(col, row);
        let ele = store.elevation(lat, lon).unwrap().unwrap();
        assert!(
            (f64::from(ele) - terrain(col, row)).abs() < 0.05,
            "{col}/{row}: {ele}"
        );
    }
}

#[test]
fn elevation_outside_the_installed_regions_is_unknown() {
    let store = synthetic_store("outside");
    assert_eq!(store.elevation(48.5, 12.5).unwrap(), None);
    assert_eq!(store.elevation(-33.0, 151.0).unwrap(), None);
    assert_eq!(store.elevation(f64::NAN, 11.5).unwrap(), None);
    let empty = RegionStore::open(&temp_dir("no-regions")).unwrap();
    assert_eq!(empty.elevation(47.6, 11.55).unwrap(), None);
}

#[test]
fn profile_samples_a_line_at_regular_distances() {
    let store = synthetic_store("profile");
    // Along one pixel row from column 5 to column 55: elevation rises by 2 m per pixel.
    let (from, to) = (position(5.0, 30.5), position(55.0, 30.5));
    let length = core_track::geo::haversine_m(from.0, from.1, to.0, to.1);

    let profile = store.elevation_profile(&[from, to], 25.0).unwrap();

    assert_eq!(profile.len(), (length / 25.0).ceil() as usize + 1);
    assert_eq!(profile[0].distance_m, 0.0);
    assert!((profile[profile.len() - 1].distance_m - length).abs() < 0.01);
    assert!(
        profile
            .windows(2)
            .all(|w| w[1].distance_m > w[0].distance_m)
    );
    assert!(
        profile
            .windows(2)
            .all(|w| w[1].distance_m - w[0].distance_m <= 25.01)
    );
    let (first, last) = (
        profile[0].ele.unwrap(),
        profile[profile.len() - 1].ele.unwrap(),
    );
    assert!(
        (f64::from(first) - terrain(5.0, 30.5)).abs() < 0.1,
        "{first}"
    );
    assert!(
        (f64::from(last) - terrain(55.0, 30.5)).abs() < 0.1,
        "{last}"
    );
    assert!(
        profile
            .windows(2)
            .all(|w| w[1].ele.unwrap() >= w[0].ele.unwrap())
    );
    assert!((profile[1].lat - from.0).abs() < 1e-3 && profile[1].lon > from.1);
}

#[test]
fn profile_keeps_the_corners_of_a_line_and_marks_unknown_ground() {
    let store = synthetic_store("corners");
    let (a, b, c) = (
        position(10.0, 10.0),
        position(20.0, 10.0),
        position(20.0, 20.0),
    );
    let profile = store.elevation_profile(&[a, b, c], 5000.0).unwrap();
    assert_eq!(
        profile.len(),
        3,
        "steps longer than the stretches leave the corners"
    );
    assert!((profile[1].lat - b.0).abs() < 1e-9 && (profile[1].lon - b.1).abs() < 1e-9);

    let outside = store
        .elevation_profile(&[(48.5, 12.5), (48.5, 12.51)], 100.0)
        .unwrap();
    assert!(outside.len() >= 2 && outside.iter().all(|point| point.ele.is_none()));
    assert!(store.elevation_profile(&[], 25.0).unwrap().is_empty());
    assert_eq!(store.elevation_profile(&[a], 25.0).unwrap().len(), 1);
}

#[test]
fn real_elevation_tiles_are_read() {
    let store = RegionStore::open(&temp_dir("real-dem")).unwrap();
    let source = fixtures().to_string_lossy().into_owned();
    store
        .install(
            &source,
            "mini",
            &DefaultFetcher::new(),
            &mut |_| {},
            &AtomicBool::new(false),
        )
        .unwrap();
    // The fixture only reaches zoom 2 (about 20 km per pixel): a rough Alpine average.
    let ele = store.elevation(47.575, 11.55).unwrap().unwrap();
    assert!((200.0..2500.0).contains(&ele), "{ele}");
}

#[test]
fn damaged_elevation_tile_is_an_error() {
    let dir = temp_dir("damaged-dem");
    let region = dir.join("bad");
    std::fs::create_dir_all(&region).unwrap();
    let bounds = [11.5, 47.5, 11.7, 47.7];
    let tiles = vec![(Z, X, Y, b"not an image".to_vec())];
    write_archive(
        Path::new(&region.join("dem.pmtiles")),
        tiles,
        &ArchiveInfo {
            tile_type: TileType::Png,
            bounds,
        },
    )
    .unwrap();
    let manifest = serde_json::json!({
        "format": 1, "id": "bad", "name": "Bad", "bbox": bounds, "version": "1", "attribution": [],
        "files": [{ "name": "dem.pmtiles", "size": 1, "sha256": "0".repeat(64) }], "size": 1,
    });
    std::fs::write(region.join("manifest.json"), manifest.to_string()).unwrap();
    let store = RegionStore::open(&dir).unwrap();
    let (lat, lon) = position(32.0, 32.0);
    assert!(store.elevation(lat, lon).is_err());
}
