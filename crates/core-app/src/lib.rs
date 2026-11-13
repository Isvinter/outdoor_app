//! Application services: the one place where location, recording, storage and
//! GPX come together. User interfaces (the Tauri shell now, native mobile apps
//! later) call [`App`] and render what it reports through [`AppListener`].

mod app;
mod error;
mod regions;

pub use app::{App, AppListener, LinePoint, RecordingPhase, RecordingStatus, TrackDetail};
pub use error::AppError;
pub use regions::{RegionInfo, RegionState, RegionsOverview};
