//! Routing through Valhalla's JSON API, with the engine outside the core: on
//! iOS valhalla-mobile, reached through the app.

use std::sync::Arc;

use crate::model::{Profile, Route, RouteRequest, Router, RoutingError};
use crate::valhalla_api::{VALHALLA_PROFILES, costing_json, engine_error, parse_valhalla_route};

/// A Valhalla engine that answers route requests as JSON text.
pub trait JsonEngine: Send + Sync {
    /// Valhalla's answer to `request`, or its error message.
    fn route_json(&self, request: &str) -> Result<String, String>;
}

/// Builds the request, asks the engine, reads the answer.
pub struct JsonRouter {
    engine: Arc<dyn JsonEngine>,
}

impl JsonRouter {
    pub fn new(engine: Arc<dyn JsonEngine>) -> Self {
        JsonRouter { engine }
    }
}

impl Router for JsonRouter {
    fn route(&self, request: &RouteRequest) -> Result<Route, RoutingError> {
        let json = costing_json(request)?;
        let answer = self
            .engine
            .route_json(&json)
            .map_err(|message| engine_error(&message))?;
        parse_valhalla_route(&answer, request)
    }

    fn profiles(&self) -> Vec<Profile> {
        VALHALLA_PROFILES.to_vec()
    }
}
