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
//!
//! [`Warner`] turns these comparisons into warnings in two levels, once per GPS fix:
//!
//! * [`Level::Near`]: closer than the set distances, or on course to fly in within the
//!   look-ahead time. The prediction follows a straight line and is skipped while
//!   circling, where only the distances count.
//! * [`Level::Alarm`]: inside, or almost (200 m horizontally and 30 m vertically).
//!
//! A warning ends only with some margin beyond the distance that started it, so it does
//! not flicker at the border.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use thiserror::Error;

use crate::airspace::{Airspace, Bounds, Category, Limit, Reference};
use crate::geo;

/// Horizontal distance for an alarm in metres.
const ALARM_HORIZONTAL_M: f64 = 200.0;
/// Vertical distance for an alarm in metres.
const ALARM_VERTICAL_M: f64 = 30.0;
/// An active warning ends only this many times beyond its distances.
const HYSTERESIS: f64 = 1.2;
/// Below this ground speed the course says little; no horizontal prediction.
const MIN_PREDICTION_SPEED_MPS: f64 = 3.0;
/// Above this smoothed turn rate the pilot is circling; no horizontal prediction.
const CIRCLING_DEG_S: f64 = 5.0;
/// Share of each new turn rate in the smoothed value.
const TURN_SMOOTHING: f64 = 0.4;
/// Time step of the prediction in seconds.
const PREDICTION_STEP_S: f64 = 2.0;

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

/// What the user can set about airspace warnings.
#[derive(Debug, Clone, PartialEq)]
pub struct WarningSettings {
    pub enabled: bool,
    /// Kinds of airspace that warn.
    pub categories: Vec<Category>,
    /// Horizontal distance for a warning in metres.
    pub horizontal_m: f64,
    /// Vertical distance for a warning in metres.
    pub vertical_m: f64,
    /// Warn this many seconds before flying in on the current course and climb.
    pub lookahead_s: f64,
    /// Volume of the warning tone from 0.0 to 1.0.
    pub volume: f64,
}

impl Default for WarningSettings {
    fn default() -> Self {
        WarningSettings {
            enabled: true,
            // A paraglider without radio or transponder may enter none of these;
            // class E, glider sectors and TRA/TSA (in Austria mostly areas *for*
            // paragliders) do not warn.
            categories: vec![
                Category::Ctr,
                Category::Controlled,
                Category::Restricted,
                Category::Danger,
                Category::Prohibited,
                Category::Tmz,
                Category::Rmz,
            ],
            horizontal_m: 1000.0,
            vertical_m: 100.0,
            lookahead_s: 30.0,
            volume: 0.8,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum WarningSettingsError {
    #[error("line {line} of the warning settings is not `name = value`")]
    Malformed { line: usize },
    #[error("{0} is out of range")]
    OutOfRange(&'static str),
}

// Plain text, one `name = value` per line, like the vario settings.
const KEY_ENABLED: &str = "enabled";
const KEY_CATEGORIES: &str = "categories";
const KEY_HORIZONTAL: &str = "horizontal_m";
const KEY_VERTICAL: &str = "vertical_m";
const KEY_LOOKAHEAD: &str = "lookahead_s";
const KEY_VOLUME: &str = "volume";

impl WarningSettings {
    /// Checks that the values are usable.
    pub fn validate(&self) -> Result<(), WarningSettingsError> {
        let ranges = [
            (self.horizontal_m, 0.0, 10_000.0, KEY_HORIZONTAL),
            (self.vertical_m, 0.0, 1_000.0, KEY_VERTICAL),
            (self.lookahead_s, 0.0, 120.0, KEY_LOOKAHEAD),
            (self.volume, 0.0, 1.0, KEY_VOLUME),
        ];
        for (value, min, max, key) in ranges {
            // `contains` is false for NaN as well.
            if !(min..=max).contains(&value) {
                return Err(WarningSettingsError::OutOfRange(key));
            }
        }
        Ok(())
    }

    /// The settings as text; read it back with [`WarningSettings::from_text`].
    pub fn to_text(&self) -> String {
        let categories: Vec<&str> = self.categories.iter().map(|c| c.name()).collect();
        format!(
            "# ZeroGrid Sky airspace warnings\n\
             {KEY_ENABLED} = {}\n{KEY_CATEGORIES} = {}\n{KEY_HORIZONTAL} = {}\n\
             {KEY_VERTICAL} = {}\n{KEY_LOOKAHEAD} = {}\n{KEY_VOLUME} = {}\n",
            self.enabled,
            categories.join(", "),
            self.horizontal_m,
            self.vertical_m,
            self.lookahead_s,
            self.volume
        )
    }

    /// Parses settings text. Missing values keep their defaults, unknown names and
    /// unknown categories are ignored.
    pub fn from_text(text: &str) -> Result<Self, WarningSettingsError> {
        let mut settings = WarningSettings::default();
        for (index, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let malformed = WarningSettingsError::Malformed { line: index + 1 };
            let (key, value) = line.split_once('=').ok_or(malformed.clone())?;
            let value = value.trim();
            let number = || value.parse::<f64>().map_err(|_| malformed.clone());
            match key.trim() {
                KEY_ENABLED => settings.enabled = value.parse().map_err(|_| malformed.clone())?,
                KEY_CATEGORIES => {
                    settings.categories = Category::ALL
                        .into_iter()
                        .filter(|c| value.split(',').any(|name| name.trim() == c.name()))
                        .collect();
                }
                KEY_HORIZONTAL => settings.horizontal_m = number()?,
                KEY_VERTICAL => settings.vertical_m = number()?,
                KEY_LOOKAHEAD => settings.lookahead_s = number()?,
                KEY_VOLUME => settings.volume = number()?,
                _ => {}
            }
        }
        settings.validate()?;
        Ok(settings)
    }
}

/// Position and motion of the pilot at one GPS fix.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fix {
    /// Time of the fix in seconds, on any steadily running clock.
    pub time_s: f64,
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    pub altitudes: Altitudes,
    pub ground_speed_mps: Option<f64>,
    /// Course over ground, 0 = north, 90 = east.
    pub course_deg: Option<f64>,
    /// Climb rate from the vario, positive up.
    pub climb_mps: Option<f64>,
}

/// How urgent a warning is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    /// Close to the airspace or on course to fly in soon.
    Near,
    /// Inside the airspace or right at its border.
    Alarm,
}

/// One airspace that warns.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Warning {
    /// Index into the airspaces given to the [`Warner`].
    pub index: usize,
    pub level: Level,
    /// Horizontal distance to the outline in metres, negative inside.
    pub horizontal_m: f64,
    pub vertical: Vertical,
    /// Seconds until flying in on the current course and climb; 0 when inside now.
    pub entry_in_s: Option<f64>,
    /// The user has silenced this warning (only near warnings can be silenced).
    pub acknowledged: bool,
}

/// Keeps track of airspace warnings from fix to fix.
pub struct Warner {
    /// Shared with the app, which reads the same list for the map. `Arc` is a pointer
    /// with a reference count: cloning it does not copy the airspaces.
    airspaces: Arc<Vec<Airspace>>,
    /// Bounding box of each airspace, same order.
    bounds: Vec<Bounds>,
    settings: WarningSettings,
    /// Level of each warning at the last fix, by airspace index.
    active: HashMap<usize, Level>,
    /// Near warnings the user has silenced.
    acknowledged: HashSet<usize>,
    /// Time and course of the last fix that had a usable course.
    last_course: Option<(f64, f64)>,
    /// Smoothed turn rate in degrees per second.
    turn_rate_deg_s: f64,
}

impl Warner {
    pub fn new(airspaces: Arc<Vec<Airspace>>, settings: WarningSettings) -> Self {
        let bounds = airspaces.iter().map(Airspace::bounds).collect();
        Warner {
            airspaces,
            bounds,
            settings,
            active: HashMap::new(),
            acknowledged: HashSet::new(),
            last_course: None,
            turn_rate_deg_s: 0.0,
        }
    }

    /// The airspaces the warnings refer to.
    pub fn airspaces(&self) -> &Arc<Vec<Airspace>> {
        &self.airspaces
    }

    pub fn settings(&self) -> &WarningSettings {
        &self.settings
    }

    pub fn set_settings(&mut self, settings: WarningSettings) {
        self.settings = settings;
    }

    /// Whether the pilot seems to be circling (then the course is not used to predict).
    pub fn is_circling(&self) -> bool {
        self.turn_rate_deg_s > CIRCLING_DEG_S
    }

    /// Silences a near warning until it ends. Alarms cannot be silenced; returns
    /// whether the warning was silenced.
    pub fn acknowledge(&mut self, index: usize) -> bool {
        if self.active.get(&index) == Some(&Level::Near) {
            self.acknowledged.insert(index);
            true
        } else {
            false
        }
    }

    /// Evaluates a new fix. Returns the current warnings, alarms first, then nearest
    /// first.
    pub fn update(&mut self, fix: &Fix) -> Vec<Warning> {
        self.track_turning(fix);
        if !self.settings.enabled {
            self.active.clear();
            self.acknowledged.clear();
            return Vec::new();
        }

        let motion = match (fix.ground_speed_mps, fix.course_deg) {
            (Some(speed), Some(course))
                if speed >= MIN_PREDICTION_SPEED_MPS && !self.is_circling() =>
            {
                Some((speed, course))
            }
            _ => None,
        };
        let lookahead_s = self.settings.lookahead_s;
        // Anything farther away than this cannot warn.
        let reach_m = self.settings.horizontal_m.max(ALARM_HORIZONTAL_M) * HYSTERESIS
            + motion.map_or(0.0, |(speed, _)| speed * lookahead_s);

        let mut warnings = Vec::new();
        for (index, airspace) in self.airspaces.iter().enumerate() {
            if !self.settings.categories.contains(&airspace.category())
                || self.bounds[index].distance_m(fix.latitude_deg, fix.longitude_deg) > reach_m
            {
                continue;
            }
            let previous = self.active.get(&index).copied();
            // Distances grow by the hysteresis while a warning of that level is on.
            let factor = |level| {
                if previous >= Some(level) {
                    HYSTERESIS
                } else {
                    1.0
                }
            };
            let horizontal_m = airspace.signed_distance_m(fix.latitude_deg, fix.longitude_deg);
            let vertical = Vertical::new(airspace.lower, airspace.upper, &fix.altitudes);
            let vertical_m = vertical.distance_m();

            let inside = horizontal_m <= 0.0 && vertical.is_inside();
            let entry_in_s = if inside {
                Some(0.0)
            } else {
                predict_entry(airspace, fix, motion, lookahead_s)
            };
            let alarm = horizontal_m <= ALARM_HORIZONTAL_M * factor(Level::Alarm)
                && vertical_m <= ALARM_VERTICAL_M * factor(Level::Alarm);
            let near = horizontal_m <= self.settings.horizontal_m * factor(Level::Near)
                && vertical_m <= self.settings.vertical_m * factor(Level::Near);
            let level = if alarm {
                Level::Alarm
            } else if near || entry_in_s.is_some() {
                Level::Near
            } else {
                continue;
            };
            warnings.push(Warning {
                index,
                level,
                horizontal_m,
                vertical,
                entry_in_s,
                acknowledged: false,
            });
        }

        self.active = warnings.iter().map(|w| (w.index, w.level)).collect();
        // A silenced warning comes back once it ended or turned into an alarm.
        let active = &self.active;
        self.acknowledged
            .retain(|index| active.get(index) == Some(&Level::Near));
        for warning in &mut warnings {
            warning.acknowledged = self.acknowledged.contains(&warning.index);
        }
        warnings.sort_by(|a, b| {
            b.level
                .cmp(&a.level)
                .then(a.horizontal_m.total_cmp(&b.horizontal_m))
        });
        warnings
    }

    /// Updates the smoothed turn rate from the change of course since the last fix.
    fn track_turning(&mut self, fix: &Fix) {
        let course = match (fix.ground_speed_mps, fix.course_deg) {
            (Some(speed), Some(course)) if speed >= MIN_PREDICTION_SPEED_MPS => course,
            _ => {
                // Too slow for a meaningful course: forget it, but keep the turn rate,
                // a short slow phase inside a thermal is still circling.
                self.last_course = None;
                return;
            }
        };
        if let Some((last_time, last_course)) = self.last_course {
            let dt = fix.time_s - last_time;
            if dt > 0.0 && dt <= 10.0 {
                // Smallest angle between both courses, 0..180.
                let change = ((course - last_course + 540.0) % 360.0 - 180.0).abs();
                let rate = change / dt;
                self.turn_rate_deg_s += TURN_SMOOTHING * (rate - self.turn_rate_deg_s);
            }
        }
        self.last_course = Some((fix.time_s, course));
    }
}

/// Seconds until the pilot would be inside the airspace when flying on straight (or,
/// without `motion`, staying in place) with the current climb rate, if that happens
/// within `lookahead_s`.
fn predict_entry(
    airspace: &Airspace,
    fix: &Fix,
    motion: Option<(f64, f64)>,
    lookahead_s: f64,
) -> Option<f64> {
    let climb = fix.climb_mps.unwrap_or(0.0);
    if motion.is_none() && climb.abs() < 0.1 {
        return None;
    }
    let steps = (lookahead_s / PREDICTION_STEP_S).floor() as u32;
    (1..=steps)
        .map(|step| f64::from(step) * PREDICTION_STEP_S)
        .find(|&t| {
            let (lat, lon) = match motion {
                Some((speed, course)) => {
                    geo::destination(fix.latitude_deg, fix.longitude_deg, course, speed * t)
                }
                None => (fix.latitude_deg, fix.longitude_deg),
            };
            let altitudes = Altitudes {
                msl_m: fix.altitudes.msl_m.map(|alt| alt + climb * t),
                pressure_m: fix.altitudes.pressure_m.map(|alt| alt + climb * t),
                ground_m: fix.altitudes.ground_m,
            };
            airspace.contains(lat, lon)
                && Vertical::new(airspace.lower, airspace.upper, &altitudes).is_inside()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::airspace::{self, FOOT_M, Unit};

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

    /// A square from 47.0 to 47.2 N and 11.0 to 11.2 E.
    fn square(class: &str, lower: &str, upper: &str) -> Arc<Vec<Airspace>> {
        let text = format!(
            "AC {class}\nAN Square\nAL {lower}\nAH {upper}\n\
             DP 47:00:00 N 011:00:00 E\nDP 47:00:00 N 011:12:00 E\n\
             DP 47:12:00 N 011:12:00 E\nDP 47:12:00 N 011:00:00 E\n"
        );
        let file = airspace::parse(text.as_bytes());
        assert!(file.issues.is_empty(), "{:?}", file.issues);
        Arc::new(file.airspaces)
    }

    /// A fix `distance_m` south of the square's southern edge, at 1000 m MSL.
    fn south_of_square(time_s: f64, distance_m: f64) -> Fix {
        let (lat, lon) = geo::destination(47.0, 11.1, 180.0, distance_m);
        Fix {
            time_s,
            latitude_deg: lat,
            longitude_deg: lon,
            altitudes: Altitudes {
                msl_m: Some(1000.0),
                pressure_m: Some(1000.0),
                ground_m: None,
            },
            ground_speed_mps: None,
            course_deg: None,
            climb_mps: None,
        }
    }

    fn heading_north(fix: Fix, speed_mps: f64) -> Fix {
        Fix {
            ground_speed_mps: Some(speed_mps),
            course_deg: Some(0.0),
            ..fix
        }
    }

    /// Flies north towards the square from 3 km away, one fix per second. Returns the
    /// distance to the edge when the first near warning and the first alarm came.
    fn approach(warner: &mut Warner, speed_mps: f64) -> (f64, f64) {
        let (mut first_near, mut first_alarm) = (None, None);
        for second in 0..400 {
            let distance = 3000.0 - speed_mps * f64::from(second);
            let fix = heading_north(south_of_square(f64::from(second), distance), speed_mps);
            let warnings = warner.update(&fix);
            match warnings.first().map(|w| w.level) {
                Some(Level::Near) => {
                    first_near.get_or_insert(distance);
                }
                Some(Level::Alarm) => {
                    first_alarm.get_or_insert(distance);
                }
                None => assert!(first_near.is_none(), "warning ended at {distance} m"),
            }
        }
        (first_near.unwrap(), first_alarm.unwrap())
    }

    #[test]
    fn warns_when_approaching_and_alarms_at_the_border() {
        let mut warner = Warner::new(
            square("CTR", "GND", "3500ft MSL"),
            WarningSettings::default(),
        );
        let (near_at, alarm_at) = approach(&mut warner, 10.0);
        assert!((990.0..=1000.0).contains(&near_at), "{near_at}");
        assert!((190.0..=200.0).contains(&alarm_at), "{alarm_at}");
    }

    #[test]
    fn predicts_the_entry_from_course_and_speed() {
        // With a small distance setting the prediction comes first: at 15 m/s the
        // border is 30 s away at 450 m.
        let settings = WarningSettings {
            horizontal_m: 100.0,
            ..WarningSettings::default()
        };
        let mut warner = Warner::new(square("CTR", "GND", "3500ft MSL"), settings);
        let (near_at, _) = approach(&mut warner, 15.0);
        assert!((435.0..=450.0).contains(&near_at), "{near_at}");

        let fix = heading_north(south_of_square(0.0, 300.0), 15.0);
        let warning = warner.update(&fix)[0];
        let entry = warning.entry_in_s.unwrap();
        assert!((20.0..=22.0).contains(&entry), "{entry}");
    }

    #[test]
    fn does_not_predict_while_circling() {
        let settings = WarningSettings {
            horizontal_m: 100.0,
            ..WarningSettings::default()
        };
        // Flying straight at 250 m: the border is 25 s away.
        let mut warner = Warner::new(square("CTR", "GND", "3500ft MSL"), settings.clone());
        let fix = heading_north(south_of_square(0.0, 250.0), 10.0);
        assert_eq!(warner.update(&fix).len(), 1);

        // Circling at the same place, one turn in 18 s.
        let mut warner = Warner::new(square("CTR", "GND", "3500ft MSL"), settings);
        let mut warned_late = false;
        for second in 0..60 {
            let fix = Fix {
                course_deg: Some(f64::from(second * 20 % 360)),
                ..heading_north(south_of_square(f64::from(second), 250.0), 10.0)
            };
            let warnings = warner.update(&fix);
            if second >= 5 {
                warned_late |= !warnings.is_empty();
            }
        }
        assert!(warner.is_circling());
        assert!(!warned_late);
    }

    #[test]
    fn warns_when_climbing_towards_a_flight_level_base() {
        // Inside the outline, circling under a TMA from FL 95 (2895.6 m).
        let mut warner = Warner::new(square("C", "FL95", "FL195"), WarningSettings::default());
        let mut levels = Vec::new();
        for second in 0..120 {
            let altitude = 2700.0 + 2.0 * f64::from(second);
            let mut fix = south_of_square(f64::from(second), -5000.0);
            fix.altitudes.pressure_m = Some(altitude);
            fix.altitudes.msl_m = Some(altitude + 150.0);
            fix.climb_mps = Some(2.0);
            let level = warner.update(&fix).first().map(|w| w.level);
            levels.push((altitude, level));
        }
        let first = |wanted| {
            levels
                .iter()
                .find(|(_, level)| *level == Some(wanted))
                .unwrap()
                .0
        };
        // Near 100 m below the base, alarm 30 m below. The GPS altitude must not matter.
        assert_eq!(first(Level::Near), 2796.0);
        assert_eq!(first(Level::Alarm), 2866.0);
    }

    #[test]
    fn uses_the_ground_for_agl_limits() {
        let airspaces = square("RMZ", "GND", "1000ft AGL");
        let mut warner = Warner::new(airspaces, WarningSettings::default());
        let mut fix = south_of_square(0.0, -5000.0);
        fix.altitudes.msl_m = Some(1100.0);
        // 400 m above ground at 700 m: 95 m above the upper limit.
        fix.altitudes.ground_m = Some(700.0);
        assert_eq!(warner.update(&fix)[0].level, Level::Near);
        // 500 m above ground at 600 m: far enough above.
        fix.altitudes.ground_m = Some(600.0);
        assert!(warner.update(&fix).is_empty());
        // Ground unknown: counts as inside.
        fix.altitudes.ground_m = None;
        assert_eq!(warner.update(&fix)[0].level, Level::Alarm);
    }

    #[test]
    fn warnings_end_with_a_margin() {
        let mut warner = Warner::new(
            square("CTR", "GND", "3500ft MSL"),
            WarningSettings::default(),
        );
        let mut level_at = |distance| {
            let fix = south_of_square(0.0, distance);
            warner.update(&fix).first().map(|w| w.level)
        };
        assert_eq!(level_at(990.0), Some(Level::Near));
        assert_eq!(level_at(1150.0), Some(Level::Near));
        assert_eq!(level_at(1250.0), None);
        assert_eq!(level_at(1100.0), None);
        assert_eq!(level_at(150.0), Some(Level::Alarm));
        assert_eq!(level_at(230.0), Some(Level::Alarm));
        assert_eq!(level_at(250.0), Some(Level::Near));
    }

    #[test]
    fn only_chosen_categories_warn() {
        let fix = south_of_square(0.0, -1000.0);
        let mut warner = Warner::new(square("E", "GND", "FL100"), WarningSettings::default());
        assert!(warner.update(&fix).is_empty());
        let settings = WarningSettings {
            categories: vec![Category::ClassE],
            ..WarningSettings::default()
        };
        warner.set_settings(settings);
        assert_eq!(warner.update(&fix)[0].level, Level::Alarm);
        warner.set_settings(WarningSettings {
            enabled: false,
            ..warner.settings().clone()
        });
        assert!(warner.update(&fix).is_empty());
    }

    #[test]
    fn near_warnings_can_be_silenced_until_they_end() {
        let mut warner = Warner::new(
            square("CTR", "GND", "3500ft MSL"),
            WarningSettings::default(),
        );
        let near = south_of_square(0.0, 800.0);
        let close = south_of_square(1.0, 100.0);
        assert!(!warner.acknowledge(0), "nothing to silence yet");
        assert!(!warner.update(&near)[0].acknowledged);
        assert!(warner.acknowledge(0));
        assert!(warner.update(&near)[0].acknowledged);
        // An alarm sounds again and cannot be silenced.
        let alarm = warner.update(&close)[0];
        assert_eq!(alarm.level, Level::Alarm);
        assert!(!alarm.acknowledged);
        assert!(!warner.acknowledge(0));
        // Back out: a new near warning, not silenced.
        assert!(!warner.update(&near)[0].acknowledged);
    }

    #[test]
    fn warning_settings_survive_a_round_trip() {
        let settings = WarningSettings {
            enabled: false,
            categories: vec![Category::Ctr, Category::ClassE, Category::Reserved],
            horizontal_m: 750.0,
            vertical_m: 80.0,
            lookahead_s: 45.0,
            volume: 0.5,
        };
        assert_eq!(
            WarningSettings::from_text(&settings.to_text()),
            Ok(settings)
        );
        assert_eq!(
            WarningSettings::from_text(""),
            Ok(WarningSettings::default())
        );
        let parsed = WarningSettings::from_text("categories = rmz, nonsense ,ctr\n").unwrap();
        assert_eq!(parsed.categories, [Category::Ctr, Category::Rmz]);
        assert_eq!(
            WarningSettings::from_text("volume = 2"),
            Err(WarningSettingsError::OutOfRange("volume"))
        );
        assert_eq!(
            WarningSettings::from_text("enabled = maybe"),
            Err(WarningSettingsError::Malformed { line: 1 })
        );
    }
}
