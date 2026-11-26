//! User settings.

use thiserror::Error;

/// Everything the user can adjust about the vario.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VarioSettings {
    /// Climb tone starts above this climb rate in m/s.
    pub climb_threshold_mps: f64,
    /// Sink tone starts below this (negative) climb rate in m/s.
    pub sink_threshold_mps: f64,
    /// Tone volume from 0.0 (silent) to 1.0.
    pub volume: f64,
    /// Averaging time of the displayed and audible climb rate in seconds. 0 turns
    /// averaging off; larger values are calmer but react later.
    pub damping_s: f64,
    /// Time window in seconds over which the glide ratio is averaged. One of
    /// [`GLIDE_WINDOWS_S`].
    pub glide_window_s: f64,
    /// Keep the vario and the airspace warning sound silent until a takeoff is detected,
    /// like "Auto Silent" on the Flymaster.
    pub tone_after_takeoff: bool,
    /// Speed over ground that starts a flight where the terrain is unknown, in m/s.
    pub takeoff_speed_mps: f64,
}

/// Allowed takeoff speeds in m/s (about 4 to 54 km/h).
pub const TAKEOFF_SPEED_RANGE_MPS: std::ops::RangeInclusive<f64> = 1.0..=15.0;

/// The glide windows the user can choose from, in seconds.
pub const GLIDE_WINDOWS_S: [f64; 4] = [10.0, 20.0, 30.0, 60.0];

impl Default for VarioSettings {
    fn default() -> Self {
        VarioSettings {
            climb_threshold_mps: 0.2,
            sink_threshold_mps: -2.5,
            volume: 0.8,
            damping_s: 0.5,
            glide_window_s: 20.0,
            tone_after_takeoff: true,
            takeoff_speed_mps: crate::flight_phase::DEFAULT_TAKEOFF_SPEED_MPS,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Error)]
pub enum SettingsError {
    #[error("the sink threshold must be below the climb threshold")]
    ThresholdsOverlap,
    #[error("the volume must be between 0 and 1")]
    VolumeOutOfRange,
    #[error("the damping must be between 0 and 10 seconds")]
    DampingOutOfRange,
    #[error("the glide window must be 10, 20, 30 or 60 seconds")]
    GlideWindowInvalid,
    #[error("the takeoff speed must be between 1 and 15 m/s")]
    TakeoffSpeedInvalid,
    #[error("settings must be finite numbers")]
    NotFinite,
    #[error("line {line} of the settings file is not `name = value`")]
    Malformed { line: usize },
}

impl VarioSettings {
    /// Checks that the values make sense together.
    pub fn validate(&self) -> Result<(), SettingsError> {
        let values = [
            self.climb_threshold_mps,
            self.sink_threshold_mps,
            self.volume,
            self.damping_s,
            self.glide_window_s,
            self.takeoff_speed_mps,
        ];
        if values.iter().any(|value| !value.is_finite()) {
            return Err(SettingsError::NotFinite);
        }
        if self.sink_threshold_mps >= self.climb_threshold_mps {
            return Err(SettingsError::ThresholdsOverlap);
        }
        if !(0.0..=1.0).contains(&self.volume) {
            return Err(SettingsError::VolumeOutOfRange);
        }
        if !(0.0..=10.0).contains(&self.damping_s) {
            return Err(SettingsError::DampingOutOfRange);
        }
        if !GLIDE_WINDOWS_S.contains(&self.glide_window_s) {
            return Err(SettingsError::GlideWindowInvalid);
        }
        if !TAKEOFF_SPEED_RANGE_MPS.contains(&self.takeoff_speed_mps) {
            return Err(SettingsError::TakeoffSpeedInvalid);
        }
        Ok(())
    }
}

// The settings file is plain text, one `name = value` per line. That keeps this crate
// free of a serialization dependency and the file readable and editable by hand.
const KEY_CLIMB: &str = "climb_threshold_mps";
const KEY_SINK: &str = "sink_threshold_mps";
const KEY_VOLUME: &str = "volume";
const KEY_DAMPING: &str = "damping_s";
const KEY_GLIDE_WINDOW: &str = "glide_window_s";
const KEY_TONE_AFTER_TAKEOFF: &str = "tone_after_takeoff";
const KEY_TAKEOFF_SPEED: &str = "takeoff_speed_mps";

impl VarioSettings {
    /// The settings as the text of a settings file. Read it back with
    /// [`VarioSettings::from_text`].
    pub fn to_text(&self) -> String {
        format!(
            "# ZeroGrid Sky settings\n\
             {KEY_CLIMB} = {}\n{KEY_SINK} = {}\n{KEY_VOLUME} = {}\n{KEY_DAMPING} = {}\n\
             {KEY_GLIDE_WINDOW} = {}\n{KEY_TONE_AFTER_TAKEOFF} = {}\n{KEY_TAKEOFF_SPEED} = {}\n",
            self.climb_threshold_mps,
            self.sink_threshold_mps,
            self.volume,
            self.damping_s,
            self.glide_window_s,
            self.tone_after_takeoff,
            self.takeoff_speed_mps
        )
    }

    /// Parses the text of a settings file.
    ///
    /// Values missing from the text keep their defaults and unknown names are ignored,
    /// so files written by older or newer versions of the app still load. The result
    /// is validated.
    pub fn from_text(text: &str) -> Result<Self, SettingsError> {
        let mut settings = VarioSettings::default();
        for (index, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let malformed = SettingsError::Malformed { line: index + 1 };
            let (key, value) = line.split_once('=').ok_or(malformed)?;
            let value = value.trim();
            // `str::parse` picks what to parse into from the type it must return.
            let number = || value.parse::<f64>().map_err(|_| malformed);
            match key.trim() {
                KEY_CLIMB => settings.climb_threshold_mps = number()?,
                KEY_SINK => settings.sink_threshold_mps = number()?,
                KEY_VOLUME => settings.volume = number()?,
                KEY_DAMPING => settings.damping_s = number()?,
                KEY_GLIDE_WINDOW => settings.glide_window_s = number()?,
                KEY_TONE_AFTER_TAKEOFF => {
                    settings.tone_after_takeoff = value.parse().map_err(|_| malformed)?;
                }
                KEY_TAKEOFF_SPEED => settings.takeoff_speed_mps = number()?,
                _ => {}
            }
        }
        settings.validate()?;
        Ok(settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_round_trip_keeps_all_values() {
        let settings = VarioSettings {
            climb_threshold_mps: 0.35,
            sink_threshold_mps: -3.1,
            volume: 0.45,
            damping_s: 1.25,
            glide_window_s: 60.0,
            tone_after_takeoff: false,
            takeoff_speed_mps: 5.5,
        };
        assert_eq!(VarioSettings::from_text(&settings.to_text()), Ok(settings));
    }

    #[test]
    fn missing_and_unknown_entries_fall_back_to_defaults() {
        let parsed = VarioSettings::from_text("volume = 0.3\nfuture_option = 7\n\n# note\n");
        assert_eq!(
            parsed,
            Ok(VarioSettings {
                volume: 0.3,
                ..VarioSettings::default()
            })
        );
        assert_eq!(VarioSettings::from_text(""), Ok(VarioSettings::default()));
    }

    #[test]
    fn broken_text_is_an_error() {
        assert_eq!(
            VarioSettings::from_text("volume = 0.3\nnonsense\n"),
            Err(SettingsError::Malformed { line: 2 })
        );
        assert_eq!(
            VarioSettings::from_text("volume = loud"),
            Err(SettingsError::Malformed { line: 1 })
        );
        assert_eq!(
            VarioSettings::from_text("volume = 2"),
            Err(SettingsError::VolumeOutOfRange)
        );
    }

    #[test]
    fn glide_window_must_be_one_of_the_choices() {
        assert_eq!(VarioSettings::default().glide_window_s, 20.0);
        let odd = VarioSettings {
            glide_window_s: 15.0,
            ..VarioSettings::default()
        };
        assert_eq!(odd.validate(), Err(SettingsError::GlideWindowInvalid));
        // Older files without the entry keep the default.
        assert_eq!(
            VarioSettings::from_text("volume = 0.3")
                .unwrap()
                .glide_window_s,
            20.0
        );
    }

    #[test]
    fn takeoff_settings_have_flymaster_like_defaults() {
        let defaults = VarioSettings::default();
        assert!(defaults.tone_after_takeoff);
        assert!((defaults.takeoff_speed_mps * 3.6 - 15.0).abs() < 1e-9);
        // Older files without the entries keep the defaults.
        let old = VarioSettings::from_text("volume = 0.3\nglide_window_s = 30").unwrap();
        assert!(old.tone_after_takeoff);
        assert_eq!(old.takeoff_speed_mps, defaults.takeoff_speed_mps);
    }

    #[test]
    fn takeoff_settings_are_checked() {
        for speed in [0.0, 0.5, 100.0] {
            assert_eq!(
                VarioSettings::from_text(&format!("takeoff_speed_mps = {speed}")),
                Err(SettingsError::TakeoffSpeedInvalid)
            );
        }
        assert_eq!(
            VarioSettings::from_text("tone_after_takeoff = maybe"),
            Err(SettingsError::Malformed { line: 1 })
        );
        assert!(
            !VarioSettings::from_text("tone_after_takeoff = false")
                .unwrap()
                .tone_after_takeoff
        );
    }

    #[test]
    fn defaults_are_valid() {
        assert_eq!(VarioSettings::default().validate(), Ok(()));
    }

    #[test]
    fn rejects_inconsistent_values() {
        let defaults = VarioSettings::default();
        let overlapping = VarioSettings {
            sink_threshold_mps: 0.5,
            ..defaults
        };
        assert_eq!(
            overlapping.validate(),
            Err(SettingsError::ThresholdsOverlap)
        );
        let loud = VarioSettings {
            volume: 1.5,
            ..defaults
        };
        assert_eq!(loud.validate(), Err(SettingsError::VolumeOutOfRange));
        let slow = VarioSettings {
            damping_s: -1.0,
            ..defaults
        };
        assert_eq!(slow.validate(), Err(SettingsError::DampingOutOfRange));
        let nan = VarioSettings {
            volume: f64::NAN,
            ..defaults
        };
        assert_eq!(nan.validate(), Err(SettingsError::NotFinite));
    }
}
