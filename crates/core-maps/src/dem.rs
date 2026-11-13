//! Elevation from Terrarium-encoded tiles.

use std::io::Cursor;
use std::sync::Arc;

use core_track::geo::haversine_m;
use serde::{Deserialize, Serialize};

use crate::pmtiles::TileType;

/// Decoded elevation tiles kept in memory (512 px tiles take 1 MB each).
const CACHE_SIZE: usize = 24;

/// Elevation in metres of a Terrarium-encoded pixel.
pub fn decode_terrarium([r, g, b]: [u8; 3]) -> f32 {
    f32::from(r) * 256.0 + f32::from(g) + f32::from(b) / 256.0 - 32768.0
}

/// Elevations of one tile in metres, row by row from the north-west corner.
#[derive(Debug, Clone, PartialEq)]
pub struct DemTile {
    /// Width and height in pixels.
    pub size: usize,
    pub heights: Vec<f32>,
}

impl DemTile {
    /// Elevation at a position inside the tile (`0..1` from the west and the
    /// north edge), interpolated between the four nearest pixel centres.
    /// Towards the edges the outermost pixels are repeated.
    pub fn sample(&self, x: f64, y: f64) -> f32 {
        let last = (self.size.max(1) - 1) as f64;
        let col = (x * self.size as f64 - 0.5).clamp(0.0, last);
        let row = (y * self.size as f64 - 0.5).clamp(0.0, last);
        let (col0, row0) = (col.floor() as usize, row.floor() as usize);
        let (col1, row1) = ((col0 + 1).min(last as usize), (row0 + 1).min(last as usize));
        let (fx, fy) = ((col - col0 as f64) as f32, (row - row0 as f64) as f32);
        let at = |row: usize, col: usize| {
            self.heights
                .get(row * self.size + col)
                .copied()
                .unwrap_or(f32::NAN)
        };
        let north = at(row0, col0) * (1.0 - fx) + at(row0, col1) * fx;
        let south = at(row1, col0) * (1.0 - fx) + at(row1, col1) * fx;
        north * (1.0 - fy) + south * fy
    }
}

/// Decodes a WebP or PNG tile holding Terrarium-encoded elevations.
pub fn decode_dem_tile(tile_type: TileType, data: &[u8]) -> Result<DemTile, String> {
    let (width, height, channels, pixels) = match tile_type {
        TileType::Webp => {
            let mut decoder =
                image_webp::WebPDecoder::new(Cursor::new(data)).map_err(|e| e.to_string())?;
            let (width, height) = decoder.dimensions();
            let channels = if decoder.has_alpha() { 4 } else { 3 };
            let len = decoder.output_buffer_size().ok_or("image too large")?;
            let mut pixels = vec![0u8; len];
            decoder.read_image(&mut pixels).map_err(|e| e.to_string())?;
            (width, height, channels, pixels)
        }
        TileType::Png => {
            let mut reader = png::Decoder::new(Cursor::new(data))
                .read_info()
                .map_err(|e| e.to_string())?;
            let len = reader.output_buffer_size().ok_or("image too large")?;
            let mut pixels = vec![0u8; len];
            let frame = reader.next_frame(&mut pixels).map_err(|e| e.to_string())?;
            let channels = match (frame.color_type, frame.bit_depth) {
                (png::ColorType::Rgb, png::BitDepth::Eight) => 3,
                (png::ColorType::Rgba, png::BitDepth::Eight) => 4,
                other => return Err(format!("unexpected pixel format {other:?}")),
            };
            pixels.truncate(frame.buffer_size());
            (frame.width, frame.height, channels, pixels)
        }
        other => {
            return Err(format!(
                "elevation tiles of type {other:?} are not supported"
            ));
        }
    };
    let size = width as usize;
    if width != height || size == 0 || pixels.len() != size * size * channels {
        return Err(format!("unexpected image size {width}x{height}"));
    }
    let heights = pixels
        .chunks_exact(channels)
        .map(|p| decode_terrarium([p[0], p[1], p[2]]))
        .collect();
    Ok(DemTile { size, heights })
}

/// One sample of an elevation profile.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProfilePoint {
    /// Distance from the start of the line in metres.
    pub distance_m: f64,
    pub lat: f64,
    pub lon: f64,
    /// Ground elevation in metres; `None` where there is no elevation data.
    pub ele: Option<f32>,
}

/// Points along a line of `(lat, lon)` at most `step_m` apart, with their
/// distance from the start: `(distance_m, lat, lon)`. Keeps the given points.
pub(crate) fn resample(line: &[(f64, f64)], step_m: f64) -> Vec<(f64, f64, f64)> {
    let Some(first) = line.first() else {
        return Vec::new();
    };
    let step_m = if step_m.is_finite() && step_m > 0.0 {
        step_m
    } else {
        f64::MAX
    };
    let mut points = vec![(0.0, first.0, first.1)];
    let mut travelled = 0.0;
    for pair in line.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        let length = haversine_m(from.0, from.1, to.0, to.1);
        if length <= 0.0 || !length.is_finite() {
            continue;
        }
        let steps = (length / step_m).ceil().max(1.0) as usize;
        for step in 1..=steps {
            let t = step as f64 / steps as f64;
            points.push((
                travelled + length * t,
                from.0 + (to.0 - from.0) * t,
                from.1 + (to.1 - from.1) * t,
            ));
        }
        travelled += length;
    }
    points
}

type Key = (String, u8, u32, u32);

/// Small most-recently-used cache of decoded tiles, keyed by region and tile.
#[derive(Default)]
pub(crate) struct DemCache(Vec<(Key, Arc<DemTile>)>);

impl DemCache {
    pub(crate) fn get(&mut self, key: &Key) -> Option<Arc<DemTile>> {
        let index = self.0.iter().position(|(k, _)| k == key)?;
        let entry = self.0.remove(index);
        let tile = Arc::clone(&entry.1);
        self.0.insert(0, entry);
        Some(tile)
    }

    pub(crate) fn put(&mut self, key: Key, tile: Arc<DemTile>) {
        self.0.truncate(CACHE_SIZE - 1);
        self.0.insert(0, (key, tile));
    }

    pub(crate) fn clear(&mut self) {
        self.0.clear();
    }
}
