//! GPX 1.1 import and export.
//!
//! Works on strings only; reading and writing files is left to the caller so the
//! crate stays free of platform specifics.

mod error;
mod read;
mod write;

use core_track::Track;

pub use error::GpxError;
pub use read::parse_gpx;
pub use write::write_gpx;

/// Content of a GPX file as far as this application uses it: its tracks and,
/// on export, waypoints. Waypoints and routes are ignored on import.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GpxDocument {
    /// `<metadata><name>`
    pub name: Option<String>,
    /// `<wpt>`; written only.
    pub waypoints: Vec<GpxWaypoint>,
    pub tracks: Vec<Track>,
}

/// A named point (`<wpt>`), such as the end of a stage.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GpxWaypoint {
    pub lat: f64,
    pub lon: f64,
    pub ele: Option<f64>,
    pub name: Option<String>,
}
