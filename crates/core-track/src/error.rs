use thiserror::Error;

/// Errors produced by the track model.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TrackError {
    #[error("unknown activity type: {0}")]
    UnknownActivity(String),
}
