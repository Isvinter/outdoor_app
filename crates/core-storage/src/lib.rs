//! SQLite persistence for tracks and settings.
//!
//! The schema is prepared for synchronisation: UUID primary keys, creation and
//! modification timestamps, soft deletes and a per-row revision counter.
//! The caller supplies the current time, so the crate needs no clock.

mod error;
mod migrations;
mod storage;

pub use error::StorageError;
pub use storage::{RouteRecord, RouteSync, Storage, SyncedRoute, TrackRecord, TrackState};
