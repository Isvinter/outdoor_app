#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::io::Write;

use common::{fixtures, sha256_hex, temp_dir};
use core_maps::{Archive, MapsError, TileType, tile_id};
use flate2::{Compression, write::GzEncoder};

#[test]
fn tile_ids_follow_the_hilbert_order_of_the_spec() {
    assert_eq!(tile_id(0, 0, 0), Some(0));
    assert_eq!(tile_id(1, 0, 0), Some(1));
    assert_eq!(tile_id(1, 0, 1), Some(2));
    assert_eq!(tile_id(1, 1, 1), Some(3));
    assert_eq!(tile_id(1, 1, 0), Some(4));
    assert_eq!(tile_id(2, 0, 0), Some(5));
    assert_eq!(tile_id(12, 3423, 1763), Some(19_078_479));
}

#[test]
fn tile_ids_reject_coordinates_outside_the_zoom_level() {
    assert_eq!(tile_id(1, 2, 0), None);
    assert_eq!(tile_id(3, 0, 8), None);
    assert_eq!(tile_id(40, 0, 0), None);
}

#[test]
fn header_of_real_archive_is_read() {
    let archive = Archive::open(&fixtures().join("mini/outdoor.pmtiles")).unwrap();
    let header = archive.header();
    assert_eq!(header.tile_type, TileType::Mvt);
    assert_eq!(header.min_zoom, 0);
    assert_eq!(header.max_zoom, 13);
    let [west, south, east, north] = header.bounds;
    assert!(
        west < 11.55 && east > 11.55 && south < 47.575 && north > 47.575,
        "{:?}",
        header.bounds
    );
}

#[test]
fn vector_tile_is_returned_decompressed() {
    let archive = Archive::open(&fixtures().join("mini/outdoor.pmtiles")).unwrap();
    let tile = archive.tile(13, 4358, 2862).unwrap().unwrap();
    assert_eq!(tile.len(), 8132);
    assert_eq!(
        sha256_hex(&tile),
        "7dc549f0820e10fa9f83b1568a43ea4c4d2d633fad83917f287875e8effc8b80"
    );
}

#[test]
fn raster_tile_is_returned_unchanged() {
    let archive = Archive::open(&fixtures().join("mini/dem.pmtiles")).unwrap();
    assert_eq!(archive.header().tile_type, TileType::Webp);
    let tile = archive.tile(2, 2, 1).unwrap().unwrap();
    assert_eq!(
        sha256_hex(&tile),
        "3bc126c1d884d4f2295ba9e3240eec9692bdf5afe540d259e3d490cfe08043f8"
    );
}

#[test]
fn tile_outside_the_archive_is_none() {
    let archive = Archive::open(&fixtures().join("mini/outdoor.pmtiles")).unwrap();
    assert_eq!(archive.tile(13, 0, 0).unwrap(), None);
    assert_eq!(archive.tile(14, 8717, 5725).unwrap(), None);
}

#[test]
fn file_that_is_not_an_archive_is_rejected() {
    let dir = temp_dir("not-an-archive");
    let path = dir.join("broken.pmtiles");
    std::fs::write(&path, vec![b'x'; 300]).unwrap();
    assert!(matches!(
        Archive::open(&path),
        Err(MapsError::InvalidArchive { .. })
    ));
    std::fs::write(&path, b"PMTiles").unwrap();
    assert!(matches!(
        Archive::open(&path),
        Err(MapsError::InvalidArchive { .. })
    ));
}

#[test]
fn missing_file_is_an_io_error() {
    let path = temp_dir("missing").join("nope.pmtiles");
    assert!(matches!(Archive::open(&path), Err(MapsError::Io { .. })));
}

// --- synthetic archive with leaf directories ------------------------------

fn varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

/// Entries are `(tile_id, offset, length, run_length)`.
fn directory(entries: &[(u64, u64, u64, u64)]) -> Vec<u8> {
    let mut raw = Vec::new();
    varint(&mut raw, entries.len() as u64);
    let mut last = 0;
    for e in entries {
        varint(&mut raw, e.0 - last);
        last = e.0;
    }
    for e in entries {
        varint(&mut raw, e.3);
    }
    for e in entries {
        varint(&mut raw, e.2);
    }
    for (i, e) in entries.iter().enumerate() {
        let follows = i > 0 && e.1 == entries[i - 1].1 + entries[i - 1].2;
        varint(&mut raw, if follows { 0 } else { e.1 + 1 });
    }
    let mut gz = GzEncoder::new(Vec::new(), Compression::default());
    gz.write_all(&raw).unwrap();
    gz.finish().unwrap()
}

/// Archive with uncompressed tiles `b"tile-<id>"` for ids 0..=20, split over two
/// leaf directories; ids 5..=8 share one run-length entry and id 12 is missing.
fn synthetic_archive() -> Vec<u8> {
    let mut data = Vec::new();
    let mut first = Vec::new();
    let mut second = Vec::new();
    for id in 0u64..=20 {
        if id == 12 || (6..=8).contains(&id) {
            continue;
        }
        let content = format!("tile-{id}");
        let entry = (
            id,
            data.len() as u64,
            content.len() as u64,
            if id == 5 { 4 } else { 1 },
        );
        data.extend_from_slice(content.as_bytes());
        if id < 10 {
            first.push(entry)
        } else {
            second.push(entry)
        }
    }
    let first = directory(&first);
    let second = directory(&second);
    let root = directory(&[
        (0, 0, first.len() as u64, 0),
        (10, first.len() as u64, second.len() as u64, 0),
    ]);
    let metadata = b"{}".to_vec();

    let root_offset = 127u64;
    let metadata_offset = root_offset + root.len() as u64;
    let leaves_offset = metadata_offset + metadata.len() as u64;
    let leaves_len = (first.len() + second.len()) as u64;
    let data_offset = leaves_offset + leaves_len;

    let mut out = Vec::new();
    out.extend_from_slice(b"PMTiles");
    out.push(3);
    for value in [
        root_offset,
        root.len() as u64,
        metadata_offset,
        metadata.len() as u64,
        leaves_offset,
        leaves_len,
        data_offset,
        data.len() as u64,
        21,
        17,
        17,
    ] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.extend_from_slice(&[1, 2, 1, 1, 0, 2]); // clustered, gzip dirs, plain tiles, mvt, zoom 0-2
    for value in [-1_800_000_000i32, -850_000_000, 1_800_000_000, 850_000_000] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.push(0);
    out.extend_from_slice(&0i32.to_le_bytes());
    out.extend_from_slice(&0i32.to_le_bytes());
    assert_eq!(out.len(), 127);
    out.extend_from_slice(&root);
    out.extend_from_slice(&metadata);
    out.extend_from_slice(&first);
    out.extend_from_slice(&second);
    out.extend_from_slice(&data);
    out
}

/// Inverse of `tile_id` by brute force, good enough for zoom 0-2.
fn coordinates(id: u64) -> (u8, u32, u32) {
    for z in 0..=2u8 {
        for x in 0..(1u32 << z) {
            for y in 0..(1u32 << z) {
                if tile_id(z, x, y) == Some(id) {
                    return (z, x, y);
                }
            }
        }
    }
    panic!("no tile with id {id}");
}

#[test]
fn tiles_are_found_through_leaf_directories() {
    let dir = temp_dir("leaves");
    let path = dir.join("synthetic.pmtiles");
    std::fs::write(&path, synthetic_archive()).unwrap();
    let archive = Archive::open(&path).unwrap();

    for id in [0u64, 4, 9, 10, 11, 13, 20] {
        let (z, x, y) = coordinates(id);
        let tile = archive.tile(z, x, y).unwrap();
        assert_eq!(
            tile.as_deref(),
            Some(format!("tile-{id}").as_bytes()),
            "tile id {id}"
        );
    }
}

#[test]
fn run_length_entries_cover_several_tile_ids() {
    let dir = temp_dir("runs");
    let path = dir.join("synthetic.pmtiles");
    std::fs::write(&path, synthetic_archive()).unwrap();
    let archive = Archive::open(&path).unwrap();

    for id in 5u64..=8 {
        let (z, x, y) = coordinates(id);
        assert_eq!(
            archive.tile(z, x, y).unwrap().as_deref(),
            Some(&b"tile-5"[..]),
            "tile id {id}"
        );
    }
    let (z, x, y) = coordinates(12);
    assert_eq!(archive.tile(z, x, y).unwrap(), None);
}

#[test]
fn truncated_archive_reports_an_error_instead_of_a_tile() {
    let dir = temp_dir("truncated");
    let path = dir.join("synthetic.pmtiles");
    let mut bytes = synthetic_archive();
    bytes.truncate(bytes.len() - 20);
    std::fs::write(&path, bytes).unwrap();
    let archive = Archive::open(&path).unwrap();
    let (z, x, y) = coordinates(20);
    assert!(matches!(
        archive.tile(z, x, y),
        Err(MapsError::InvalidArchive { .. })
    ));
}
