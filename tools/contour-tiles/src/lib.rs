//! Contour lines from elevation tiles, as Mapbox Vector Tiles.

mod build;
mod isolines;
mod mvt;

pub use build::{ContourError, Options, Summary, run};
pub use isolines::{Grid, Isoline, isolines, simplify};
pub use mvt::{EXTENT, encode_tile};

/// Minor and major interval in metres at a zoom level, the same as the desktop
/// draws (`apps/desktop/ui/src/map.ts`, `CONTOUR_TILES`); `None` outside 11..=15.
pub fn intervals(z: u8) -> Option<(u32, u32)> {
    match z {
        11 => Some((100, 500)),
        12 => Some((50, 250)),
        13 | 14 => Some((20, 100)),
        15 => Some((10, 50)),
        _ => None,
    }
}
