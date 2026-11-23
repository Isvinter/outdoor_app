//! Joining the ways of one street into one entry.

use std::collections::HashMap;

use crate::towns::distance_m;

/// Ways of one name belong together if an end of one is this near to an end
/// of the other (metres), directly or through other ways.
const JOIN_DISTANCE: f64 = 150.0;
/// Metres per degree of latitude, rounded down so grid cells are never too small.
const METRES_PER_DEGREE: f64 = 110_000.0;

/// The ends of the ways by grid cell: index of the way and `(lat, lon)`.
type EndGrid = HashMap<(i64, i64), Vec<(usize, (f64, f64))>>;

struct Way {
    /// `(lat, lon)`, never empty.
    vertices: Vec<(f64, f64)>,
    alt_names: Vec<String>,
}

/// A merged street, ready to become an entry.
pub struct Merged {
    pub name: String,
    pub alt_names: Vec<String>,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Default)]
pub struct Streets {
    by_name: HashMap<String, Vec<Way>>,
}

impl Streets {
    /// Adds a way. A way without vertices has no place and is ignored.
    pub fn add(&mut self, name: &str, alt_names: Vec<String>, vertices: Vec<(f64, f64)>) {
        if vertices.is_empty() {
            return;
        }
        self.by_name.entry(name.to_string()).or_default().push(Way {
            vertices,
            alt_names,
        });
    }

    pub fn into_merged(self) -> impl Iterator<Item = Merged> {
        self.by_name.into_iter().flat_map(|(name, ways)| {
            let groups = connected(&ways);
            groups.into_iter().filter_map(move |group| {
                let (lat, lon) = on_street(&ways, &group)?;
                let mut alt_names: Vec<String> = Vec::new();
                for alt in group.iter().flat_map(|&i| &ways[i].alt_names) {
                    if !alt_names.contains(alt) {
                        alt_names.push(alt.clone());
                    }
                }
                Some(Merged {
                    name: name.clone(),
                    alt_names,
                    lat,
                    lon,
                })
            })
        })
    }
}

fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

/// The indices of the ways, grouped by what is connected end to end.
fn connected(ways: &[Way]) -> Vec<Vec<usize>> {
    let max_lat = ways
        .iter()
        .flat_map(|way| &way.vertices)
        .fold(0.0_f64, |max, v| max.max(v.0.abs()));
    let lat_cell = JOIN_DISTANCE / METRES_PER_DEGREE;
    // A degree of longitude is shortest at the highest latitude.
    let lon_cell = lat_cell / max_lat.min(89.0).to_radians().cos();
    let cell = |(lat, lon): (f64, f64)| {
        (
            (lat / lat_cell).floor() as i64,
            (lon / lon_cell).floor() as i64,
        )
    };

    let mut parent: Vec<usize> = (0..ways.len()).collect();
    let mut grid: EndGrid = HashMap::new();
    for (index, way) in ways.iter().enumerate() {
        let (Some(&first), Some(&last)) = (way.vertices.first(), way.vertices.last()) else {
            continue;
        };
        for end in [first, last] {
            let (row, col) = cell(end);
            for r in row - 1..=row + 1 {
                for c in col - 1..=col + 1 {
                    for &(other, at) in grid.get(&(r, c)).into_iter().flatten() {
                        if distance_m(end.0, end.1, at.0, at.1) <= JOIN_DISTANCE {
                            let (a, b) = (find(&mut parent, index), find(&mut parent, other));
                            parent[a] = b;
                        }
                    }
                }
            }
        }
        for end in [first, last] {
            grid.entry(cell(end)).or_default().push((index, end));
        }
    }

    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for index in 0..ways.len() {
        groups
            .entry(find(&mut parent, index))
            .or_default()
            .push(index);
    }
    groups.into_values().collect()
}

/// The vertex of the street closest to the mean of all its vertices.
fn on_street(ways: &[Way], group: &[usize]) -> Option<(f64, f64)> {
    let vertices = || group.iter().flat_map(|&i| &ways[i].vertices);
    let (mut lat, mut lon, mut count) = (0.0, 0.0, 0.0);
    for v in vertices() {
        lat += v.0;
        lon += v.1;
        count += 1.0;
    }
    let (lat, lon) = (lat / count, lon / count);
    vertices()
        .min_by(|a, b| distance_m(lat, lon, a.0, a.1).total_cmp(&distance_m(lat, lon, b.0, b.1)))
        .copied()
}
