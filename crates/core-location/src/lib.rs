//! Position sources behind a common trait.
//!
//! Sources push fixes to a [`FixSink`]; nothing polls. The same trait is meant
//! to be implemented by the native iOS and Android location services later.

mod gpsd;
mod replay;
mod worker;

use std::sync::Arc;

use core_track::TrackPoint;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use gpsd::GpsdSource;
pub use replay::GpxReplaySource;

/// A position fix as delivered by a source.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Fix {
    pub lat: f64,
    pub lon: f64,
    /// Elevation above mean sea level in metres.
    pub ele: Option<f64>,
    /// UTC time in milliseconds since the Unix epoch.
    pub time_ms: i64,
    /// Horizontal accuracy in metres.
    pub h_accuracy_m: Option<f32>,
    /// Ground speed in metres per second.
    pub speed_mps: Option<f32>,
    /// Course over ground in degrees from true north.
    pub course_deg: Option<f32>,
}

impl Fix {
    pub fn to_track_point(&self) -> TrackPoint {
        TrackPoint {
            lat: self.lat,
            lon: self.lon,
            ele: self.ele,
            time_ms: Some(self.time_ms),
            h_accuracy_m: self.h_accuracy_m,
            speed_mps: self.speed_mps,
        }
    }
}

/// Connection state of a source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", content = "reason", rename_all = "snake_case")]
pub enum SourceStatus {
    /// Trying to reach the device or daemon.
    Connecting,
    /// Connected; fixes will follow as soon as the receiver has one.
    Active,
    /// Connection lost or not possible; the source keeps retrying.
    Lost(String),
    /// The source has no more fixes to deliver (end of a replay).
    Finished,
}

/// Receiver of fixes. Called from the source's own thread.
pub trait FixSink: Send + Sync {
    fn on_fix(&self, fix: Fix);
    fn on_status(&self, status: SourceStatus);
}

/// Settings common to all sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LocationConfig {
    /// Fixes closer together than this are dropped. 0 delivers every fix.
    pub min_interval_ms: u32,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LocationError {
    #[error("the location source is already running")]
    AlreadyRunning,
    #[error("cannot start the location source: {0}")]
    Start(String),
}

/// A source of position fixes.
pub trait LocationSource: Send {
    /// Starts delivering fixes to `sink` until [`LocationSource::stop`] is called.
    fn start(
        &mut self,
        config: LocationConfig,
        sink: Arc<dyn FixSink>,
    ) -> Result<(), LocationError>;

    /// Stops the source. No fix is delivered after this returns.
    fn stop(&mut self);
}
