//! Reads vector tiles of a PMTiles archive for inspection and tests.

pub mod mvt;

/// Range of tile columns and rows `(x0, y0, x1, y1)`, inclusive, that cover
/// the box west, south, east, north (degrees) at `zoom` in Web Mercator.
pub fn tile_range(bbox: [f64; 4], zoom: u8) -> (u32, u32, u32, u32) {
    let n = f64::from(1u32 << zoom);
    let max = (1u32 << zoom) - 1;
    let x = |lon: f64| (((lon + 180.0) / 360.0 * n).floor().max(0.0) as u32).min(max);
    let y = |lat: f64| {
        let rad = lat.clamp(-85.0511, 85.0511).to_radians();
        let v = (1.0 - (rad.tan() + 1.0 / rad.cos()).ln() / std::f64::consts::PI) / 2.0 * n;
        (v.floor().max(0.0) as u32).min(max)
    };
    let [west, south, east, north] = bbox;
    (x(west), y(north), x(east), y(south))
}
