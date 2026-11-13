use std::path::{Path, PathBuf};

use thiserror::Error;

/// Errors of map region handling.
#[derive(Debug, Error)]
pub enum MapsError {
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{} is not a usable tile archive: {reason}", path.display())]
    InvalidArchive { path: PathBuf, reason: String },
    #[error("invalid region description {location}: {reason}")]
    InvalidManifest { location: String, reason: String },
    #[error("download of {location} failed: {reason}")]
    Download { location: String, reason: String },
    #[error("{file} of region {region} is damaged (checksum mismatch)")]
    Checksum { region: String, file: String },
    #[error("elevation data cannot be read: {0}")]
    Elevation(String),
    #[error("region {0} is not in the catalog")]
    UnknownRegion(String),
    #[error("region {0} is not installed")]
    NotInstalled(String),
    #[error("region {0} is already being downloaded")]
    Busy(String),
    #[error("the download was cancelled")]
    Cancelled,
}

impl MapsError {
    /// Stable identifier, used by user interfaces to pick a translated message.
    pub fn code(&self) -> &'static str {
        match self {
            MapsError::Io { .. } => "region_io",
            MapsError::InvalidArchive { .. } => "region_archive",
            MapsError::InvalidManifest { .. } => "region_manifest",
            MapsError::Download { .. } => "region_download",
            MapsError::Checksum { .. } => "region_checksum",
            MapsError::Elevation(_) => "region_elevation",
            MapsError::UnknownRegion(_) => "region_unknown",
            MapsError::NotInstalled(_) => "region_not_installed",
            MapsError::Busy(_) => "region_busy",
            MapsError::Cancelled => "region_cancelled",
        }
    }

    pub(crate) fn io(path: &Path) -> impl FnOnce(std::io::Error) -> MapsError {
        let path = path.to_path_buf();
        move |source| MapsError::Io { path, source }
    }
}
