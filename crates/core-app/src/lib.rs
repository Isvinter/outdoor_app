//! Application services: the one place where location, recording, storage and
//! GPX come together. User interfaces (the Tauri shell now, native mobile apps
//! later) call [`App`] and render what it reports through [`AppListener`].

mod app;
mod error;
mod plan;
mod regions;
mod routes;
mod search;
mod share;

pub use app::{App, AppListener, LinePoint, RecordingPhase, RecordingStatus, TrackDetail};
pub use error::AppError;
pub use plan::{Leg, LegError, LegSpec, PlanRequest, RoutePlan, RoutePoint, Stage, Totals};
pub use regions::{MapOverview, RegionInfo, RegionState, RegionsOverview};
pub use routes::{ProfileInfo, RouterFactory};
pub use search::{SearchHit, SearchResults};
pub use share::{
    ImportOutcome, ImportedRoute, RemoteRoute, RemoteRouteState, SharedRoute, SharedRouteInfo,
};
