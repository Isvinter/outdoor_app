//! Where the current glide meets the terrain.
//!
//! Walks along the course in fixed steps, comparing the height of the glide line with
//! the ground below. Pure: the ground comes in through [`Terrain`].

use crate::geo::destination;
use crate::terrain::Terrain;

/// Distance between two ground checks along the course.
pub const LANDING_STEP_M: f64 = 50.0;
/// Farther than this nothing is searched.
pub const LANDING_MAX_RANGE_M: f64 = 50_000.0;

/// The glide to follow.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LandingQuery {
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    /// Height above mean sea level (GPS altitude), like the terrain.
    pub altitude_m: f64,
    pub course_deg: f64,
    /// Glide ratio over ground (distance per height lost).
    pub glide_ratio: f64,
}

/// Where the glide line reaches the ground.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LandingPoint {
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    pub distance_m: f64,
}

/// Finds the first point along the course where the glide line reaches the ground.
/// `None` if there is none within range, the start is already below ground, or the
/// ground is unknown anywhere on the way (better nothing than something wrong).
pub fn find_landing(query: &LandingQuery, terrain: &dyn Terrain) -> Option<LandingPoint> {
    if query.glide_ratio.is_nan() || query.glide_ratio <= 0.0 {
        return None;
    }
    // Line height minus ground height at a distance along the course.
    let clearance = |distance_m: f64| -> Option<f64> {
        let (lat, lon) = destination(
            query.latitude_deg,
            query.longitude_deg,
            query.course_deg,
            distance_m,
        );
        let ground = terrain.elevation_m(lat, lon)?;
        Some(query.altitude_m - distance_m / query.glide_ratio - ground)
    };

    let mut previous = clearance(0.0)?;
    if previous <= 0.0 {
        return None;
    }
    let steps = (LANDING_MAX_RANGE_M / LANDING_STEP_M) as usize;
    for k in 1..=steps {
        let distance = k as f64 * LANDING_STEP_M;
        let current = clearance(distance)?;
        if current <= 0.0 {
            // Linear interpolation of the zero crossing between the two steps.
            let fraction = previous / (previous - current);
            let hit = distance - LANDING_STEP_M + fraction * LANDING_STEP_M;
            let (lat, lon) = destination(
                query.latitude_deg,
                query.longitude_deg,
                query.course_deg,
                hit,
            );
            return Some(LandingPoint {
                latitude_deg: lat,
                longitude_deg: lon,
                distance_m: hit,
            });
        }
        previous = current;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::distance_m;
    use crate::terrain::FlatTerrain;

    const LAT: f64 = 47.0;
    const LON: f64 = 11.0;

    fn query(altitude_m: f64, course_deg: f64, glide_ratio: f64) -> LandingQuery {
        LandingQuery {
            latitude_deg: LAT,
            longitude_deg: LON,
            altitude_m,
            course_deg,
            glide_ratio,
        }
    }

    /// Ground rising 1 m per 10 m northwards of the start latitude.
    struct Slope;
    impl Terrain for Slope {
        fn elevation_m(&self, lat_deg: f64, lon_deg: f64) -> Option<f64> {
            Some(600.0 + distance_m(LAT, LON, lat_deg, lon_deg) / 10.0)
        }
    }

    /// Flat 600 m, but known only within 3 km of the start.
    struct Island;
    impl Terrain for Island {
        fn elevation_m(&self, lat_deg: f64, lon_deg: f64) -> Option<f64> {
            (distance_m(LAT, LON, lat_deg, lon_deg) < 3000.0).then_some(600.0)
        }
    }

    #[test]
    fn flat_ground_lands_at_height_times_ratio() {
        let point = find_landing(&query(1600.0, 90.0, 8.0), &FlatTerrain(600.0)).unwrap();
        assert!((point.distance_m - 8000.0).abs() < 1.0, "{point:?}");
        let (lat, lon) = destination(LAT, LON, 90.0, 8000.0);
        assert!((point.latitude_deg - lat).abs() < 1e-4);
        assert!((point.longitude_deg - lon).abs() < 1e-4);
    }

    #[test]
    fn a_slope_ahead_stops_the_glide_early() {
        let point = find_landing(&query(1600.0, 0.0, 8.0), &Slope).unwrap();
        assert!((point.distance_m - 4444.0).abs() < 5.0, "{point:?}");
    }

    #[test]
    fn backwards_course_lands_behind() {
        let point = find_landing(&query(1600.0, 180.0, 8.0), &FlatTerrain(600.0)).unwrap();
        assert!(point.latitude_deg < LAT);
    }

    #[test]
    fn leaving_the_regions_gives_nothing() {
        assert_eq!(find_landing(&query(1600.0, 0.0, 8.0), &Island), None);
    }

    #[test]
    fn no_hit_within_range_gives_nothing() {
        assert_eq!(
            find_landing(&query(8000.0, 0.0, 40.0), &FlatTerrain(0.0)),
            None
        );
    }

    #[test]
    fn below_ground_gives_nothing() {
        assert_eq!(
            find_landing(&query(500.0, 0.0, 8.0), &FlatTerrain(600.0)),
            None
        );
    }
}
