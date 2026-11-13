//! Slope angle tiles from elevation tiles.
//!
//! Reads a PMTiles archive of Terrarium-encoded elevation tiles and writes an
//! archive of raster tiles that colour terrain by slope angle, in the classes
//! avalanche maps use. See `PLAN.md`, section 6.8, for the reasoning.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

pub use core_maps::decode_terrarium;
use core_maps::{
    Archive, ArchiveInfo, DemTile, MapsError, TileType, decode_dem_tile, write_archive,
};
use rayon::prelude::*;
use thiserror::Error;

/// Colours (RGB) of the slope classes: below 30° (not drawn), 30–35° yellow,
/// 35–40° orange, 40–45° red, 45° and steeper violet.
pub const CLASS_COLORS: [[u8; 3]; 5] = [
    [0, 0, 0],
    [0xf7, 0xe1, 0x3c],
    [0xf5, 0x95, 0x00],
    [0xe0, 0x24, 0x1c],
    [0x8e, 0x3f, 0xa8],
];
/// Lower bounds in degrees of the classes 1 to 4.
const CLASS_LIMITS: [f32; 4] = [30.0, 35.0, 40.0, 45.0];
/// Length of the equator in metres (Web Mercator sphere).
const EQUATOR_M: f64 = 40_075_016.685_578_49;
/// Decoded elevation tiles kept in memory at most.
const DEM_CACHE_SIZE: usize = 256;

#[derive(Debug, Error)]
pub enum SlopeError {
    #[error(transparent)]
    Maps(#[from] MapsError),
    #[error("elevation tile {z}/{x}/{y} cannot be decoded: {reason}")]
    Decode {
        z: u8,
        x: u32,
        y: u32,
        reason: String,
    },
    #[error("elevation tiles of type {0:?} are not supported, expected WebP or PNG")]
    UnsupportedTiles(TileType),
    #[error("cannot start worker threads: {0}")]
    Threads(String),
}

/// Slope class of an angle in degrees: 0 (below 30° or unknown) to 4 (45° and steeper).
pub fn classify(degrees: f32) -> u8 {
    CLASS_LIMITS
        .iter()
        .filter(|limit| degrees >= **limit)
        .count() as u8
}

/// Ground size in metres of one pixel of a `tile_size` pixel tile at zoom `z` and latitude `lat`.
pub fn pixel_size_m(z: u8, tile_size: usize, lat: f64) -> f64 {
    EQUATOR_M * lat.to_radians().cos() / (f64::from(1u32 << z.min(31)) * tile_size as f64)
}

/// Latitude in degrees of a (fractional) tile row.
fn latitude(z: u8, row: f64) -> f64 {
    let n = f64::from(1u32 << z.min(31));
    (std::f64::consts::PI * (1.0 - 2.0 * row / n))
        .sinh()
        .atan()
        .to_degrees()
}

/// Slope angles in degrees of one tile, row by row; `NaN` where unknown.
#[derive(Debug, Clone, PartialEq)]
pub struct Grid {
    /// Width and height in pixels.
    pub size: usize,
    pub values: Vec<f32>,
}

/// Computes the slope angles of tile row `y` at zoom `z` from its elevations.
///
/// `dem` holds `size + 2` rows of `size + 2` elevations: the tile plus one
/// pixel of its neighbours on every side, so that tiles join without seams.
/// The gradient follows Horn (1981); a pixel is unknown if any elevation in
/// its 3×3 window is.
pub fn slope_grid(dem: &[f32], size: usize, z: u8, y: u32) -> Grid {
    let stride = size + 2;
    let mut values = Vec::with_capacity(size * size);
    if dem.len() != stride * stride {
        return Grid {
            size,
            values: vec![f32::NAN; size * size],
        };
    }
    for row in 0..size {
        // Web Mercator stretches with latitude; the pixel size is the same in both directions.
        let lat = latitude(z, f64::from(y) + (row as f64 + 0.5) / size as f64);
        let scale = 1.0 / (8.0 * pixel_size_m(z, size, lat) as f32);
        let (north, here, south) = (row * stride, (row + 1) * stride, (row + 2) * stride);
        for col in 0..size {
            let (a, b, c) = (dem[north + col], dem[north + col + 1], dem[north + col + 2]);
            let (d, f) = (dem[here + col], dem[here + col + 2]);
            let (g, h, i) = (dem[south + col], dem[south + col + 1], dem[south + col + 2]);
            let dz_dx = ((c + 2.0 * f + i) - (a + 2.0 * d + g)) * scale;
            let dz_dy = ((g + 2.0 * h + i) - (a + 2.0 * b + c)) * scale;
            // NaN in the window (or at the pixel itself) propagates.
            let slope = dz_dx.hypot(dz_dy).atan().to_degrees() + 0.0 * dem[here + col + 1];
            values.push(slope);
        }
    }
    Grid { size, values }
}

/// Combines four tiles (north-west, north-east, south-west, south-east) into
/// their parent by averaging 2×2 pixels; unknown pixels do not count. `None`
/// if there is no child at all.
pub fn downsample(children: [Option<&Grid>; 4]) -> Option<Grid> {
    let size = children.iter().flatten().next()?.size;
    let half = size / 2;
    let mut values = vec![f32::NAN; size * size];
    for (index, child) in children.iter().enumerate() {
        let Some(child) =
            child.filter(|child| child.size == size && child.values.len() == size * size)
        else {
            continue;
        };
        let (x0, y0) = ((index % 2) * half, (index / 2) * half);
        for row in 0..half {
            for col in 0..half {
                let at = 2 * row * size + 2 * col;
                let block = [
                    child.values[at],
                    child.values[at + 1],
                    child.values[at + size],
                    child.values[at + size + 1],
                ];
                let known = block.iter().filter(|value| !value.is_nan());
                let count = known.clone().count();
                if count > 0 {
                    values[(y0 + row) * size + x0 + col] = known.sum::<f32>() / count as f32;
                }
            }
        }
    }
    Some(Grid { size, values })
}

/// Encodes a tile as PNG with one palette entry per slope class. `None` if
/// nothing in the tile reaches the first class.
pub fn encode_png(grid: &Grid) -> Result<Option<Vec<u8>>, png::EncodingError> {
    let pixels: Vec<u8> = grid.values.iter().map(|value| classify(*value)).collect();
    if pixels.iter().all(|class| *class == 0) {
        return Ok(None);
    }
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, grid.size as u32, grid.size as u32);
    encoder.set_color(png::ColorType::Indexed);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_palette(CLASS_COLORS.concat());
    encoder.set_trns(vec![0u8, 255, 255, 255, 255]);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&pixels)?;
    writer.finish()?;
    Ok(Some(out))
}

/// Settings of a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// Zoom level at which slope is computed; limited by the elevation data.
    pub zoom: u8,
    /// Lowest zoom level to write; levels below `zoom` are averaged.
    pub min_zoom: u8,
    /// Worker threads; all cores if `None`.
    pub threads: Option<usize>,
}

/// What a run did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub zoom: u8,
    pub min_zoom: u8,
    /// Tiles at `zoom` for which slope was computed.
    pub tiles_computed: u64,
    /// Tiles in the output; tiles without steep terrain are left out.
    pub tiles_written: u64,
    /// Size of the output archive.
    pub bytes: u64,
}

type Dem = DemTile;

type OutputTile = (u8, u32, u32, Vec<u8>);
/// Decoded elevation tiles by column and row; `None` for tiles the archive lacks.
type DemCache = HashMap<(u32, u32), Option<Arc<Dem>>>;

struct Job<'a> {
    archive: &'a Archive,
    zoom: u8,
    bounds: [f64; 4],
    cache: Mutex<DemCache>,
    output: Mutex<Vec<OutputTile>>,
    computed: AtomicU64,
}

impl Job<'_> {
    fn decode(&self, x: u32, y: u32, data: &[u8]) -> Result<Dem, SlopeError> {
        decode_dem_tile(self.archive.header().tile_type, data).map_err(|reason| {
            SlopeError::Decode {
                z: self.zoom,
                x,
                y,
                reason,
            }
        })
    }

    /// The elevation tile at the compute zoom, if the archive has it.
    fn dem(&self, x: i64, y: i64) -> Result<Option<Arc<Dem>>, SlopeError> {
        let side = 1i64 << self.zoom;
        if !(0..side).contains(&x) || !(0..side).contains(&y) {
            return Ok(None);
        }
        let key = (x as u32, y as u32);
        if let Some(cached) = self
            .cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&key)
        {
            return Ok(cached.clone());
        }
        let dem = match self.archive.tile(self.zoom, key.0, key.1)? {
            Some(data) => Some(Arc::new(self.decode(key.0, key.1, &data)?)),
            None => None,
        };
        let mut cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
        if cache.len() >= DEM_CACHE_SIZE {
            cache.clear();
        }
        cache.insert(key, dem.clone());
        Ok(dem)
    }

    /// Slope of one tile at the compute zoom, using the neighbours for the border.
    fn leaf(&self, x: u32, y: u32) -> Result<Option<Grid>, SlopeError> {
        let Some(centre) = self.dem(i64::from(x), i64::from(y))? else {
            return Ok(None);
        };
        let size = centre.size;
        let mut around: [Option<Arc<Dem>>; 9] = Default::default();
        for (index, slot) in around.iter_mut().enumerate() {
            let (dx, dy) = (index as i64 % 3 - 1, index as i64 / 3 - 1);
            *slot = match (dx, dy) {
                (0, 0) => Some(Arc::clone(&centre)),
                _ => self
                    .dem(i64::from(x) + dx, i64::from(y) + dy)?
                    .filter(|dem| dem.size == size),
            };
        }
        let stride = size + 2;
        let last = size as i64 - 1;
        let mut bordered = Vec::with_capacity(stride * stride);
        for row in -1..=size as i64 {
            for col in -1..=size as i64 {
                let tile = |v: i64| {
                    if v < 0 {
                        0
                    } else if v > last {
                        2
                    } else {
                        1
                    }
                };
                let wrap = |v: i64| v.rem_euclid(size as i64) as usize;
                let height = match &around[tile(row) * 3 + tile(col)] {
                    Some(dem) => dem.heights[wrap(row) * size + wrap(col)],
                    // No neighbour: repeat the edge of this tile.
                    None => {
                        centre.heights
                            [row.clamp(0, last) as usize * size + col.clamp(0, last) as usize]
                    }
                };
                bordered.push(height);
            }
        }
        self.computed.fetch_add(1, Ordering::Relaxed);
        Ok(Some(slope_grid(&bordered, size, self.zoom, y)))
    }

    fn outside(&self, z: u8, x: u32, y: u32) -> bool {
        let n = f64::from(1u32 << z);
        let (west, east) = (
            f64::from(x) / n * 360.0 - 180.0,
            f64::from(x + 1) / n * 360.0 - 180.0,
        );
        let (north, south) = (latitude(z, f64::from(y)), latitude(z, f64::from(y + 1)));
        east < self.bounds[0]
            || west > self.bounds[2]
            || north < self.bounds[1]
            || south > self.bounds[3]
    }

    /// Computes a tile and everything below it, writing all of them.
    fn build(&self, z: u8, x: u32, y: u32) -> Result<Option<Grid>, SlopeError> {
        if self.outside(z, x, y) {
            return Ok(None);
        }
        let grid = if z == self.zoom {
            self.leaf(x, y)?
        } else {
            let children: Vec<Option<Grid>> = [(0, 0), (1, 0), (0, 1), (1, 1)]
                .into_par_iter()
                .map(|(dx, dy)| self.build(z + 1, 2 * x + dx, 2 * y + dy))
                .collect::<Result<_, _>>()?;
            downsample([
                children[0].as_ref(),
                children[1].as_ref(),
                children[2].as_ref(),
                children[3].as_ref(),
            ])
        };
        if let Some(grid) = &grid {
            let png = encode_png(grid).map_err(|e| SlopeError::Decode {
                z,
                x,
                y,
                reason: format!("cannot encode: {e}"),
            })?;
            if let Some(png) = png {
                self.output
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push((z, x, y, png));
            }
        }
        Ok(grid)
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

/// Derives the slope archive `output` from the elevation archive `input`.
pub fn run(input: &Path, output: &Path, options: &Options) -> Result<Summary, SlopeError> {
    let archive = Archive::open(input)?;
    let header = archive.header().clone();
    if !matches!(header.tile_type, TileType::Webp | TileType::Png) {
        return Err(SlopeError::UnsupportedTiles(header.tile_type));
    }
    let zoom = options.zoom.min(header.max_zoom).min(24);
    let min_zoom = options.min_zoom.min(zoom);
    let job = Job {
        archive: &archive,
        zoom,
        bounds: header.bounds,
        cache: Mutex::default(),
        output: Mutex::default(),
        computed: AtomicU64::new(0),
    };

    let (columns, rows) = tile_range(min_zoom, &header.bounds);
    let roots: Vec<(u32, u32)> = columns
        .flat_map(|x| rows.clone().map(move |y| (x, y)))
        .collect();
    let work = || {
        roots
            .par_iter()
            .try_for_each(|(x, y)| job.build(min_zoom, *x, *y).map(|_| ()))
    };
    match options.threads {
        Some(threads) => rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .map_err(|e| SlopeError::Threads(e.to_string()))?
            .install(work)?,
        None => work()?,
    }

    let tiles = job
        .output
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner);
    let tiles_written = tiles.len() as u64;
    let info = ArchiveInfo {
        tile_type: TileType::Png,
        bounds: header.bounds,
    };
    let bytes = write_archive(output, tiles, &info)?;
    Ok(Summary {
        zoom,
        min_zoom,
        tiles_computed: job.computed.into_inner(),
        tiles_written,
        bytes,
    })
}
