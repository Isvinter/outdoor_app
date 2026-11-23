//! Joining the ways of one street into one entry.

use std::collections::HashMap;

use crate::towns::distance_m;

/// A way joins a street of the same name whose centre is this near (metres).
const JOIN_RADIUS: f64 = 2_000.0;

struct Street {
    lat_sum: f64,
    lon_sum: f64,
    ways: u32,
    alt_names: Vec<String>,
}

impl Street {
    fn centre(&self) -> (f64, f64) {
        let n = f64::from(self.ways);
        (self.lat_sum / n, self.lon_sum / n)
    }
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
    by_name: HashMap<String, Vec<Street>>,
}

impl Streets {
    pub fn add(&mut self, name: &str, alt_names: Vec<String>, lat: f64, lon: f64) {
        let streets = self.by_name.entry(name.to_string()).or_default();
        let nearest = streets
            .iter_mut()
            .map(|s| {
                let (clat, clon) = s.centre();
                (distance_m(lat, lon, clat, clon), s)
            })
            .filter(|(d, _)| *d <= JOIN_RADIUS)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, s)| s);
        match nearest {
            Some(street) => {
                street.lat_sum += lat;
                street.lon_sum += lon;
                street.ways += 1;
                for alt in alt_names {
                    if !street.alt_names.contains(&alt) {
                        street.alt_names.push(alt);
                    }
                }
            }
            None => streets.push(Street {
                lat_sum: lat,
                lon_sum: lon,
                ways: 1,
                alt_names,
            }),
        }
    }

    pub fn into_merged(self) -> impl Iterator<Item = Merged> {
        self.by_name.into_iter().flat_map(|(name, streets)| {
            streets.into_iter().map(move |s| {
                let (lat, lon) = s.centre();
                Merged {
                    name: name.clone(),
                    alt_names: s.alt_names,
                    lat,
                    lon,
                }
            })
        })
    }
}
