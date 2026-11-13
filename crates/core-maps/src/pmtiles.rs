//! Reader for PMTiles version 3 archives (<https://github.com/protomaps/PMTiles>).
//!
//! Synchronous and self-contained so that it runs unchanged on desktop and
//! mobile. Directories are cached, tile contents are read on demand.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use flate2::read::GzDecoder;

use crate::MapsError;

const HEADER_LEN: usize = 127;
/// Directories nest at most this deep (the specification allows three leaf levels).
const MAX_DEPTH: usize = 4;
/// Upper bound for a single tile or directory, to keep damaged archives from
/// triggering huge allocations.
const MAX_BLOB_LEN: u64 = 64 << 20;
/// Leaf directories kept in memory per archive.
const LEAF_CACHE_SIZE: usize = 64;

/// Content type of the tiles in an archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileType {
    Unknown,
    Mvt,
    Png,
    Jpeg,
    Webp,
    Avif,
}

impl TileType {
    fn from_byte(byte: u8) -> Self {
        match byte {
            1 => TileType::Mvt,
            2 => TileType::Png,
            3 => TileType::Jpeg,
            4 => TileType::Webp,
            5 => TileType::Avif,
            _ => TileType::Unknown,
        }
    }

    /// MIME type to serve tiles of this type with.
    pub fn mime(self) -> &'static str {
        match self {
            TileType::Mvt => "application/x-protobuf",
            TileType::Png => "image/png",
            TileType::Jpeg => "image/jpeg",
            TileType::Webp => "image/webp",
            TileType::Avif => "image/avif",
            TileType::Unknown => "application/octet-stream",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Compression {
    None,
    Gzip,
}

/// The fixed-size header of an archive.
#[derive(Debug, Clone, PartialEq)]
pub struct Header {
    pub tile_type: TileType,
    pub min_zoom: u8,
    pub max_zoom: u8,
    /// West, south, east, north in degrees.
    pub bounds: [f64; 4],
    root_offset: u64,
    root_length: u64,
    leaves_offset: u64,
    tiles_offset: u64,
    internal_compression: Compression,
    tile_compression: Compression,
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    tile_id: u64,
    offset: u64,
    length: u64,
    /// Number of consecutive tile ids sharing this content; 0 marks a leaf directory.
    run_length: u64,
}

type Directory = Arc<Vec<Entry>>;

/// An opened PMTiles archive.
#[derive(Debug)]
pub struct Archive {
    path: PathBuf,
    header: Header,
    root: Directory,
    file: Mutex<File>,
    leaves: Mutex<HashMap<u64, Directory>>,
}

impl Archive {
    pub fn open(path: &Path) -> Result<Self, MapsError> {
        let mut file = File::open(path).map_err(MapsError::io(path))?;
        let mut raw = [0u8; HEADER_LEN];
        file.read_exact(&mut raw).map_err(|e| match e.kind() {
            std::io::ErrorKind::UnexpectedEof => invalid(path, "file is shorter than the header"),
            _ => MapsError::io(path)(e),
        })?;
        let header = parse_header(&raw).map_err(|reason| invalid(path, reason))?;
        let mut archive = Archive {
            path: path.to_path_buf(),
            header,
            root: Arc::default(),
            file: Mutex::new(file),
            leaves: Mutex::default(),
        };
        archive.root = Arc::new(
            archive.read_directory(archive.header.root_offset, archive.header.root_length)?,
        );
        Ok(archive)
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    /// Returns the content of a tile, decompressed, or `None` if the archive
    /// does not contain it.
    pub fn tile(&self, z: u8, x: u32, y: u32) -> Result<Option<Vec<u8>>, MapsError> {
        if z < self.header.min_zoom || z > self.header.max_zoom {
            return Ok(None);
        }
        let Some(id) = tile_id(z, x, y) else {
            return Ok(None);
        };
        let mut directory = Arc::clone(&self.root);
        for _ in 0..MAX_DEPTH {
            let Some(entry) = find(&directory, id) else {
                return Ok(None);
            };
            if entry.run_length == 0 {
                directory = self.leaf(entry.offset, entry.length)?;
                continue;
            }
            if id - entry.tile_id >= entry.run_length {
                return Ok(None);
            }
            let offset = self.offset(self.header.tiles_offset, entry.offset)?;
            let raw = self.read(offset, entry.length)?;
            return match self.header.tile_compression {
                Compression::None => Ok(Some(raw)),
                Compression::Gzip => self.gunzip(&raw).map(Some),
            };
        }
        Err(invalid(&self.path, "directories are nested too deeply"))
    }

    fn leaf(&self, offset: u64, length: u64) -> Result<Directory, MapsError> {
        if let Some(leaf) = self
            .leaves
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&offset)
        {
            return Ok(Arc::clone(leaf));
        }
        let absolute = self.offset(self.header.leaves_offset, offset)?;
        let leaf = Arc::new(self.read_directory(absolute, length)?);
        let mut cache = self.leaves.lock().unwrap_or_else(PoisonError::into_inner);
        if cache.len() >= LEAF_CACHE_SIZE {
            cache.clear();
        }
        cache.insert(offset, Arc::clone(&leaf));
        Ok(leaf)
    }

    fn read_directory(&self, offset: u64, length: u64) -> Result<Vec<Entry>, MapsError> {
        let raw = self.read(offset, length)?;
        let bytes = match self.header.internal_compression {
            Compression::None => raw,
            Compression::Gzip => self.gunzip(&raw)?,
        };
        parse_directory(&bytes).ok_or_else(|| invalid(&self.path, "damaged directory"))
    }

    fn offset(&self, base: u64, relative: u64) -> Result<u64, MapsError> {
        base.checked_add(relative)
            .ok_or_else(|| invalid(&self.path, "offset out of range"))
    }

    fn read(&self, offset: u64, length: u64) -> Result<Vec<u8>, MapsError> {
        if length > MAX_BLOB_LEN {
            return Err(invalid(&self.path, "entry is implausibly large"));
        }
        let mut buffer = vec![0u8; length as usize];
        let mut file = self.file.lock().unwrap_or_else(PoisonError::into_inner);
        file.seek(SeekFrom::Start(offset))
            .map_err(MapsError::io(&self.path))?;
        file.read_exact(&mut buffer).map_err(|e| match e.kind() {
            std::io::ErrorKind::UnexpectedEof => {
                invalid(&self.path, "entry lies beyond the end of the file")
            }
            _ => MapsError::io(&self.path)(e),
        })?;
        Ok(buffer)
    }

    fn gunzip(&self, raw: &[u8]) -> Result<Vec<u8>, MapsError> {
        let mut out = Vec::with_capacity(raw.len() * 2);
        GzDecoder::new(raw)
            .take(MAX_BLOB_LEN)
            .read_to_end(&mut out)
            .map_err(|_| invalid(&self.path, "damaged gzip data"))?;
        Ok(out)
    }
}

fn invalid(path: &Path, reason: impl Into<String>) -> MapsError {
    MapsError::InvalidArchive {
        path: path.to_path_buf(),
        reason: reason.into(),
    }
}

fn parse_header(raw: &[u8; HEADER_LEN]) -> Result<Header, String> {
    if &raw[0..7] != b"PMTiles" {
        return Err("missing PMTiles signature".into());
    }
    if raw[7] != 3 {
        return Err(format!("unsupported PMTiles version {}", raw[7]));
    }
    let u64_at = |at: usize| {
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&raw[at..at + 8]);
        u64::from_le_bytes(bytes)
    };
    let degrees_at = |at: usize| {
        let mut bytes = [0u8; 4];
        bytes.copy_from_slice(&raw[at..at + 4]);
        f64::from(i32::from_le_bytes(bytes)) / 1e7
    };
    let compression = |byte: u8, what: &str| match byte {
        1 => Ok(Compression::None),
        2 => Ok(Compression::Gzip),
        other => Err(format!("unsupported {what} compression {other}")),
    };
    Ok(Header {
        tile_type: TileType::from_byte(raw[99]),
        min_zoom: raw[100],
        max_zoom: raw[101],
        bounds: [
            degrees_at(102),
            degrees_at(106),
            degrees_at(110),
            degrees_at(114),
        ],
        root_offset: u64_at(8),
        root_length: u64_at(16),
        leaves_offset: u64_at(40),
        tiles_offset: u64_at(56),
        internal_compression: compression(raw[97], "directory")?,
        tile_compression: compression(raw[98], "tile")?,
    })
}

fn read_varint(bytes: &[u8], at: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *bytes.get(*at)?;
        *at += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

fn parse_directory(bytes: &[u8]) -> Option<Vec<Entry>> {
    let mut at = 0;
    let count = usize::try_from(read_varint(bytes, &mut at)?).ok()?;
    // Every entry takes at least four bytes, one per column.
    if count > bytes.len() / 4 + 1 {
        return None;
    }
    let mut entries = Vec::with_capacity(count);
    let mut tile_id = 0u64;
    for _ in 0..count {
        tile_id = tile_id.checked_add(read_varint(bytes, &mut at)?)?;
        entries.push(Entry {
            tile_id,
            offset: 0,
            length: 0,
            run_length: 0,
        });
    }
    for entry in &mut entries {
        entry.run_length = read_varint(bytes, &mut at)?;
    }
    for entry in &mut entries {
        entry.length = read_varint(bytes, &mut at)?;
    }
    let mut previous_end = None;
    for entry in &mut entries {
        entry.offset = match (read_varint(bytes, &mut at)?, previous_end) {
            (0, Some(end)) => end,
            (0, None) => return None,
            (value, _) => value - 1,
        };
        previous_end = Some(entry.offset.checked_add(entry.length)?);
    }
    Some(entries)
}

/// The last entry whose tile id is not greater than `id`.
fn find(directory: &[Entry], id: u64) -> Option<Entry> {
    let after = directory.partition_point(|entry| entry.tile_id <= id);
    after
        .checked_sub(1)
        .and_then(|index| directory.get(index))
        .copied()
}

/// Position of a tile in the archive order: zoom levels one after another,
/// each along a Hilbert curve. `None` if the coordinates do not exist.
pub fn tile_id(z: u8, x: u32, y: u32) -> Option<u64> {
    if z > 31 || u64::from(x) >= 1u64 << z || u64::from(y) >= 1u64 << z {
        return None;
    }
    // Tiles of all lower zoom levels: (4^z - 1) / 3.
    let before = ((1u64 << (2 * u32::from(z))) - 1) / 3;
    let (mut x, mut y) = (u64::from(x), u64::from(y));
    let mut position = 0u64;
    let mut side = (1u64 << z) >> 1;
    while side > 0 {
        let rx = u64::from(x & side != 0);
        let ry = u64::from(y & side != 0);
        position += side * side * ((3 * rx) ^ ry);
        // Rotate the quadrant; only the bits below `side` matter from here on.
        x &= side - 1;
        y &= side - 1;
        if ry == 0 {
            if rx == 1 {
                x = side - 1 - x;
                y = side - 1 - y;
            }
            std::mem::swap(&mut x, &mut y);
        }
        side >>= 1;
    }
    Some(before + position)
}
