//! Small spherical geometry helpers.

/// Mean Earth radius in metres (IUGG).
pub const EARTH_RADIUS_M: f64 = 6_371_008.8;

/// Great-circle distance in metres between two WGS 84 positions given in degrees.
pub fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (phi1, phi2) = (lat1.to_radians(), lat2.to_radians());
    let d_phi = phi2 - phi1;
    let d_lambda = (lon2 - lon1).to_radians();
    let a = (d_phi / 2.0).sin().powi(2) + phi1.cos() * phi2.cos() * (d_lambda / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_M * a.sqrt().min(1.0).asin()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_degree_of_latitude_is_about_111_km() {
        let d = haversine_m(47.0, 11.0, 48.0, 11.0);
        assert!((d - 111_195.08).abs() < 0.01, "{d}");
    }

    #[test]
    fn longitude_distance_shrinks_with_cosine_of_latitude() {
        let d = haversine_m(60.0, 10.0, 60.0, 11.0);
        assert!((d - 111_195.08 * 0.5).abs() < 5.0, "{d}");
    }

    #[test]
    fn identical_and_antipodal_points_are_finite() {
        assert_eq!(haversine_m(47.0, 11.0, 47.0, 11.0), 0.0);
        let d = haversine_m(0.0, 0.0, 0.0, 180.0);
        assert!(
            (d - std::f64::consts::PI * EARTH_RADIUS_M).abs() < 1.0,
            "{d}"
        );
    }
}
