//! Route planning: profiles, the route model, time estimates and the
//! [`Router`] trait. The Valhalla implementation sits behind the feature
//! `valhalla`, so that everything else builds without a C++ toolchain.

mod json_router;
mod manual;
mod model;
pub mod turns;
mod valhalla_api;
#[cfg(feature = "valhalla")]
mod valhalla_router;

pub use json_router::{JsonEngine, JsonRouter};
pub use manual::{DinParams, din_time_s, manual_route};
pub use model::{
    FAILED_LEG_COLOR, LatLon, MatrixEntry, MatrixRequest, OptionSpec, Profile, Route, RouteOptions,
    RouteRequest, Router, RoutingError, Turn, TurnKind,
};
pub use valhalla_api::{
    VALHALLA_PROFILES, costing_json, decode_polyline6, engine_error, matrix_json,
    parse_valhalla_matrix, parse_valhalla_route,
};
#[cfg(feature = "valhalla")]
pub use valhalla_router::ValhallaRouter;
