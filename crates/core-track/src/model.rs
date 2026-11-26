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
    /// No activity given, or several in one track.
    Other,
}

impl Activity {
    pub const ALL: [Activity; 5] = [
        Activity::Bike,
        Activity::Mtb,
        Activity::Hike,
        Activity::SkiTour,
        Activity::Other,
    ];

    /// Stable identifier used in storage, GPX `<type>` and on the command line.
    pub fn as_str(self) -> &'static str {
        match self {
            Activity::Bike => "bike",
            Activity::Mtb => "mtb",
            Activity::Hike => "hike",
            Activity::SkiTour => "ski_tour",
            Activity::Other => "other",
        }
    }

    /// Default spacing of recorded fixes. Slower activities need fewer fixes,
    /// which saves storage and, on mobile devices, battery. Without an
    /// activity every second, since it may be anything.
    pub fn location_interval_ms(self) -> u32 {
        match self {
            Activity::Bike | Activity::Mtb | Activity::Other => 1000,
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

/// Why a segment began.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentReason {
    /// The first segment of a recording, or of an imported track.
    #[default]
    Start,
    /// The user switched the activity.
    ActivityChange,
    /// The recording was resumed after a pause.
    Resume,
    /// A long time without fixes.
    Gap,
    /// A rest was detected; the segment holds the fixes of the rest.
    Rest,
    /// The user moved on after a rest.
    RestEnd,
    /// The recording was continued from a stored track, e.g. after a crash.
    Continue,
}

impl SegmentReason {
    pub const ALL: [SegmentReason; 7] = [
        SegmentReason::Start,
        SegmentReason::ActivityChange,
        SegmentReason::Resume,
        SegmentReason::Gap,
        SegmentReason::Rest,
        SegmentReason::RestEnd,
        SegmentReason::Continue,
    ];

    /// Stable identifier used in storage.
    pub fn as_str(self) -> &'static str {
        match self {
            SegmentReason::Start => "start",
            SegmentReason::ActivityChange => "activity_change",
            SegmentReason::Resume => "resume",
            SegmentReason::Gap => "gap",
            SegmentReason::Rest => "rest",
            SegmentReason::RestEnd => "rest_end",
            SegmentReason::Continue => "continue",
        }
    }
}

impl FromStr for SegmentReason {
    type Err = TrackError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        SegmentReason::ALL
            .into_iter()
            .find(|r| r.as_str() == s)
            .ok_or_else(|| TrackError::UnknownSegmentReason(s.to_owned()))
    }
}

/// Consecutive fixes without interruption, a section of a tour. A new segment begins
/// with a pause, a restart, a long gap, a change of activity and a detected rest.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Segment {
    pub points: Vec<TrackPoint>,
    /// Activity of this section; `None` means that of the track.
    #[serde(default)]
    pub activity: Option<Activity>,
    #[serde(default)]
    pub reason: SegmentReason,
}

impl Segment {
    /// Whether the segment holds the fixes of a detected rest.
    pub fn is_rest(&self) -> bool {
        self.reason == SegmentReason::Rest
    }
}

/// A recorded or imported track.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Track {
    pub name: Option<String>,
    pub activity: Option<Activity>,
    pub segments: Vec<Segment>,
}

impl Track {
    /// Activity of segment `i`: its own, else that of the track, else [`Activity::Other`].
    pub fn segment_activity(&self, i: usize) -> Activity {
        self.segments
            .get(i)
            .and_then(|s| s.activity)
            .or(self.activity)
            .unwrap_or(Activity::Other)
    }
}
