//! The places around an entry, from a coarse grid.

use std::collections::HashMap;

/// Cell size in degrees. The largest radius (15 km) fits in one cell up to
/// about 65 degrees of latitude, so the 3x3 cells around a point are enough.
const CELL: f64 = 0.5;

/// What a place counts as when a street is searched together with a town.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Rank {
    City,
    Town,
    Village,
    /// A suburb, quarter, neighbourhood or hamlet.
    Part,
}

impl Rank {
    /// How the size of a place is stored in the search file.
    pub fn as_str(self) -> &'static str {
        match self {
            Rank::City => "city",
            Rank::Town => "town",
            Rank::Village => "village",
            Rank::Part => "part",
        }
    }
}

struct Town {
    /// The display name first, then the alternative names.
    names: Vec<String>,
    rank: Rank,
    lat: f64,
    lon: f64,
    radius: f64,
}

/// The towns of one position.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Located {
    /// The nearest place, shown with the entry.
    pub town: Option<String>,
    /// Everything the entry is found under in the town column: the nearest
    /// place and the nearest city, town and village, with alternative names.
    pub town_names: Vec<String>,
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

/// Keeps `candidate` if it is closer than what `best` holds.
fn closer(best: &mut Option<(f64, usize)>, candidate: (f64, usize)) {
    if best.is_none_or(|(d, _)| candidate.0 < d) {
        *best = Some(candidate);
    }
}

impl Towns {
    pub fn add(
        &mut self,
        name: &str,
        alt_names: &[String],
        rank: Rank,
        (lat, lon): (f64, f64),
        radius: f64,
    ) {
        let index = self.towns.len();
        let mut names = Vec::with_capacity(alt_names.len() + 1);
        names.push(name.to_string());
        names.extend_from_slice(alt_names);
        self.towns.push(Town {
            names,
            rank,
            lat,
            lon,
            radius,
        });
        self.grid.entry(cell(lat, lon)).or_default().push(index);
    }

    /// The closest place whose own radius reaches the position, and all names
    /// the position can be searched under.
    pub fn locate(&self, lat: f64, lon: f64) -> Located {
        let (row, col) = cell(lat, lon);
        let mut nearest = None;
        let (mut city, mut town, mut village) = (None, None, None);
        for r in row - 1..=row + 1 {
            for c in col - 1..=col + 1 {
                for &i in self.grid.get(&(r, c)).into_iter().flatten() {
                    let place = &self.towns[i];
                    let d = distance_m(lat, lon, place.lat, place.lon);
                    if d > place.radius {
                        continue;
                    }
                    closer(&mut nearest, (d, i));
                    match place.rank {
                        Rank::City => closer(&mut city, (d, i)),
                        Rank::Town => closer(&mut town, (d, i)),
                        Rank::Village => closer(&mut village, (d, i)),
                        Rank::Part => {}
                    }
                }
            }
        }
        let mut town_names: Vec<String> = Vec::new();
        for (_, i) in [nearest, city, town, village].into_iter().flatten() {
            for name in &self.towns[i].names {
                if !town_names.contains(name) {
                    town_names.push(name.clone());
                }
            }
        }
        Located {
            town: nearest.and_then(|(_, i)| self.towns[i].names.first().cloned()),
            town_names,
        }
    }
}
