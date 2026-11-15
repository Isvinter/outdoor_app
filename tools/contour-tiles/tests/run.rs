//! Building a contour archive from an elevation archive.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::{Path, PathBuf};

use contour_tiles::{EXTENT, Options, run};
use core_maps::{Archive, ArchiveInfo, TileType, write_archive};

/// Centre of the synthetic mountain, and the zoom of its elevation tiles.
const PEAK: (f64, f64) = (47.6, 11.55);
const DEM_ZOOM: u8 = 13;
/// Tiles of 256 pixels.
const SIZE: u32 = 256;

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("contour-tiles-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Fractional tile column and row of a position at zoom `z`.
fn tile_xy(lat: f64, lon: f64, z: u8) -> (f64, f64) {
    let n = f64::from(1u32 << z);
    let lat = lat.to_radians();
    (
        (lon + 180.0) / 360.0 * n,
        (1.0 - lat.tan().asinh() / std::f64::consts::PI) / 2.0 * n,
    )
}

/// A cone: 2 000 m at the peak, falling to 500 m at a distance of 2.5 tiles at
/// the elevation zoom, flat beyond.
fn height(x: f64, y: f64, z: u8) -> f64 {
    let (px, py) = tile_xy(PEAK.0, PEAK.1, z);
    let scale = f64::from(1u32 << DEM_ZOOM) / f64::from(1u32 << z);
    let distance = ((x - px).powi(2) + (y - py).powi(2)).sqrt() * scale;
    500.0 + 1500.0 * (1.0 - distance / 2.5).max(0.0)
}

fn terrarium_png(z: u8, x: u32, y: u32) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((SIZE * SIZE * 3) as usize);
    for row in 0..SIZE {
        for col in 0..SIZE {
            let (fx, fy) = (
                f64::from(x) + (f64::from(col) + 0.5) / f64::from(SIZE),
                f64::from(y) + (f64::from(row) + 0.5) / f64::from(SIZE),
            );
            let value = height(fx, fy, z) + 32768.0;
            let (r, g) = ((value / 256.0).floor(), value.floor() % 256.0);
            let b = ((value - value.floor()) * 256.0).floor();
            pixels.extend_from_slice(&[r as u8, g as u8, b as u8]);
        }
    }
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, SIZE, SIZE);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&pixels)
        .unwrap();
    out
}

/// An elevation archive with the 5 × 5 tiles around the peak at zoom 13 and
/// the tiles above them at zooms 11 and 12.
fn dem_archive(dir: &Path) -> (PathBuf, [f64; 4]) {
    let (cx, cy) = tile_xy(PEAK.0, PEAK.1, DEM_ZOOM);
    let (cx, cy) = (cx.floor() as u32, cy.floor() as u32);
    let mut tiles = Vec::new();
    for z in 11..=DEM_ZOOM {
        let shift = DEM_ZOOM - z;
        let (x0, x1, y0, y1) = (
            (cx - 2) >> shift,
            (cx + 2) >> shift,
            (cy - 2) >> shift,
            (cy + 2) >> shift,
        );
        for x in x0..=x1 {
            for y in y0..=y1 {
                tiles.push((z, x, y, terrarium_png(z, x, y)));
            }
        }
    }
    let lon = |x: u32| f64::from(x) / f64::from(1u32 << DEM_ZOOM) * 360.0 - 180.0;
    let lat = |y: u32| {
        let n = std::f64::consts::PI * (1.0 - 2.0 * f64::from(y) / f64::from(1u32 << DEM_ZOOM));
        n.sinh().atan().to_degrees()
    };
    let bounds = [lon(cx - 2), lat(cy + 3), lon(cx + 3), lat(cy - 2)];
    let path = dir.join("dem.pmtiles");
    write_archive(
        &path,
        tiles,
        &ArchiveInfo {
            tile_type: TileType::Png,
            bounds,
        },
    )
    .unwrap();
    (path, bounds)
}

fn options() -> Options {
    Options {
        min_zoom: 11,
        max_zoom: 15,
        threads: Some(2),
        bounds: None,
    }
}

/// The contour archive of the synthetic mountain, built once for all tests.
fn build() -> &'static (Archive, [f64; 4]) {
    static BUILT: std::sync::OnceLock<(Archive, [f64; 4])> = std::sync::OnceLock::new();
    BUILT.get_or_init(|| {
        let dir = temp("shared");
        let (dem, bounds) = dem_archive(&dir);
        let output = dir.join("contours.pmtiles");
        let summary = run(&dem, &output, &options()).unwrap();
        assert!(summary.tiles_written > 0);
        (Archive::open(&output).unwrap(), bounds)
    })
}

/// Every tile of the archive at zoom `z` within `bounds`, decoded.
fn tiles_at(archive: &Archive, bounds: &[f64; 4], z: u8) -> Vec<((u32, u32), common::Layer)> {
    let (x0, y0) = tile_xy(bounds[3], bounds[0], z);
    let (x1, y1) = tile_xy(bounds[1], bounds[2], z);
    let mut out = Vec::new();
    for x in x0.floor() as u32..=x1.floor() as u32 {
        for y in y0.floor() as u32..=y1.floor() as u32 {
            if let Some(data) = archive.tile(z, x, y).unwrap() {
                let mut layers = common::decode(&data);
                assert_eq!(layers.len(), 1);
                out.push(((x, y), layers.remove(0)));
            }
        }
    }
    out
}

#[test]
fn writes_zooms_11_to_15() {
    let (archive, bounds) = build();
    let header = archive.header();
    assert_eq!(header.tile_type, TileType::Mvt);
    assert_eq!((header.min_zoom, header.max_zoom), (11, 15));
    for (got, want) in header.bounds.iter().zip(bounds) {
        assert!(
            (got - want).abs() < 1e-6,
            "{:?} vs {bounds:?}",
            header.bounds
        );
    }
}

#[test]
fn values_follow_intervals() {
    let (archive, bounds) = build();
    for (z, minor, major) in [(13, 20, 100), (15, 10, 50)] {
        let tiles = tiles_at(archive, bounds, z);
        assert!(!tiles.is_empty(), "no tiles at {z}");
        for (_, layer) in &tiles {
            assert_eq!(
                (layer.name.as_str(), layer.extent),
                ("contours", u64::from(EXTENT))
            );
            for feature in &layer.features {
                let (ele, level) = (feature.int("ele"), feature.int("level"));
                assert_eq!(ele % minor, 0, "z{z}: {ele}");
                assert_eq!(level == 1, ele % major == 0, "z{z}: {ele} level {level}");
            }
        }
    }
}

/// Where the segments of a line cross the vertical `x`, as (ele, y).
fn crossings(layer: &common::Layer, x: f64) -> Vec<(i64, f64)> {
    let mut out = Vec::new();
    for feature in &layer.features {
        for line in &feature.lines {
            for pair in line.windows(2) {
                let ([ax, ay], [bx, by]) = (pair[0].map(f64::from), pair[1].map(f64::from));
                if (ax - x) * (bx - x) < 0.0 || (ax == x && bx != x) {
                    let t = (x - ax) / (bx - ax);
                    out.push((feature.int("ele"), ay + t * (by - ay)));
                }
            }
        }
    }
    out
}

/// Distance from `p` to the nearest segment of the lines at `ele` (both
/// tiles simplify by up to 2 units, so a line may shift by that much).
fn distance_to_lines(layer: &common::Layer, ele: i64, p: [f64; 2]) -> f64 {
    let mut best = f64::INFINITY;
    for feature in layer.features.iter().filter(|f| f.int("ele") == ele) {
        for line in &feature.lines {
            for pair in line.windows(2) {
                let (a, b) = (pair[0].map(f64::from), pair[1].map(f64::from));
                let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                let t = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / (dx * dx + dy * dy))
                    .clamp(0.0, 1.0);
                best = best.min((p[0] - a[0] - t * dx).hypot(p[1] - a[1] - t * dy));
            }
        }
    }
    best
}

#[test]
fn lines_continue_across_tile_edges() {
    let (archive, bounds) = build();
    let tiles = tiles_at(archive, bounds, 14);
    let mut checked = 0;
    for ((x, y), left) in &tiles {
        let Some((_, right)) = tiles.iter().find(|((rx, ry), _)| *rx == x + 1 && ry == y) else {
            continue;
        };
        for (ele, y_left) in crossings(left, f64::from(EXTENT)) {
            // The point where the line leaves, in the coordinates of the right tile.
            let gap = distance_to_lines(right, ele, [0.0, y_left]);
            assert!(
                gap <= 4.0,
                "line {ele} leaves tile {x}/{y} at y {y_left}; the next tile's line is {gap} away"
            );
            checked += 1;
        }
    }
    assert!(checked > 10, "only {checked} crossings checked");
}

#[test]
fn edge_of_region_has_no_invented_lines() {
    let (archive, bounds) = build();
    let border = 8.0 * 16.0;
    for z in 11..=15 {
        for (_, layer) in tiles_at(archive, bounds, z) {
            for feature in &layer.features {
                let ele = feature.int("ele");
                assert!((500..=2000).contains(&ele), "z{z}: invented height {ele}");
                for point in feature.lines.iter().flatten() {
                    for c in point.map(f64::from) {
                        assert!(
                            (-border..=f64::from(EXTENT) + border).contains(&c),
                            "z{z}: {point:?} outside the buffer"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn empty_tiles_are_not_written() {
    let (archive, bounds) = build();
    for z in 11..=15 {
        for (tile, layer) in tiles_at(archive, bounds, z) {
            assert!(!layer.features.is_empty(), "z{z} {tile:?} is empty");
        }
    }
}

#[test]
fn bounds_option_limits_the_output() {
    let dir = temp("bounds");
    let (dem, _) = dem_archive(&dir);
    let output = dir.join("contours.pmtiles");
    // A small box on the flank of the mountain.
    let small = [
        PEAK.1 + 0.002,
        PEAK.0 - 0.002,
        PEAK.1 + 0.006,
        PEAK.0 + 0.002,
    ];
    let summary = run(
        &dem,
        &output,
        &Options {
            bounds: Some(small),
            ..options()
        },
    )
    .unwrap();
    let archive = Archive::open(&output).unwrap();
    for (got, want) in archive.header().bounds.iter().zip(small) {
        assert!((got - want).abs() < 1e-6);
    }
    let full = run(&dem, &dir.join("full.pmtiles"), &options()).unwrap();
    assert!(summary.tiles_written > 0 && summary.tiles_written < full.tiles_written);
}
