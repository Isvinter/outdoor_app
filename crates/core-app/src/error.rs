use thiserror::Error;

/// Errors of application services.
#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Storage(#[from] core_storage::StorageError),
    #[error(transparent)]
    Gpx(#[from] core_gpx::GpxError),
    #[error(transparent)]
    Track(#[from] core_track::TrackError),
    #[error(transparent)]
    Location(#[from] core_location::LocationError),
    #[error("no recording is in progress")]
    NoRecording,
    #[error("a recording is already in progress")]
    RecordingActive,
    #[error("there is no interrupted recording")]
    NothingInterrupted,
    #[error("the file does not say which activity the track is; choose one")]
    ActivityRequired,
}

impl AppError {
    /// Stable identifier, used by user interfaces to pick a translated message.
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Storage(_) => "storage",
            AppError::Gpx(_) => "gpx",
            AppError::Track(_) => "track",
            AppError::Location(_) => "location",
            AppError::NoRecording => "no_recording",
            AppError::RecordingActive => "recording_active",
            AppError::NothingInterrupted => "nothing_interrupted",
            AppError::ActivityRequired => "activity_required",
        }
    }
}
