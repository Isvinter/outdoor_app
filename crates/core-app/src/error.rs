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
    #[error(transparent)]
    Maps(#[from] core_maps::MapsError),
    #[error(transparent)]
    Routing(#[from] core_routing::RoutingError),
    #[error("stored route cannot be read: {0}")]
    RoutePlan(String),
    #[error("route cannot be taken over: {0}")]
    RouteImport(String),
    #[error("no route is planned")]
    NoRoutePlanned,
    #[error("no map catalog is configured")]
    NoCatalog,
    #[error("no recording is in progress")]
    NoRecording,
    #[error("a recording is already in progress")]
    RecordingActive,
    #[error("there is no interrupted recording")]
    NothingInterrupted,
    #[error("the file does not say which activity the track is; choose one")]
    ActivityRequired,
    #[error("no such section, or a rest, whose activity cannot be set")]
    InvalidSection,
    #[error("a photo or note needs a fitting kind, source, file and text")]
    InvalidMedia,
    #[error("media file: {0}")]
    MediaFile(String),
    #[error("not enough free space for the package")]
    ExportNoSpace,
    #[error("the package cannot be written: {0}")]
    Export(String),
    #[error("unknown search category: {0}")]
    SearchCategory(String),
}

impl AppError {
    /// Stable identifier, used by user interfaces to pick a translated message.
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Storage(_) => "storage",
            AppError::Gpx(_) => "gpx",
            AppError::Track(_) => "track",
            AppError::Location(_) => "location",
            AppError::Maps(e) => e.code(),
            AppError::Routing(e) => match e {
                core_routing::RoutingError::TooFewWaypoints => "route_too_few_waypoints",
                core_routing::RoutingError::NoRoute(_) => "route_not_found",
                core_routing::RoutingError::Unavailable(_) => "routing_unavailable",
                core_routing::RoutingError::Incompatible { .. } => "routing_incompatible",
                _ => "routing",
            },
            AppError::RoutePlan(_) => "route_plan",
            AppError::RouteImport(_) => "route_import",
            AppError::NoRoutePlanned => "no_route_planned",
            AppError::NoCatalog => "no_catalog",
            AppError::NoRecording => "no_recording",
            AppError::RecordingActive => "recording_active",
            AppError::NothingInterrupted => "nothing_interrupted",
            AppError::ActivityRequired => "activity_required",
            AppError::InvalidSection => "invalid_section",
            AppError::InvalidMedia => "invalid_media",
            AppError::MediaFile(_) => "media_file",
            AppError::ExportNoSpace => "export_no_space",
            AppError::Export(_) => "export_failed",
            AppError::SearchCategory(_) => "search_category",
        }
    }
}
