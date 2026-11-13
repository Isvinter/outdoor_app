use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::TrackError;

/// Kind of outdoor activity. Selects default statistics parameters and, later,
/// the location interval and routing profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Activity {
    Bike,
    Mtb,
    Hike,
    SkiTour,
}

impl Activity {
    pub const ALL: [Activity; 4] = [
        Activity::Bike,
        Activity::Mtb,
        Activity::Hike,
        Activity::SkiTour,
    ];

    /// Stable identifier used in storage, GPX `<type>` and on the command line.
    pub fn as_str(self) -> &'static str {
        match self {
            Activity::Bike => "bike",
            Activity::Mtb => "mtb",
            Activity::Hike => "hike",
            Activity::SkiTour => "ski_tour",
        }
    }

    /// Default spacing of recorded fixes. Slower activities need fewer fixes,
    /// which saves storage and, on mobile devices, battery.
    pub fn location_interval_ms(self) -> u32 {
        match self {
            Activity::Bike | Activity::Mtb => 1000,
            Activity::Hike | Activity::SkiTour => 3000,
        }
    }
}

impl FromStr for Activity {
    type Err = TrackError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Activity::ALL
            .into_iter()
            .find(|a| a.as_str() == s)
            .ok_or_else(|| TrackError::UnknownActivity(s.to_owned()))
    }
}

/// A single position fix.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TrackPoint {
    /// Latitude in degrees (WGS 84).
    pub lat: f64,
    /// Longitude in degrees (WGS 84).
    pub lon: f64,
    /// Elevation in metres above sea level.
    pub ele: Option<f64>,
    /// UTC time in milliseconds since the Unix epoch.
    pub time_ms: Option<i64>,
    /// Horizontal accuracy reported by the receiver, in metres.
    pub h_accuracy_m: Option<f32>,
    /// Ground speed reported by the receiver (Doppler), in metres per second.
    pub speed_mps: Option<f32>,
}

impl TrackPoint {
    pub fn new(lat: f64, lon: f64) -> Self {
        Self {
            lat,
            lon,
            ele: None,
            time_ms: None,
            h_accuracy_m: None,
            speed_mps: None,
        }
    }
}

/// Consecutive fixes without interruption. A pause or a restart begins a new segment.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Segment {
    pub points: Vec<TrackPoint>,
}

/// A recorded or imported track.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Track {
    pub name: Option<String>,
    pub activity: Option<Activity>,
    pub segments: Vec<Segment>,
}
