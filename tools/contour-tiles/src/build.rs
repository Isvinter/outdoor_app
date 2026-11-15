//! Builds a contour archive from an elevation archive, tile by tile.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use core_maps::{
    Archive, ArchiveInfo, DemTile, MapsError, TileType, decode_dem_tile, write_archive,
};
use rayon::prelude::*;
use thiserror::Error;

use crate::{EXTENT, Grid, encode_tile, intervals, isolines, simplify};

/// Samples per tile side; cells are `EXTENT / CELLS` tile units wide.
const CELLS: usize = 256;
/// Cells added on every side, so that lines run on past the tile edge.
const BORDER: usize = 8;
/// Douglas–Peucker tolerance in tile units.
const TOLERANCE: f32 = 2.0;
/// Decoded elevation tiles kept at a time.
const DEM_CACHE_SIZE: usize = 512;

#[derive(Debug, Error)]
pub enum ContourError {
    #[error(transparent)]
    Maps(#[from] MapsError),
    #[error("elevation tile {z}/{x}/{y} cannot be decoded: {reason}")]
    Decode {
        z: u8,
        x: u32,
        y: u32,
        reason: String,
    },
    #[error("elevation tiles of type {0:?} are not supported (WebP or PNG expected)")]
    UnsupportedTiles(TileType),
    #[error("no contour zoom levels between {0} and {1} (11 to 15 are defined)")]
    NoZoom(u8, u8),
    #[error("worker threads: {0}")]
    Threads(String),
}

/// Settings of a run.
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    pub min_zoom: u8,
    pub max_zoom: u8,
    /// Worker threads; all cores if `None`.
    pub threads: Option<usize>,
    /// Only this box (west, south, east, north) instead of the whole archive.
    pub bounds: Option<[f64; 4]>,
}

/// What a run did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub tiles_written: u64,
    pub bytes: u64,
    /// Highest zoom of the elevation tiles; contours above it are interpolated.
    pub max_dem_zoom: u8,
}

type DemCache = HashMap<(u8, u32, u32), Option<Arc<DemTile>>>;

struct Job<'a> {
    archive: &'a Archive,
    max_dem_zoom: u8,
    cache: Mutex<DemCache>,
}

impl Job<'_> {
    fn dem(&self, z: u8, x: i64, y: i64) -> Result<Option<Arc<DemTile>>, ContourError> {
        let side = 1i64 << z;
        if !(0..side).contains(&x) || !(0..side).contains(&y) {
            return Ok(None);
        }
        let key = (z, x as u32, y as u32);
        if let Some(cached) = self
            .cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&key)
        {
            return Ok(cached.clone());
        }
        let dem = match self.archive.tile(z, key.1, key.2)? {
            Some(data) => Some(Arc::new(
                decode_dem_tile(self.archive.header().tile_type, &data).map_err(|reason| {
                    ContourError::Decode {
                        z,
                        x: key.1,
                        y: key.2,
                        reason,
                    }
                })?,
            )),
            None => None,
        };
        let mut cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
        if cache.len() >= DEM_CACHE_SIZE {
            cache.clear();
        }
        cache.insert(key, dem.clone());
        Ok(dem)
    }

    /// Elevations over tile `z/x/y` and its border; NaN where there is no data.
    fn grid(&self, z: u8, x: u32, y: u32) -> Result<Grid, ContourError> {
        let dem_zoom = z.min(self.max_dem_zoom);
        let scale = f64::from(1u32 << (z - dem_zoom));
        let side = CELLS + 1 + 2 * BORDER;
        let mut values = Vec::with_capacity(side * side);
        for row in 0..side {
            for col in 0..side {
                let offset = |i: usize| (i as f64 - BORDER as f64) / CELLS as f64;
                // Position in tiles of the elevation zoom.
                let gx = (f64::from(x) + offset(col)) / scale;
                let gy = (f64::from(y) + offset(row)) / scale;
                let (tx, ty) = (gx.floor(), gy.floor());
                let value = match self.dem(dem_zoom, tx as i64, ty as i64)? {
                    Some(dem) => dem.sample(gx - tx, gy - ty),
                    None => f32::NAN,
                };
                values.push(value);
            }
        }
        Ok(Grid { side, values })
    }

    /// The encoded tile, or `None` if no line crosses it.
    fn tile(
        &self,
        z: u8,
        x: u32,
        y: u32,
        (minor, major): (u32, u32),
    ) -> Result<Option<Vec<u8>>, ContourError> {
        let grid = self.grid(z, x, y)?;
        let unit = EXTENT as f32 / CELLS as f32;
        let lines: Vec<(i32, u8, Vec<[i32; 2]>)> = isolines(&grid, minor)
            .into_iter()
            .filter_map(|line| {
                let points: Vec<[f32; 2]> = line
                    .points
                    .iter()
                    .map(|[px, py]| [(px - BORDER as f32) * unit, (py - BORDER as f32) * unit])
                    .collect();
                let points: Vec<[i32; 2]> = simplify(&points, TOLERANCE)
                    .into_iter()
                    .map(|[px, py]| [px.round() as i32, py.round() as i32])
                    .collect();
                let level = u8::from(line.ele.rem_euclid(major as i32) == 0);
                (points.len() >= 2).then_some((line.ele, level, points))
            })
            .collect();
        Ok((!lines.is_empty()).then(|| encode_tile(&lines)))
    }
}

/// Tile column and row ranges covering `bounds` at zoom `z`.
fn tile_range(
    z: u8,
    bounds: &[f64; 4],
) -> (std::ops::RangeInclusive<u32>, std::ops::RangeInclusive<u32>) {
    let n = f64::from(1u32 << z);
    let max = (1u32 << z) - 1;
    let column = |lon: f64| (((lon + 180.0) / 360.0 * n).floor().max(0.0) as u32).min(max);
    let row = |lat: f64| {
        let lat = lat.clamp(-85.0511, 85.0511).to_radians();
        (((1.0 - lat.tan().asinh() / std::f64::consts::PI) / 2.0 * n)
            .floor()
            .max(0.0) as u32)
            .min(max)
    };
    (
        column(bounds[0])..=column(bounds[2]),
        row(bounds[3])..=row(bounds[1]),
    )
}

/// Derives the contour archive `output` from the elevation archive `input`.
pub fn run(input: &Path, output: &Path, options: &Options) -> Result<Summary, ContourError> {
    let archive = Archive::open(input)?;
    let header = archive.header().clone();
    if !matches!(header.tile_type, TileType::Webp | TileType::Png) {
        return Err(ContourError::UnsupportedTiles(header.tile_type));
    }
    let zooms: Vec<(u8, (u32, u32))> = (options.min_zoom..=options.max_zoom)
        .filter_map(|z| intervals(z).map(|i| (z, i)))
        .collect();
    if zooms.is_empty() {
        return Err(ContourError::NoZoom(options.min_zoom, options.max_zoom));
    }
    let bounds = options.bounds.unwrap_or(header.bounds);
    let job = Job {
        archive: &archive,
        max_dem_zoom: header.max_zoom,
        cache: Mutex::default(),
    };
    let work: Vec<(u8, u32, u32, (u32, u32))> = zooms
        .iter()
        .flat_map(|&(z, interval)| {
            let (columns, rows) = tile_range(z, &bounds);
            columns.flat_map(move |x| rows.clone().map(move |y| (z, x, y, interval)))
        })
        .collect();
    let build = || {
        work.par_iter()
            .map(|&(z, x, y, interval)| {
                Ok(job.tile(z, x, y, interval)?.map(|data| (z, x, y, data)))
            })
            .filter_map(Result::transpose)
            .collect::<Result<Vec<_>, ContourError>>()
    };
    let tiles = match options.threads {
        Some(threads) => rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .map_err(|e| ContourError::Threads(e.to_string()))?
            .install(build)?,
        None => build()?,
    };
    let tiles_written = tiles.len() as u64;
    let bytes = write_archive(
        output,
        tiles,
        &ArchiveInfo {
            tile_type: TileType::Mvt,
            bounds,
        },
    )?;
    Ok(Summary {
        tiles_written,
        bytes,
        max_dem_zoom: header.max_zoom,
    })
}
