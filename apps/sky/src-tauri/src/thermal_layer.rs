//! Which thermal layer to show: season and time of day, chosen automatically
//! from the date, the place and the sunrise, or fixed by the user.

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

const FILE_NAME: &str = "thermal-map.json";

/// Season layer of the thermal map. Serialized as its lowercase name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Season {
    All,
    Jan,
    Apr,
    Jul,
    Oct,
}

impl Season {
    pub fn code(self) -> &'static str {
        match self {
            Season::All => "all",
            Season::Jan => "jan",
            Season::Apr => "apr",
            Season::Jul => "jul",
            Season::Oct => "oct",
        }
    }
}

/// Time-of-day layer of the thermal map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DayTime {
    All,
    Morning,
    Midday,
    Evening,
}

impl DayTime {
    /// The part of the layer name for this time of day.
    pub fn code(self) -> &'static str {
        match self {
            DayTime::All => "all",
            DayTime::Morning => "04",
            DayTime::Midday => "07",
            DayTime::Evening => "10",
        }
    }
}

/// Either "pick it for me" or one fixed value. On the wire it is the plain string
/// `"auto"` or the value's own name. The manual (de)serialization below reads "auto"
/// first and everything else as the value `T`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice<T> {
    Auto,
    Fixed(T),
}

impl<T: Serialize> Serialize for Choice<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Choice::Auto => serializer.serialize_str("auto"),
            Choice::Fixed(value) => value.serialize(serializer),
        }
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Choice<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Read the text first, then decide: "auto", or hand it to `T`.
        let text = String::deserialize(deserializer)?;
        if text == "auto" {
            return Ok(Choice::Auto);
        }
        // `into_deserializer` lets `T` read the text as if it were the JSON string.
        let value = T::deserialize(serde::de::IntoDeserializer::<D::Error>::into_deserializer(
            text,
        ))?;
        Ok(Choice::Fixed(value))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ThermalSettings {
    pub enabled: bool,
    pub season: Choice<Season>,
    pub day_time: Choice<DayTime>,
    /// Opacity of the thermal layer on the light map, in whole percent.
    pub opacity_light: u8,
    /// The same on the dark map, where the colours stand out more.
    pub opacity_dark: u8,
}

/// Bounds of the opacity slider, in percent.
const OPACITY_MIN: u8 = 10;
const OPACITY_MAX: u8 = 100;

impl Default for ThermalSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            season: Choice::Auto,
            day_time: Choice::Auto,
            opacity_light: 60,
            opacity_dark: 35,
        }
    }
}

impl ThermalSettings {
    /// Anything unreadable gives the default.
    /// Opacities out of range are moved to the nearest bound.
    pub fn parse(text: &str) -> Self {
        let mut settings: Self = serde_json::from_str(text).unwrap_or_default();
        settings.opacity_light = settings.opacity_light.clamp(OPACITY_MIN, OPACITY_MAX);
        settings.opacity_dark = settings.opacity_dark.clamp(OPACITY_MIN, OPACITY_MAX);
        settings
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayerKey {
    pub season: Season,
    pub day_time: DayTime,
}

const SEASONS: [Season; 5] = [
    Season::All,
    Season::Jan,
    Season::Apr,
    Season::Jul,
    Season::Oct,
];
const DAY_TIMES: [DayTime; 4] = [
    DayTime::All,
    DayTime::Morning,
    DayTime::Midday,
    DayTime::Evening,
];

impl LayerKey {
    /// For example `jul_07`.
    pub fn as_string(self) -> String {
        format!("{}_{}", self.season.code(), self.day_time.code())
    }

    /// Only the 20 valid keys; anything else (also path tricks) gives `None`.
    pub fn parse(text: &str) -> Option<LayerKey> {
        SEASONS.iter().find_map(|&season| {
            DAY_TIMES
                .iter()
                .map(|&day_time| LayerKey { season, day_time })
                .find(|key| key.as_string() == text)
        })
    }
}

pub fn auto_season(month: u8) -> Season {
    match month {
        3..=5 => Season::Apr,
        6..=8 => Season::Jul,
        9..=11 => Season::Oct,
        _ => Season::Jan,
    }
}

pub fn auto_day_time(hours_since_sunrise: Option<f64>) -> DayTime {
    match hours_since_sunrise {
        None => DayTime::All,
        Some(h) if h < 5.5 => DayTime::Morning,
        Some(h) if h < 8.5 => DayTime::Midday,
        Some(_) => DayTime::Evening,
    }
}

/// The layer to show now at this place; fixed choices win over automatic ones.
pub fn choose(
    settings: &ThermalSettings,
    unix_now: i64,
    latitude_deg: f64,
    longitude_deg: f64,
) -> LayerKey {
    let date = flight_core::sun::solar_date(unix_now, longitude_deg);
    let season = match settings.season {
        Choice::Fixed(season) => season,
        Choice::Auto => auto_season(date.month()),
    };
    let day_time = match settings.day_time {
        Choice::Fixed(day_time) => day_time,
        Choice::Auto => {
            // Absolute seconds, so a sunrise on the neighbouring UTC day still works.
            let hours = match flight_core::sun::sunrise(date, latitude_deg, longitude_deg) {
                flight_core::sun::Sunrise::At { unix_seconds } => {
                    Some((unix_now - unix_seconds) as f64 / 3600.0)
                }
                flight_core::sun::Sunrise::AlwaysUp | flight_core::sun::Sunrise::AlwaysDown => None,
            };
            auto_day_time(hours)
        }
    };
    LayerKey { season, day_time }
}

/// Other layers to use when this one is missing, best first.
pub fn fallbacks(key: LayerKey) -> Vec<LayerKey> {
    let mut result: Vec<LayerKey> = Vec::new();
    let mut add = |candidate: LayerKey| {
        if candidate != key && !result.contains(&candidate) {
            result.push(candidate);
        }
    };
    // Same season: the other times of day, midday first, then the whole day.
    for day_time in [
        DayTime::Midday,
        DayTime::Morning,
        DayTime::Evening,
        DayTime::All,
    ] {
        add(LayerKey {
            season: key.season,
            day_time,
        });
    }
    // Last resort: the whole year, the whole day.
    add(LayerKey {
        season: Season::All,
        day_time: DayTime::All,
    });
    result
}

/// The saved settings; the default (off, automatic) if none were saved.
pub fn load(app: &AppHandle) -> ThermalSettings {
    let text = crate::settings::config_file(app, FILE_NAME)
        .ok()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .unwrap_or_default();
    ThermalSettings::parse(&text)
}

pub fn save(app: &AppHandle, settings: &ThermalSettings) -> Result<(), String> {
    let text = serde_json::to_string(settings).map_err(|error| error.to_string())?;
    crate::settings::write_file(crate::settings::config_file(app, FILE_NAME)?, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seasons_by_month() {
        use Season::*;
        let got: Vec<_> = (1..=12).map(auto_season).collect();
        assert_eq!(
            got,
            [Jan, Jan, Apr, Apr, Apr, Jul, Jul, Jul, Oct, Oct, Oct, Jan]
        );
    }
    #[test]
    fn day_time_bounds() {
        use DayTime::*;
        assert_eq!(auto_day_time(Some(-1.0)), Morning);
        assert_eq!(auto_day_time(Some(5.49)), Morning);
        assert_eq!(auto_day_time(Some(5.5)), Midday);
        assert_eq!(auto_day_time(Some(8.49)), Midday);
        assert_eq!(auto_day_time(Some(8.5)), Evening);
        assert_eq!(auto_day_time(None), All);
    }
    #[test]
    fn keys_round_trip_and_reject_unknown() {
        let k = LayerKey {
            season: Season::Jul,
            day_time: DayTime::Midday,
        };
        assert_eq!(k.as_string(), "jul_07");
        assert_eq!(LayerKey::parse("jul_07"), Some(k));
        for bad in ["jul_99", "may_07", "jul", "../x", "jul_07/../.."] {
            assert_eq!(LayerKey::parse(bad), None);
        }
    }
    #[test]
    fn fallback_order() {
        let names = |k: &str| {
            fallbacks(LayerKey::parse(k).unwrap())
                .iter()
                .map(|k| k.as_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(names("oct_04"), ["oct_07", "oct_10", "oct_all", "all_all"]);
        assert_eq!(names("oct_10"), ["oct_07", "oct_04", "oct_all", "all_all"]);
        assert_eq!(names("oct_07"), ["oct_04", "oct_10", "oct_all", "all_all"]);
        assert_eq!(names("oct_all"), ["oct_07", "oct_04", "oct_10", "all_all"]);
        assert_eq!(names("all_all"), ["all_07", "all_04", "all_10"]);
    }
    #[test]
    fn brauneck_midday_in_october() {
        // 2026-10-01 12:00 UTC, sunrise ≈ 05:11 UTC → 6.8 h
        let t = flight_core::time::Date::new(2026, 10, 1)
            .unwrap()
            .days_since_unix_epoch()
            * 86_400
            + 12 * 3600;
        assert_eq!(
            choose(&ThermalSettings::default(), t, 47.68, 11.53).as_string(),
            "oct_07"
        );
    }
    #[test]
    fn cusco_evening_picks_evening() {
        // 2026-06-22 00:30 UTC = 19:30 local on 21 June
        let t = flight_core::time::Date::new(2026, 6, 22)
            .unwrap()
            .days_since_unix_epoch()
            * 86_400
            + 1800;
        assert_eq!(
            choose(&ThermalSettings::default(), t, -13.53, -71.97).as_string(),
            "jul_10"
        );
    }
    #[test]
    fn fixed_choice_wins() {
        let s = ThermalSettings::parse(r#"{"enabled":true,"season":"apr","dayTime":"morning"}"#);
        assert!(s.enabled);
        assert_eq!(choose(&s, 0, 47.68, 11.53).as_string(), "apr_04");
    }
    #[test]
    fn settings_default_and_garbage() {
        assert_eq!(
            ThermalSettings::parse("nonsense"),
            ThermalSettings::default()
        );
        assert_eq!(
            serde_json::to_string(&ThermalSettings::default()).unwrap(),
            r#"{"enabled":false,"season":"auto","dayTime":"auto","opacityLight":60,"opacityDark":35}"#
        );
    }

    #[test]
    fn opacity_defaults_for_older_files() {
        // A file written before the slider existed keeps its choices.
        let s = ThermalSettings::parse(r#"{"enabled":true,"season":"apr","dayTime":"auto"}"#);
        assert!(s.enabled);
        assert_eq!((s.opacity_light, s.opacity_dark), (60, 35));
    }

    #[test]
    fn opacity_is_kept_within_bounds() {
        let s = ThermalSettings::parse(r#"{"opacityLight":0,"opacityDark":250}"#);
        assert_eq!((s.opacity_light, s.opacity_dark), (10, 100));
        let s = ThermalSettings::parse(r#"{"opacityLight":45,"opacityDark":20}"#);
        assert_eq!((s.opacity_light, s.opacity_dark), (45, 20));
    }
}
