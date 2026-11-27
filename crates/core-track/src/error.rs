use thiserror::Error;

/// Errors produced by the track model.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TrackError {
    #[error("unknown activity type: {0}")]
    UnknownActivity(String),
    #[error("unknown segment reason: {0}")]
    UnknownSegmentReason(String),
    #[error("cannot {action} a recording that is {state}")]
    InvalidTransition {
        state: &'static str,
        action: &'static str,
    },
}
