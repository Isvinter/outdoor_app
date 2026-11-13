#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use core_maps::{Archive, ArchiveInfo, TileType, write_archive};
use slope_tiles::{
    CLASS_COLORS, Grid, Options, classify, decode_terrarium, downsample, encode_png, pixel_size_m,
    run, slope_grid,
};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("slope-tiles-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// --- pure functions -----------------------------------------------------------

#[test]
fn terrarium_pixels_decode_to_metres() {
    assert_eq!(decode_terrarium([128, 0, 0]), 0.0);
    assert_eq!(decode_terrarium([131, 232, 128]), 1000.5);
    assert_eq!(decode_terrarium([127, 255, 0]), -1.0);
}

#[test]
fn slope_classes_follow_avalanche_map_conventions() {
    assert_eq!(classify(0.0), 0);
    assert_eq!(classify(29.9), 0);
    assert_eq!(classify(30.0), 1);
    assert_eq!(classify(34.9), 1);
    assert_eq!(classify(35.0), 2);
    assert_eq!(classify(40.0), 3);
    assert_eq!(classify(45.0), 4);
    assert_eq!(classify(89.0), 4);
    assert_eq!(classify(f32::NAN), 0);
    assert_eq!(CLASS_COLORS.len(), 5);
}

#[test]
fn pixel_size_shrinks_with_latitude_and_zoom() {
    // 512 px tiles at zoom 13 on the equator: 40075016.686 m / 2^13 / 512.
    assert!((pixel_size_m(13, 512, 0.0) - 9.5546).abs() < 1e-3);
    assert!((pixel_size_m(13, 512, 60.0) - 9.5546 / 2.0).abs() < 1e-3);
    assert!((pixel_size_m(14, 512, 0.0) - 9.5546 / 2.0).abs() < 1e-3);
    assert!((pixel_size_m(13, 256, 0.0) - 9.5546 * 2.0).abs() < 1e-3);
}

/// Elevations of a `size`+2 square (one pixel border) from a function of
/// column and row, both starting at -1.
fn dem(size: usize, height: impl Fn(f64, f64) -> f64) -> Vec<f32> {
    let n = size + 2;
    (0..n * n)
        .map(|i| height((i % n) as f64 - 1.0, (i / n) as f64 - 1.0) as f32)
        .collect()
}

/// Rows of tile y at zoom z span little latitude; good enough as one pixel size.
fn row_size(z: u8, y: u32, size: usize) -> f64 {
    let n = f64::from(1u32 << z);
    let lat = (std::f64::consts::PI * (1.0 - 2.0 * (f64::from(y) + 0.5) / n))
        .sinh()
        .atan()
        .to_degrees();
    pixel_size_m(z, size, lat)
}

#[test]
fn plane_tilted_east_has_its_angle_everywhere() {
    let (z, y, size) = (13, 2862, 64);
    let rise = 37f64.to_radians().tan() * row_size(z, y, size);
    let grid = slope_grid(&dem(size, |col, _| 1000.0 + col * rise), size, z, y);
    assert_eq!(grid.size, size);
    for (i, value) in grid.values.iter().enumerate() {
        assert!((value - 37.0).abs() < 0.1, "pixel {i}: {value}");
    }
}

#[test]
fn plane_tilted_north_and_diagonally_is_measured_too() {
    let (z, y, size) = (13, 2862, 32);
    let rise = 42f64.to_radians().tan() * row_size(z, y, size);
    let north = slope_grid(&dem(size, |_, row| 500.0 - row * rise), size, z, y);
    assert!(
        north.values.iter().all(|v| (v - 42.0).abs() < 0.1),
        "{:?}",
        &north.values[..4]
    );
    // Rising equally east and south: the gradient is sqrt(2) times one component.
    let component = 30f64.to_radians().tan() / 2f64.sqrt() * row_size(z, y, size);
    let diagonal = slope_grid(&dem(size, |col, row| (col + row) * component), size, z, y);
    assert!(
        diagonal.values.iter().all(|v| (v - 30.0).abs() < 0.1),
        "{:?}",
        &diagonal.values[..4]
    );
}

#[test]
fn flat_ground_has_no_slope_and_missing_data_stays_missing() {
    let flat = slope_grid(&dem(16, |_, _| 812.0), 16, 12, 1431);
    assert!(flat.values.iter().all(|v| *v == 0.0));
    let mut holes = dem(16, |_, _| 812.0);
    holes[5 * 18 + 5] = f32::NAN;
    let grid = slope_grid(&holes, 16, 12, 1431);
    assert!(grid.values[4 * 16 + 4].is_nan(), "the pixel itself");
    assert!(
        grid.values[3 * 16 + 4].is_nan(),
        "a pixel whose window touches the hole"
    );
    assert_eq!(grid.values[10 * 16 + 10], 0.0);
}

#[test]
fn parent_tile_averages_blocks_of_its_children() {
    let child = |value: f32| Grid {
        size: 4,
        values: vec![value; 16],
    };
    let (a, b, c) = (child(10.0), child(20.0), child(40.0));
    // Order: north-west, north-east, south-west, south-east; the last one is missing.
    let parent = downsample([Some(&a), Some(&b), Some(&c), None]).unwrap();
    assert_eq!(parent.size, 4);
    assert_eq!(parent.values[0], 10.0);
    assert_eq!(parent.values[3], 20.0);
    assert_eq!(parent.values[2 * 4], 40.0);
    assert!(parent.values[3 * 4 + 3].is_nan());
    assert!(downsample([None, None, None, None]).is_none());

    let mut mixed = child(30.0);
    mixed.values[0] = 50.0;
    mixed.values[1] = f32::NAN;
    let parent = downsample([Some(&mixed), None, None, None]).unwrap();
    assert_eq!(
        parent.values[0],
        (50.0 + 30.0 + 30.0) / 3.0,
        "missing pixels do not count"
    );
}

fn decode_indexed(png_bytes: &[u8]) -> (u32, Vec<u8>, Vec<u8>, Vec<u8>) {
    let mut reader = png::Decoder::new(std::io::Cursor::new(png_bytes))
        .read_info()
        .unwrap();
    let info = reader.info();
    assert_eq!(info.color_type, png::ColorType::Indexed);
    let palette = info.palette.clone().unwrap().into_owned();
    let alpha = info.trns.clone().unwrap().into_owned();
    let width = info.width;
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let frame = reader.next_frame(&mut pixels).unwrap();
    pixels.truncate(frame.buffer_size());
    (width, pixels, palette, alpha)
}

#[test]
fn tiles_are_encoded_as_palette_images_and_empty_ones_skipped() {
    let mut grid = Grid {
        size: 8,
        values: vec![10.0; 64],
    };
    assert!(
        encode_png(&grid).unwrap().is_none(),
        "nothing steeper than 30 degrees"
    );
    grid.values[9] = 32.0;
    grid.values[10] = 47.0;
    let (width, pixels, palette, alpha) = decode_indexed(&encode_png(&grid).unwrap().unwrap());
    assert_eq!(width, 8);
    assert_eq!(pixels.len(), 64);
    assert_eq!((pixels[0], pixels[9], pixels[10]), (0, 1, 4));
    assert_eq!(alpha[0], 0, "class 0 is transparent");
    assert_eq!(&palette[3..6], &CLASS_COLORS[1][..]);
}

// --- whole run ----------------------------------------------------------------

fn terrarium_png(size: usize, height: impl Fn(usize, usize) -> f64) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(size * size * 3);
    for row in 0..size {
        for col in 0..size {
            let v = height(col, row) + 32768.0;
            rgb.extend_from_slice(&[
                (v / 256.0) as u8,
                (v % 256.0) as u8,
                (v.fract() * 256.0) as u8,
            ]);
        }
    }
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, size as u32, size as u32);
    encoder.set_color(png::ColorType::Rgb);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&rgb).unwrap();
    writer.finish().unwrap();
    out
}

const SIZE: usize = 64;
const X0: u32 = 4357;
const Y0: u32 = 2861;

/// 3×3 elevation tiles at zoom 13 holding one plane that rises eastwards at 37°.
fn plane_archive(dir: &std::path::Path) -> PathBuf {
    let rise = 37f64.to_radians().tan() * row_size(13, Y0 + 1, SIZE);
    let mut tiles = Vec::new();
    for dx in 0..3u32 {
        for dy in 0..3u32 {
            let offset = (dx as usize * SIZE) as f64;
            let image = terrarium_png(SIZE, |col, _| 600.0 + (offset + col as f64) * rise);
            tiles.push((13u8, X0 + dx, Y0 + dy, image));
        }
    }
    let path = dir.join("dem.pmtiles");
    let info = ArchiveInfo {
        tile_type: TileType::Png,
        bounds: [11.47, 47.52, 11.60, 47.61],
    };
    write_archive(&path, tiles, &info).unwrap();
    path
}

#[test]
fn slope_archive_is_computed_across_tile_borders() {
    let dir = temp_dir("plane");
    let dem = plane_archive(&dir);
    let out = dir.join("slope.pmtiles");

    let summary = run(
        &dem,
        &out,
        &Options {
            zoom: 13,
            min_zoom: 12,
            threads: Some(2),
        },
    )
    .unwrap();

    assert_eq!(summary.zoom, 13);
    assert_eq!(summary.tiles_computed, 9);
    let archive = Archive::open(&out).unwrap();
    assert_eq!(archive.header().tile_type, TileType::Png);
    assert_eq!(
        (archive.header().min_zoom, archive.header().max_zoom),
        (12, 13)
    );
    assert_eq!(archive.header().bounds, [11.47, 47.52, 11.60, 47.61]);

    // The middle tile has neighbours on all sides: 37° (class 2) up to its edges.
    let middle = archive.tile(13, X0 + 1, Y0 + 1).unwrap().unwrap();
    let (width, pixels, _, _) = decode_indexed(&middle);
    assert_eq!(width as usize, SIZE);
    assert!(pixels.iter().all(|p| *p == 2), "{:?}", &pixels[..SIZE]);

    // At the edge of the data the outermost column has no eastern neighbour.
    let east = archive.tile(13, X0 + 2, Y0 + 1).unwrap().unwrap();
    let (_, pixels, _, _) = decode_indexed(&east);
    assert_eq!(pixels[SIZE / 2], 2);
    assert_eq!(pixels[SIZE - 1], 0);

    // Zoom 12 is the average of its children and shows the same class.
    let middle = (X0 + 1, Y0 + 1);
    let parent = archive
        .tile(12, middle.0 / 2, middle.1 / 2)
        .unwrap()
        .unwrap();
    let (width, pixels, _, _) = decode_indexed(&parent);
    assert_eq!(width as usize, SIZE);
    assert!(pixels.iter().filter(|p| **p == 2).count() > SIZE * SIZE / 8);
    assert!(archive.tile(13, X0 + 5, Y0).unwrap().is_none());
}

#[test]
fn flat_terrain_gives_an_archive_without_tiles() {
    let dir = temp_dir("flat");
    let path = dir.join("dem.pmtiles");
    let info = ArchiveInfo {
        tile_type: TileType::Png,
        bounds: [11.5, 47.5, 11.6, 47.6],
    };
    write_archive(
        &path,
        vec![(13, X0, Y0, terrarium_png(SIZE, |_, _| 700.0))],
        &info,
    )
    .unwrap();
    let out = dir.join("slope.pmtiles");

    let summary = run(
        &path,
        &out,
        &Options {
            zoom: 13,
            min_zoom: 12,
            threads: None,
        },
    )
    .unwrap();

    assert_eq!(summary.tiles_written, 0);
    assert!(
        Archive::open(&out)
            .unwrap()
            .tile(13, X0, Y0)
            .unwrap()
            .is_none()
    );
}

#[test]
fn compute_zoom_is_limited_by_the_elevation_data() {
    let dir = temp_dir("zoom");
    let dem = plane_archive(&dir);
    let summary = run(
        &dem,
        &dir.join("slope.pmtiles"),
        &Options {
            zoom: 15,
            min_zoom: 13,
            threads: None,
        },
    )
    .unwrap();
    assert_eq!(summary.zoom, 13);
}

#[test]
fn real_elevation_tiles_are_understood() {
    // The fixture only reaches zoom 2; this checks WebP decoding end to end.
    let dem =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/regions/mini/dem.pmtiles");
    let dir = temp_dir("real");
    let summary = run(
        &dem,
        &dir.join("slope.pmtiles"),
        &Options {
            zoom: 2,
            min_zoom: 2,
            threads: None,
        },
    )
    .unwrap();
    assert_eq!(summary.zoom, 2);
    assert!(summary.tiles_computed >= 1);
}

#[test]
fn many_tiles_survive_a_round_trip_through_leaf_directories() {
    let dir = temp_dir("many");
    let path = dir.join("many.pmtiles");
    let mut tiles = Vec::new();
    for x in 0..120u32 {
        for y in 0..120u32 {
            // Distinct contents so that nothing is merged into runs.
            tiles.push((14u8, x, y, format!("tile {x}/{y}").into_bytes()));
        }
    }
    let info = ArchiveInfo {
        tile_type: TileType::Png,
        bounds: [-180.0, 80.0, -170.0, 85.0],
    };
    write_archive(&path, tiles, &info).unwrap();
    let archive = Archive::open(&path).unwrap();
    for (x, y) in [(0, 0), (119, 119), (57, 3), (3, 57), (64, 64)] {
        assert_eq!(
            archive.tile(14, x, y).unwrap().unwrap(),
            format!("tile {x}/{y}").into_bytes()
        );
    }
    assert!(archive.tile(14, 120, 0).unwrap().is_none());
}
