//! In-process routing with Valhalla.

use std::path::Path;
use std::sync::{Mutex, PoisonError};

use valhalla::config::{Logging, Mjolnir};
use valhalla::proto::options::Action;
use valhalla::{Actor, ConfigBuilder, Response};

use crate::model::{Profile, Route, RouteRequest, Router, RoutingError};
use crate::valhalla_api::VALHALLA_PROFILES;
use crate::valhalla_api::{costing_json, engine_error, parse_valhalla_route};

/// Routes on the Valhalla tiles of one region (`routing.tar`).
pub struct ValhallaRouter {
    // Valhalla's actor keeps per-request state; one request at a time.
    actor: Mutex<Actor>,
}

impl ValhallaRouter {
    /// Opens a tile extract as written by `scripts/build-valhalla-tiles.sh`.
    pub fn open(tile_extract: &Path) -> Result<Self, RoutingError> {
        if !tile_extract.is_file() {
            return Err(RoutingError::Unavailable(format!(
                "{} does not exist",
                tile_extract.display()
            )));
        }
        let config = ConfigBuilder {
            // Valhalla writes to stdout by default, and warns about optional
            // data that an offline device never has; the empty type is silent.
            logging: Logging {
                r#type: String::new(),
                ..Default::default()
            },
            mjolnir: Mjolnir {
                tile_extract: tile_extract.display().to_string(),
                ..Default::default()
            },
            ..Default::default()
        }
        .build();
        let actor = Actor::new(&config).map_err(|e| RoutingError::Engine(e.to_string()))?;
        Ok(ValhallaRouter {
            actor: Mutex::new(actor),
        })
    }
}

impl Router for ValhallaRouter {
    fn route(&self, request: &RouteRequest) -> Result<Route, RoutingError> {
        let json = costing_json(request)?;
        let options = Actor::parse_json_request(&json, Action::Route)
            .map_err(|e| RoutingError::Engine(e.to_string()))?;
        let response = self
            .actor
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .route(&options);
        match response {
            Ok(Response::Json(answer)) => parse_valhalla_route(&answer, request),
            Ok(_) => Err(RoutingError::Engine("unexpected answer format".into())),
            Err(e) => Err(engine_error(&e.to_string())),
        }
    }

    fn profiles(&self) -> Vec<Profile> {
        VALHALLA_PROFILES.to_vec()
    }
}
