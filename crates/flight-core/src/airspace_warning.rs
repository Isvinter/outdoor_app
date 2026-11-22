//! Warnings before flying into an airspace.
//!
//! Limits of airspaces come in three references, and each needs its own altitude of the
//! pilot:
//!
//! | limit              | compared with                                   |
//! |--------------------|-------------------------------------------------|
//! | flight level (FL)  | pressure altitude at 1013.25 hPa (barometer)    |
//! | height MSL         | altitude above mean sea level (GPS)             |
//! | height AGL         | GPS altitude minus the ground elevation         |
//!
//! When an altitude is missing the comparison errs on the side of a warning: an AGL
//! lower limit without known ground counts as the ground itself.

use crate::airspace::{Limit, Reference};

/// The altitudes of the pilot at one moment, each in its own reference.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Altitudes {
    /// Above mean sea level in metres (GPS).
    pub msl_m: Option<f64>,
    /// Pressure altitude at standard pressure in metres (barometer).
    pub pressure_m: Option<f64>,
    /// Ground elevation below the pilot above mean sea level in metres.
    pub ground_m: Option<f64>,
}

impl Altitudes {
    /// Altitude above mean sea level, from the GPS or else from the barometer (then off
    /// by the difference between QNH and standard pressure, a few dozen metres).
    fn msl(&self) -> Option<f64> {
        self.msl_m.or(self.pressure_m)
    }

    /// Pressure altitude, from the barometer or else from the GPS.
    fn pressure(&self) -> Option<f64> {
        self.pressure_m.or(self.msl_m)
    }

    /// How far the pilot is above a limit in metres (negative: below it), measured in
    /// the limit's own reference. `None` if that cannot be told; callers decide which
    /// side to assume.
    fn above(&self, limit: Limit) -> Option<f64> {
        match limit {
            Limit::Ground => self
                .msl()
                .zip(self.ground_m)
                .map(|(alt, ground)| alt - ground),
            Limit::Height {
                metres,
                reference: Reference::Msl,
                ..
            } => self.msl().map(|alt| alt - metres),
            Limit::Height {
                metres,
                reference: Reference::Agl,
                ..
            } => self
                .msl()
                .zip(self.ground_m)
                .map(|(alt, ground)| alt - ground - metres),
            Limit::FlightLevel(_) => {
                let metres = limit.metres()?;
                self.pressure().map(|alt| alt - metres)
            }
            Limit::Unlimited => Some(f64::NEG_INFINITY),
        }
    }
}

/// Where the pilot is in relation to the lower and upper limit of an airspace.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertical {
    /// Metres up to the lower limit: positive while below the airspace, zero or
    /// negative once above the lower limit. `-∞` for a lower limit at the ground.
    pub to_lower_m: f64,
    /// Metres down to the upper limit: positive while above the airspace, zero or
    /// negative once below the upper limit. `-∞` without upper limit.
    pub to_upper_m: f64,
}

impl Vertical {
    /// Compares the pilot's altitudes with the limits of an airspace. Where an altitude
    /// is missing, the pilot counts as inside the vertical range on that side.
    pub fn new(lower: Limit, upper: Limit, altitudes: &Altitudes) -> Self {
        let to_lower_m = match lower {
            Limit::Ground => f64::NEG_INFINITY,
            _ => altitudes
                .above(lower)
                .map_or(f64::NEG_INFINITY, |above| -above),
        };
        let to_upper_m = match upper {
            Limit::Unlimited => f64::NEG_INFINITY,
            _ => altitudes.above(upper).unwrap_or(f64::NEG_INFINITY),
        };
        Vertical {
            to_lower_m,
            to_upper_m,
        }
    }

    /// Vertical distance to the airspace in metres, 0 when between the limits.
    pub fn distance_m(&self) -> f64 {
        self.to_lower_m.max(self.to_upper_m).max(0.0)
    }

    /// Whether the pilot is between the limits.
    pub fn is_inside(&self) -> bool {
        self.distance_m() == 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::airspace::{FOOT_M, Unit};

    fn msl_feet(feet: f64) -> Limit {
        Limit::Height {
            metres: feet * FOOT_M,
            unit: Unit::Feet,
            reference: Reference::Msl,
        }
    }

    fn agl_feet(feet: f64) -> Limit {
        Limit::Height {
            metres: feet * FOOT_M,
            unit: Unit::Feet,
            reference: Reference::Agl,
        }
    }

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn compares_msl_limits_with_the_gps_altitude() {
        // A TMA from 5500 ft (1676.4 m) to FL 100.
        let lower = msl_feet(5500.0);
        let upper = Limit::FlightLevel(100);
        let below = Altitudes {
            msl_m: Some(1600.0),
            pressure_m: Some(1500.0),
            ground_m: None,
        };
        let v = Vertical::new(lower, upper, &below);
        assert!(near(v.to_lower_m, 76.4), "{v:?}");
        assert!(near(v.distance_m(), 76.4));
        assert!(!v.is_inside());

        let inside = Altitudes {
            msl_m: Some(1700.0),
            ..below
        };
        assert!(Vertical::new(lower, upper, &inside).is_inside());
    }

    #[test]
    fn compares_flight_levels_with_the_pressure_altitude() {
        // FL 95 = 2895.6 m pressure altitude; the GPS altitude must not matter.
        let lower = Limit::FlightLevel(95);
        let altitudes = Altitudes {
            msl_m: Some(3100.0),
            pressure_m: Some(2800.0),
            ground_m: None,
        };
        let v = Vertical::new(lower, Limit::Unlimited, &altitudes);
        assert!(near(v.to_lower_m, 95.6), "{v:?}");
        assert_eq!(v.to_upper_m, f64::NEG_INFINITY);

        // Upper limit FL 95, flying above it.
        let v = Vertical::new(
            Limit::Ground,
            lower,
            &Altitudes {
                pressure_m: Some(3000.0),
                ..altitudes
            },
        );
        assert!(near(v.to_upper_m, 104.4), "{v:?}");
        assert!(near(v.distance_m(), 104.4));
    }

    #[test]
    fn compares_agl_limits_with_the_height_above_ground() {
        // An RMZ from GND to 1000 ft AGL (304.8 m) over ground at 700 m.
        let upper = agl_feet(1000.0);
        let mut altitudes = Altitudes {
            msl_m: Some(1100.0),
            pressure_m: None,
            ground_m: Some(700.0),
        };
        let v = Vertical::new(Limit::Ground, upper, &altitudes);
        assert!(near(v.to_upper_m, 95.2), "{v:?}");

        // Lower limit 1000 ft AGL, 200 m above the ground.
        altitudes.msl_m = Some(900.0);
        let v = Vertical::new(upper, Limit::FlightLevel(100), &altitudes);
        assert!(near(v.to_lower_m, 104.8), "{v:?}");
    }

    #[test]
    fn errs_on_the_side_of_a_warning_without_altitudes() {
        // Ground unknown: an AGL lower limit counts as reached, an AGL upper limit as
        // not yet left.
        let altitudes = Altitudes {
            msl_m: Some(3000.0),
            pressure_m: None,
            ground_m: None,
        };
        let v = Vertical::new(agl_feet(1000.0), agl_feet(2000.0), &altitudes);
        assert!(v.is_inside(), "{v:?}");

        // Nothing known at all: inside.
        let v = Vertical::new(
            msl_feet(5000.0),
            Limit::FlightLevel(100),
            &Altitudes::default(),
        );
        assert!(v.is_inside(), "{v:?}");

        // Without barometer the GPS altitude stands in for the pressure altitude.
        let gps_only = Altitudes {
            msl_m: Some(2800.0),
            ..Altitudes::default()
        };
        let v = Vertical::new(Limit::FlightLevel(95), Limit::Unlimited, &gps_only);
        assert!(near(v.to_lower_m, 95.6), "{v:?}");
    }
}
