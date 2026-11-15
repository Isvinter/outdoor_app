//! Application services: the one place where location, recording, storage and
//! GPX come together. User interfaces (the Tauri shell now, native mobile apps
//! later) call [`App`] and render what it reports through [`AppListener`].

mod app;
mod error;
mod plan;
mod regions;
mod routes;

pub use app::{App, AppListener, LinePoint, RecordingPhase, RecordingStatus, TrackDetail};
pub use error::AppError;
pub use plan::{Leg, LegError, LegSpec, PlanRequest, RoutePlan, RoutePoint, Stage, Totals};
pub use regions::{RegionInfo, RegionState, RegionsOverview};
pub use routes::{ProfileInfo, RouterFactory};
