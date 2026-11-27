//! In-process routing with Valhalla.

use std::path::Path;
use std::sync::{Mutex, PoisonError};

use valhalla::config::{
    Logging, Mjolnir, ServiceLimits, ServiceLimitsAuto, ServiceLimitsBicycle,
    ServiceLimitsPedestrian,
};
use valhalla::proto::options::Action;
use valhalla::{Actor, ConfigBuilder, Response};

use crate::model::{
    LatLon, MatrixEntry, MatrixRequest, Profile, Route, RouteRequest, Router, RoutingError,
};
use crate::valhalla_api::VALHALLA_PROFILES;
use crate::valhalla_api::{
    costing_json, engine_error, matrix_json, parse_valhalla_matrix, parse_valhalla_route,
};

/// How far the matrix searches, per Valhalla costing: its `max_matrix_distance`.
/// The one-to-many search for search hits stops at a cost derived from this
/// distance (at 10 mph by bike, 2 mph on foot, 35 mph by car), so a target
/// that needs a long detour is not reached and comes back as `None` ("Umweg
/// oder kein Weg") instead of making the search run on for seconds.
///
/// Tuned on alpenraum-ost (1.4 GB) from Bozen: the largest round value for
/// which a ring of 30 targets at 50 km takes at most about 300 ms (release
/// build, median of 3) for every profile with that costing. For bikes
/// (RoadBike, Trekking, Mtb) 300 km gives 237 / 133 / 164 ms and reaches
/// 17 / 10 / 26 of 30 targets; 350 km already costs 314 ms for RoadBike.
const BICYCLE_REACH_M: f64 = 300_000.0;
/// Reach for Hike. Valhalla's default; Hike takes about 280 ms at 50 km and
/// reaches all 30 targets, and a larger value changes nothing.
const PEDESTRIAN_REACH_M: f64 = 200_000.0;
/// Reach for Car. Valhalla's default; Car takes about 140 ms at 50 km and
/// reaches 27 of 30 targets (the other three have no road), larger values
/// change nothing.
const AUTO_REACH_M: f64 = 400_000.0;

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
            service_limits: ServiceLimits {
                bicycle: ServiceLimitsBicycle {
                    max_matrix_distance: BICYCLE_REACH_M,
                    ..Default::default()
                },
                pedestrian: ServiceLimitsPedestrian {
                    max_matrix_distance: PEDESTRIAN_REACH_M,
                    ..Default::default()
                },
                auto: ServiceLimitsAuto {
                    max_matrix_distance: AUTO_REACH_M,
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

impl ValhallaRouter {
    /// One matrix call; "no way" errors come back as such.
    fn matrix_once(
        &self,
        request: &MatrixRequest,
    ) -> Result<Vec<Option<MatrixEntry>>, RoutingError> {
        let json = matrix_json(request)?;
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
            Err(e) => Err(engine_error(&e.to_string())),
        }
    }

    /// Valhalla rejects the whole matrix when one location cannot be placed
    /// on the network (e.g. outside the tiles) or is in a separate region.
    /// Then the targets are halved until the culprits stand alone and count
    /// as "no way"; the other targets keep their answers.
    fn matrix_split(
        &self,
        request: &MatrixRequest,
    ) -> Result<Vec<Option<MatrixEntry>>, RoutingError> {
        if request.to.is_empty() {
            return Ok(Vec::new());
        }
        match self.matrix_once(request) {
            Err(error) if is_unroutable(&error) => {
                if request.to.len() == 1 {
                    return Ok(vec![None]);
                }
                let (first, second) = request.to.split_at(request.to.len() / 2);
                let part = |to: &[LatLon]| MatrixRequest {
                    to: to.to_vec(),
                    ..request.clone()
                };
                let mut entries = self.matrix_split(&part(first))?;
                entries.extend(self.matrix_split(&part(second))?);
                Ok(entries)
            }
            other => other,
        }
    }
}

/// Errors that say "there is no way to (one of) the targets".
fn is_unroutable(error: &RoutingError) -> bool {
    match error {
        RoutingError::NoRoute(_) => true,
        RoutingError::Engine(message) => message.contains("unconnected regions"),
        _ => false,
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
        // Validates profile and options even without targets.
        matrix_json(request)?;
        self.matrix_split(request)
    }

    fn profiles(&self) -> Vec<Profile> {
        VALHALLA_PROFILES.to_vec()
    }
}
