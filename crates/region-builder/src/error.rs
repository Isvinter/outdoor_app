use std::path::{Path, PathBuf};

use thiserror::Error;

/// Errors of building a region.
#[derive(Debug, Error)]
pub enum BuildError {
    #[error("building regions is not possible here, missing: {}", .0.join(", "))]
    Prerequisites(Vec<String>),
    #[error("the chosen area cannot be built: {0}")]
    InvalidArea(String),
    #[error("no OpenStreetMap extract covers the chosen area")]
    NoExtract,
    #[error("the list of OpenStreetMap extracts cannot be read: {0}")]
    Index(String),
    #[error("the build failed, see {}:\n{tail}", log.display())]
    Failed { tail: String, log: PathBuf },
    #[error("the build was cancelled")]
    Cancelled,
    #[error("another region is being built")]
    Busy,
    #[error("there is no definition of region {0}")]
    UnknownRegion(String),
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl BuildError {
    /// Stable identifier, used by user interfaces to pick a translated message.
    pub fn code(&self) -> &'static str {
        match self {
            BuildError::Prerequisites(_) => "build_prerequisites",
            BuildError::InvalidArea(_) => "build_area",
            BuildError::NoExtract => "build_no_extract",
            BuildError::Index(_) => "build_index",
            BuildError::Failed { .. } => "build_failed",
            BuildError::Cancelled => "build_cancelled",
            BuildError::Busy => "build_busy",
            BuildError::UnknownRegion(_) => "build_unknown_region",
            BuildError::Io { .. } => "build_io",
        }
    }

    pub(crate) fn io(path: &Path) -> impl FnOnce(std::io::Error) -> BuildError {
        let path = path.to_path_buf();
        move |source| BuildError::Io { path, source }
    }
}
