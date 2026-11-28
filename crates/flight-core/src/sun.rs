//! Sunrise and solar date, without any system clock.

use crate::time::{Date, UtcDateTime};

/// Result of a sunrise calculation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sunrise {
    /// The sun rises at this moment (seconds since the Unix epoch, UTC).
    At { unix_seconds: i64 },
    /// Polar day: the sun does not set.
    AlwaysUp,
    /// Polar night: the sun does not rise.
    AlwaysDown,
}

/// Sunrise on `date` at the given place.
///
/// Uses the NOAA "general solar position" approximation (equation of time and solar
/// declination as Fourier series, zenith 90.833 degrees); accurate to about +-2 minutes
/// at mid latitudes. `date` is the UTC day the returned time is counted from, so for
/// far-west longitudes the result can fall just past that day's end, and for far-east
/// longitudes just before that day's start (on the previous UTC day).
pub fn sunrise(date: Date, latitude_deg: f64, longitude_deg: f64) -> Sunrise {
    let jan_first = Date::new(date.year(), 1, 1).expect("1 January always exists");
    let day_of_year = (date.days_since_unix_epoch() - jan_first.days_since_unix_epoch()) as f64;
    // Fractional year in radians, evaluated at noon.
    let g = std::f64::consts::TAU / 365.0 * day_of_year;

    let eqtime = 229.18
        * (0.000075 + 0.001868 * g.cos()
            - 0.032077 * g.sin()
            - 0.014615 * (2.0 * g).cos()
            - 0.040849 * (2.0 * g).sin());
    let decl = 0.006918 - 0.399912 * g.cos() + 0.070257 * g.sin() - 0.006758 * (2.0 * g).cos()
        + 0.000907 * (2.0 * g).sin()
        - 0.002697 * (3.0 * g).cos()
        + 0.00148 * (3.0 * g).sin();

    let lat = latitude_deg.to_radians();
    let cos_ha = 90.833_f64.to_radians().cos() / (lat.cos() * decl.cos()) - lat.tan() * decl.tan();
    if cos_ha > 1.0 {
        return Sunrise::AlwaysDown;
    }
    if cos_ha < -1.0 {
        return Sunrise::AlwaysUp;
    }
    let ha_deg = cos_ha.acos().to_degrees();
    let minutes_utc = 720.0 - 4.0 * (longitude_deg + ha_deg) - eqtime;
    Sunrise::At {
        unix_seconds: date.days_since_unix_epoch() * 86_400 + (minutes_utc * 60.0).round() as i64,
    }
}

/// The calendar date at local solar time (4 minutes per degree of longitude).
pub fn solar_date(unix_seconds: i64, longitude_deg: f64) -> Date {
    UtcDateTime::from_unix_seconds(unix_seconds + (longitude_deg * 240.0).round() as i64).date
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc_minutes(s: Sunrise) -> f64 {
        match s {
            Sunrise::At { unix_seconds } => (unix_seconds.rem_euclid(86_400)) as f64 / 60.0,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn munich_midsummer() {
        // published 05:13 CEST
        let m = utc_minutes(sunrise(Date::new(2026, 6, 21).unwrap(), 48.137, 11.575));
        assert!((m - (3.0 * 60.0 + 13.0)).abs() <= 3.0, "{m}");
    }

    #[test]
    fn munich_midwinter() {
        // published 08:02 CET
        let m = utc_minutes(sunrise(Date::new(2026, 12, 21).unwrap(), 48.137, 11.575));
        assert!((m - (7.0 * 60.0 + 2.0)).abs() <= 3.0, "{m}");
    }

    #[test]
    fn cusco_june() {
        // 06:09 local (UTC-5)
        let m = utc_minutes(sunrise(Date::new(2026, 6, 21).unwrap(), -13.53, -71.97));
        assert!((m - (11.0 * 60.0 + 9.0)).abs() <= 3.0, "{m}");
    }

    #[test]
    fn sunrise_lies_on_the_given_date() {
        let date = Date::new(2026, 6, 21).unwrap();
        // `let ... else` leaves via panic if the pattern does not match.
        let Sunrise::At { unix_seconds } = sunrise(date, -13.53, -71.97) else {
            panic!()
        };
        assert_eq!(
            unix_seconds.div_euclid(86_400),
            date.days_since_unix_epoch()
        );
    }

    #[test]
    fn polar_day_and_night() {
        assert_eq!(
            sunrise(Date::new(2026, 6, 21).unwrap(), 69.65, 18.96),
            Sunrise::AlwaysUp
        );
        assert_eq!(
            sunrise(Date::new(2026, 12, 21).unwrap(), 69.65, 18.96),
            Sunrise::AlwaysDown
        );
    }

    #[test]
    fn solar_date_follows_longitude() {
        // 2026-06-22 02:00 UTC is still the evening of 21 June in Cusco.
        let t = Date::new(2026, 6, 22).unwrap().days_since_unix_epoch() * 86_400 + 2 * 3600;
        assert_eq!(solar_date(t, -71.97), Date::new(2026, 6, 21).unwrap());
        assert_eq!(solar_date(t, 11.5), Date::new(2026, 6, 22).unwrap());
    }
}
