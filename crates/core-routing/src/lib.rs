//! Route planning: profiles, the route model, time estimates and the
//! [`Router`] trait. The Valhalla implementation sits behind the feature
//! `valhalla`, so that everything else builds without a C++ toolchain.

mod manual;
mod model;
mod valhalla_api;
#[cfg(feature = "valhalla")]
mod valhalla_router;

pub use manual::{DinParams, din_time_s, manual_route};
pub use model::{
    LatLon, OptionSpec, Profile, Route, RouteOptions, RouteRequest, Router, RoutingError,
};
pub use valhalla_api::{costing_json, decode_polyline6, parse_valhalla_route};
#[cfg(feature = "valhalla")]
pub use valhalla_router::ValhallaRouter;
