//! In-process routing with Valhalla.

use std::path::Path;
use std::sync::{Mutex, PoisonError};

use valhalla::config::{
    Logging, Mjolnir, ServiceLimits, ServiceLimitsBicycle, ServiceLimitsPedestrian,
};
use valhalla::proto::options::Action;
use valhalla::{Actor, ConfigBuilder, Response};

use crate::model::{
    MatrixEntry, MatrixRequest, Profile, Route, RouteRequest, Router, RoutingError,
};
use crate::valhalla_api::VALHALLA_PROFILES;
use crate::valhalla_api::{
    costing_json, engine_error, matrix_json, parse_valhalla_matrix, parse_valhalla_route,
};

/// Matrix search reach for bikes and pedestrians, in metres of Valhalla's
/// `max_matrix_distance`.
const MATRIX_REACH_M: f64 = 1_000_000.0;

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
            // The matrix search stops at a cost derived from this distance
            // (assuming 10 mph by bike, 2 mph on foot). Bike costs include
            // hill and surface penalties, so with the default 200 km it
            // missed most targets beyond about 20 km that single routes find.
            service_limits: ServiceLimits {
                bicycle: ServiceLimitsBicycle {
                    max_matrix_distance: MATRIX_REACH_M,
                    ..Default::default()
                },
                pedestrian: ServiceLimitsPedestrian {
                    max_matrix_distance: MATRIX_REACH_M,
                    ..Default::default()
                },
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

    fn matrix(&self, request: &MatrixRequest) -> Result<Vec<Option<MatrixEntry>>, RoutingError> {
        let json = matrix_json(request)?;
        if request.to.is_empty() {
            return Ok(Vec::new());
        }
        let options = Actor::parse_json_request(&json, Action::SourcesToTargets)
            .map_err(|e| RoutingError::Engine(e.to_string()))?;
        let response = self
            .actor
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .matrix(&options);
        match response {
            Ok(Response::Json(answer)) => parse_valhalla_matrix(&answer, request.to.len()),
            Ok(_) => Err(RoutingError::Engine("unexpected answer format".into())),
            // No way to any of the targets.
            Err(e) => match engine_error(&e.to_string()) {
                RoutingError::NoRoute(_) => Ok(vec![None; request.to.len()]),
                other => Err(other),
            },
        }
    }

    fn profiles(&self) -> Vec<Profile> {
        VALHALLA_PROFILES.to_vec()
    }
}
