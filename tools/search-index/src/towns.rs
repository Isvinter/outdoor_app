//! The nearest place for an entry, from a coarse grid.

use std::collections::HashMap;

/// Cell size in degrees. The largest radius (15 km) fits in one cell up to
/// about 65 degrees of latitude, so the 3x3 cells around a point are enough.
const CELL: f64 = 0.5;

struct Town {
    name: String,
    lat: f64,
    lon: f64,
    radius: f64,
}

#[derive(Default)]
pub struct Towns {
    towns: Vec<Town>,
    grid: HashMap<(i32, i32), Vec<usize>>,
}

fn cell(lat: f64, lon: f64) -> (i32, i32) {
    ((lat / CELL).floor() as i32, (lon / CELL).floor() as i32)
}

/// Distance in metres, flat-earth approximation for short distances.
pub fn distance_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let dy = (lat2 - lat1).to_radians();
    let dx = (lon2 - lon1).to_radians() * ((lat1 + lat2) / 2.0).to_radians().cos();
    dx.hypot(dy) * 6_371_000.0
}

impl Towns {
    pub fn add(&mut self, name: &str, lat: f64, lon: f64, radius: f64) {
        let index = self.towns.len();
        self.towns.push(Town {
            name: name.to_string(),
            lat,
            lon,
            radius,
        });
        self.grid.entry(cell(lat, lon)).or_default().push(index);
    }

    /// The name of the closest place whose own radius reaches the position.
    pub fn nearest(&self, lat: f64, lon: f64) -> Option<&str> {
        let (row, col) = cell(lat, lon);
        let mut best: Option<(f64, &Town)> = None;
        for r in row - 1..=row + 1 {
            for c in col - 1..=col + 1 {
                for &i in self.grid.get(&(r, c)).into_iter().flatten() {
                    let town = &self.towns[i];
                    let d = distance_m(lat, lon, town.lat, town.lon);
                    if d <= town.radius && best.is_none_or(|(b, _)| d < b) {
                        best = Some((d, town));
                    }
                }
            }
        }
        best.map(|(_, t)| t.name.as_str())
    }
}
