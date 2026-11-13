//! Track model, GPS filtering and statistics.
//!
//! This crate is platform independent: no I/O, no clock, no UI. Statistics are a
//! pure function of the recorded points and a set of [`StatsParams`].

mod error;
mod filter;
pub mod geo;
mod model;
mod params;
mod recorder;
mod stats;

pub use error::TrackError;
pub use filter::filter_segment;
pub use model::{Activity, Segment, Track, TrackPoint};
pub use params::StatsParams;
pub use recorder::{Recorder, RecorderState};
pub use stats::{TrackStats, compute_stats, elevation_gain};
