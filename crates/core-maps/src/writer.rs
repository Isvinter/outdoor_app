//! Writes PMTiles version 3 archives.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use crate::MapsError;
use crate::pmtiles::{TileType, tile_id};
use flate2::{Compression, write::GzEncoder};

const HEADER_LEN: usize = 127;
/// Header and root directory must fit into the first 16 KiB of the file.
const MAX_ROOT_LEN: usize = 16_384 - HEADER_LEN;

/// What an archive says about itself.
#[derive(Debug, Clone, PartialEq)]
pub struct ArchiveInfo {
    pub tile_type: TileType,
    /// West, south, east, north in degrees.
    pub bounds: [f64; 4],
}

struct Entry {
    tile_id: u64,
    offset: u64,
    length: u64,
    run_length: u64,
}

fn varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push((value & 0x7f) as u8 | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn gzip(raw: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(raw)?;
    encoder.finish()
}

fn directory(entries: &[Entry]) -> std::io::Result<Vec<u8>> {
    let mut raw = Vec::with_capacity(entries.len() * 6);
    varint(&mut raw, entries.len() as u64);
    let mut last = 0;
    for entry in entries {
        varint(&mut raw, entry.tile_id - last);
        last = entry.tile_id;
    }
    for entry in entries {
        varint(&mut raw, entry.run_length);
    }
    for entry in entries {
        varint(&mut raw, entry.length);
    }
    let mut next = None;
    for entry in entries {
        // 0 means "directly after the previous entry".
        varint(
            &mut raw,
            if next == Some(entry.offset) {
                0
            } else {
                entry.offset + 1
            },
        );
        next = Some(entry.offset + entry.length);
    }
    gzip(&raw)
}

/// Root directory and leaf directory section for the given tile entries.
fn directories(entries: &[Entry]) -> std::io::Result<(Vec<u8>, Vec<u8>)> {
    let root = directory(entries)?;
    if root.len() <= MAX_ROOT_LEN {
        return Ok((root, Vec::new()));
    }
    let mut leaf_size = 4096;
    loop {
        let mut leaves = Vec::new();
        let mut pointers = Vec::new();
        for chunk in entries.chunks(leaf_size) {
            let leaf = directory(chunk)?;
            pointers.push(Entry {
                tile_id: chunk.first().map_or(0, |entry| entry.tile_id),
                offset: leaves.len() as u64,
                length: leaf.len() as u64,
                run_length: 0,
            });
            leaves.extend_from_slice(&leaf);
        }
        let root = directory(&pointers)?;
        if root.len() <= MAX_ROOT_LEN {
            return Ok((root, leaves));
        }
        leaf_size *= 2;
    }
}

fn e7(degrees: f64) -> [u8; 4] {
    ((degrees * 1e7).round() as i32).to_le_bytes()
}

/// Writes the tiles `(z, x, y, content)` to a new archive at `path` and returns
/// its size in bytes. Identical contents are stored once; tiles with
/// impossible coordinates are left out.
pub fn write_archive(
    path: &Path,
    tiles: Vec<(u8, u32, u32, Vec<u8>)>,
    info: &ArchiveInfo,
) -> Result<u64, MapsError> {
    let min_zoom = tiles.iter().map(|tile| tile.0).min().unwrap_or(0);
    let max_zoom = tiles.iter().map(|tile| tile.0).max().unwrap_or(0);
    let mut tiles: Vec<(u64, Vec<u8>)> = tiles
        .into_iter()
        .filter_map(|(z, x, y, data)| Some((tile_id(z, x, y)?, data)))
        .collect();
    tiles.sort_by_key(|tile| tile.0);
    tiles.dedup_by_key(|tile| tile.0);

    let mut entries: Vec<Entry> = Vec::with_capacity(tiles.len());
    let mut stored: HashMap<&[u8], u64> = HashMap::new();
    let mut contents: Vec<&[u8]> = Vec::new();
    let mut data_len = 0u64;
    for (id, data) in &tiles {
        let offset = *stored.entry(data.as_slice()).or_insert_with(|| {
            contents.push(data.as_slice());
            let offset = data_len;
            data_len += data.len() as u64;
            offset
        });
        match entries.last_mut() {
            Some(last) if last.offset == offset && last.tile_id + last.run_length == *id => {
                last.run_length += 1
            }
            _ => entries.push(Entry {
                tile_id: *id,
                offset,
                length: data.len() as u64,
                run_length: 1,
            }),
        }
    }

    let io = |source| MapsError::Io {
        path: path.to_path_buf(),
        source,
    };
    let (root, leaves) = directories(&entries).map_err(io)?;
    let metadata = gzip(b"{}").map_err(io)?;
    let root_offset = HEADER_LEN as u64;
    let metadata_offset = root_offset + root.len() as u64;
    let leaves_offset = metadata_offset + metadata.len() as u64;
    let data_offset = leaves_offset + leaves.len() as u64;

    let mut header = Vec::with_capacity(HEADER_LEN);
    header.extend_from_slice(b"PMTiles");
    header.push(3);
    for value in [
        root_offset,
        root.len() as u64,
        metadata_offset,
        metadata.len() as u64,
        leaves_offset,
        leaves.len() as u64,
        data_offset,
        data_len,
        tiles.len() as u64,
        entries.len() as u64,
        contents.len() as u64,
    ] {
        header.extend_from_slice(&value.to_le_bytes());
    }
    let tile_type = match info.tile_type {
        TileType::Unknown => 0,
        TileType::Mvt => 1,
        TileType::Png => 2,
        TileType::Jpeg => 3,
        TileType::Webp => 4,
        TileType::Avif => 5,
    };
    // Clustered, gzip directories, tiles stored as they are.
    header.extend_from_slice(&[1, 2, 1, tile_type, min_zoom, max_zoom]);
    let [west, south, east, north] = info.bounds;
    for degrees in [west, south, east, north] {
        header.extend_from_slice(&e7(degrees));
    }
    header.push(min_zoom);
    header.extend_from_slice(&e7((west + east) / 2.0));
    header.extend_from_slice(&e7((south + north) / 2.0));

    let mut out = BufWriter::new(File::create(path).map_err(io)?);
    for part in [header.as_slice(), &root, &metadata, &leaves] {
        out.write_all(part).map_err(io)?;
    }
    for content in contents {
        out.write_all(content).map_err(io)?;
    }
    out.flush().map_err(io)?;
    Ok(data_offset + data_len)
}
