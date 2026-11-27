//! Application services: the one place where location, recording, storage and
//! GPX come together. User interfaces (the Tauri shell now, native mobile apps
//! later) call [`App`] and render what it reports through [`AppListener`].

mod app;
mod error;
mod guide;
mod plan;
mod regions;
mod routes;
mod search;
mod share;

pub use app::{
    App, AppListener, LinePoint, RecordingPhase, RecordingStatus, SectionDetail, TrackDetail,
};
pub use error::AppError;
pub use guide::{
    Guide, GuideParams, GuidePhase, GuideStatus, HintParams, HintStage, LegKind, LegSpan, LineTurn,
    NextWaypoint, OffNetwork, Projection, Remaining, RouteLine, TurnHint, WayBack, WayBackRouter,
    WayBackTarget,
};
pub use plan::{
    KnownPosition, Leg, LegError, LegSpec, MAX_POSITION_AGE_S, PlanRequest, RoutePlan, RoutePoint,
    RouteStart, Stage, Totals,
};
pub use regions::{MapOverview, RegionInfo, RegionState, RegionsOverview};
pub use routes::{ProfileInfo, RouterFactory};
pub use search::{SearchHit, SearchResults};
pub use share::{
    ImportOutcome, ImportedRoute, RemoteRoute, RemoteRouteState, SharedRoute, SharedRouteInfo,
};
