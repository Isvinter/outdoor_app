//! Route planning: routes from the router of the region or straight lines,
//! with elevation from the installed regions; saved routes.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use core_gpx::{GpxDocument, write_gpx};
use core_maps::ProfilePoint;
use core_routing::{
    DinParams, OptionSpec, Profile, Route, RouteOptions, RouteRequest, Router, RoutingError,
    din_time_s, manual_route,
};
use core_storage::RouteRecord;
use core_track::{Activity, Segment, Track, TrackPoint, elevation_gain};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppError;
use crate::app::{App, now_ms};

/// Largest distance between two samples of the elevation profile, in metres.
const PROFILE_STEP_M: f64 = 25.0;
/// Smoothing window for ascent and descent. Terrain models are smooth
/// compared to GPS elevation, a short window is enough.
const ELE_SMOOTH_M: f64 = 50.0;
/// Changes of direction smaller than this do not count as ascent or descent.
const ELE_THRESHOLD_M: f64 = 3.0;

/// A route with everything shown about it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutePlan {
    pub route: Route,
    /// The settings the route was computed with.
    pub options: RouteOptions,
    /// Elevation along the route.
    pub elevation: Vec<ProfilePoint>,
    /// `None` if the installed regions have no elevation data for the route.
    pub ascent_m: Option<f64>,
    pub descent_m: Option<f64>,
    pub min_ele_m: Option<f64>,
    pub max_ele_m: Option<f64>,
    /// Walking time after DIN 33466, for routes on foot or ski with known ascent.
    pub hiking_time_s: Option<f64>,
    /// The region whose routing data was used.
    pub region: Option<String>,
    /// Ways above this `mtb:scale` are closed to bicycles in that routing data.
    pub max_mtb_scale: Option<u8>,
}

/// A routing profile as offered to the user.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProfileInfo {
    pub profile: Profile,
    pub options: Vec<OptionSpec>,
    /// Whether this build can compute the profile at all; routed profiles
    /// additionally need a region with routing data.
    pub available: bool,
}

/// Routing state owned by the app.
#[derive(Default)]
pub(crate) struct Routing {
    /// Open routers by region id.
    routers: Mutex<HashMap<String, Arc<dyn Router>>>,
    planned: Mutex<Option<RoutePlan>>,
}

impl Routing {
    fn routers(&self) -> MutexGuard<'_, HashMap<String, Arc<dyn Router>>> {
        self.routers.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn planned(&self) -> MutexGuard<'_, Option<RoutePlan>> {
        self.planned.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Closes the router of a region whose files are about to change.
    pub(crate) fn forget_region(&self, id: &str) {
        self.routers().remove(id);
    }
}

#[cfg(feature = "valhalla")]
fn open_router(tiles: &std::path::Path) -> Result<Arc<dyn Router>, RoutingError> {
    Ok(Arc::new(core_routing::ValhallaRouter::open(tiles)?))
}

#[cfg(not(feature = "valhalla"))]
fn open_router(_tiles: &std::path::Path) -> Result<Arc<dyn Router>, RoutingError> {
    Err(RoutingError::Unavailable(
        "this build has no routing engine".into(),
    ))
}

fn activity_of(profile: Profile) -> Option<Activity> {
    match profile {
        Profile::RoadBike | Profile::Trekking => Some(Activity::Bike),
        Profile::Mtb => Some(Activity::Mtb),
        Profile::Hike => Some(Activity::Hike),
        Profile::Manual => Some(Activity::SkiTour),
        Profile::Car => None,
    }
}

fn plan_gpx(plan: &RoutePlan, name: &str) -> Result<String, AppError> {
    let points = plan
        .elevation
        .iter()
        .map(|point| TrackPoint {
            lat: point.lat,
            lon: point.lon,
            ele: point.ele.map(f64::from),
            time_ms: None,
            h_accuracy_m: None,
            speed_mps: None,
        })
        .collect();
    let name = Some(name.to_string());
    Ok(write_gpx(&GpxDocument {
        name: name.clone(),
        tracks: vec![Track {
            name,
            activity: activity_of(plan.route.profile),
            segments: vec![Segment { points }],
        }],
    })?)
}

impl App {
    /// The routing profiles with their adjustable settings.
    pub fn route_profiles(&self) -> Vec<ProfileInfo> {
        Profile::ALL
            .into_iter()
            .map(|profile| ProfileInfo {
                profile,
                options: profile.options().to_vec(),
                available: profile == Profile::Manual || cfg!(feature = "valhalla"),
            })
            .collect()
    }

    /// Computes a route and remembers it as the planned route. May take a
    /// moment; call it off the UI thread.
    pub fn plan_route(&self, request: &RouteRequest) -> Result<RoutePlan, AppError> {
        let (route, region, max_mtb_scale) = if request.profile == Profile::Manual {
            (manual_route(&request.waypoints)?, None, None)
        } else {
            if request.waypoints.len() < 2 {
                return Err(RoutingError::TooFewWaypoints.into());
            }
            let positions: Vec<(f64, f64)> =
                request.waypoints.iter().map(|p| (p.lat, p.lon)).collect();
            let tiles = self
                .shared
                .maps
                .store()
                .routing_tiles(&positions)
                .ok_or_else(|| {
                    RoutingError::Unavailable(
                        "no installed region with routing data covers all waypoints".into(),
                    )
                })?;
            let router = {
                let mut routers = self.shared.routing.routers();
                match routers.get(&tiles.region) {
                    Some(router) => Arc::clone(router),
                    None => {
                        let router = open_router(&tiles.path)?;
                        routers.insert(tiles.region.clone(), Arc::clone(&router));
                        router
                    }
                }
            };
            let scale = tiles.info.map(|info| info.max_mtb_scale);
            (router.route(request)?, Some(tiles.region), scale)
        };

        let line: Vec<(f64, f64)> = route.line.iter().map(|p| (p.lat, p.lon)).collect();
        let elevation = self
            .shared
            .maps
            .store()
            .elevation_profile(&line, PROFILE_STEP_M)?;
        let known: Vec<(f64, f64)> = elevation
            .iter()
            .filter_map(|point| Some((point.distance_m, f64::from(point.ele?))))
            .collect();
        // Gaps in the elevation data would hide ascent; then rather none at all.
        let complete = known.len() >= 2 && known.len() == elevation.len();
        let (ascent_m, descent_m) = if complete {
            let (up, down) = elevation_gain(&known, ELE_SMOOTH_M, ELE_THRESHOLD_M);
            (Some(up), Some(down))
        } else {
            (None, None)
        };
        let eles = || known.iter().map(|(_, ele)| *ele);
        let on_foot = matches!(route.profile, Profile::Hike | Profile::Manual);
        let hiking_time_s = match (on_foot, ascent_m, descent_m) {
            (true, Some(up), Some(down)) => Some(din_time_s(
                route.distance_m,
                up,
                down,
                &DinParams::default(),
            )),
            _ => None,
        };
        let plan = RoutePlan {
            route,
            options: request.options,
            elevation,
            ascent_m,
            descent_m,
            min_ele_m: eles().reduce(f64::min).filter(|_| complete),
            max_ele_m: eles().reduce(f64::max).filter(|_| complete),
            hiking_time_s,
            region,
            max_mtb_scale,
        };
        *self.shared.routing.planned() = Some(plan.clone());
        Ok(plan)
    }

    /// The route planned or loaded last.
    pub fn planned_route(&self) -> Option<RoutePlan> {
        self.shared.routing.planned().clone()
    }

    pub fn clear_planned_route(&self) {
        *self.shared.routing.planned() = None;
    }

    /// Stores the planned route under a name.
    pub fn save_planned_route(&self, name: &str) -> Result<RouteRecord, AppError> {
        let plan = self.planned_route().ok_or(AppError::NoRoutePlanned)?;
        let json = serde_json::to_string(&plan).map_err(|e| AppError::RoutePlan(e.to_string()))?;
        let mut inner = self.shared.inner();
        let id = inner.storage.save_route(
            name,
            plan.route.profile.as_str(),
            plan.route.distance_m,
            plan.ascent_m,
            &json,
            now_ms(),
        )?;
        let routes = inner.storage.list_routes()?;
        routes
            .into_iter()
            .find(|record| record.id == id)
            .ok_or(AppError::Storage(core_storage::StorageError::NotFound(id)))
    }

    /// The saved routes, newest first.
    pub fn saved_routes(&self) -> Result<Vec<RouteRecord>, AppError> {
        Ok(self.shared.inner().storage.list_routes()?)
    }

    fn load_plan(&self, id: Uuid) -> Result<RoutePlan, AppError> {
        let json = self.shared.inner().storage.route_plan(id)?;
        serde_json::from_str(&json).map_err(|e| AppError::RoutePlan(e.to_string()))
    }

    /// Loads a saved route; it becomes the planned route.
    pub fn saved_route(&self, id: Uuid) -> Result<RoutePlan, AppError> {
        let plan = self.load_plan(id)?;
        *self.shared.routing.planned() = Some(plan.clone());
        Ok(plan)
    }

    pub fn rename_route(&self, id: Uuid, name: &str) -> Result<(), AppError> {
        Ok(self
            .shared
            .inner()
            .storage
            .rename_route(id, name, now_ms())?)
    }

    pub fn delete_route(&self, id: Uuid) -> Result<(), AppError> {
        Ok(self.shared.inner().storage.delete_route(id, now_ms())?)
    }

    /// The planned route as a GPX 1.1 track with elevation, without times.
    pub fn export_planned_route_gpx(&self, name: &str) -> Result<String, AppError> {
        let plan = self.planned_route().ok_or(AppError::NoRoutePlanned)?;
        plan_gpx(&plan, name)
    }

    /// A saved route as a GPX 1.1 track.
    pub fn export_route_gpx(&self, id: Uuid) -> Result<String, AppError> {
        let plan = self.load_plan(id)?;
        let name = self
            .saved_routes()?
            .into_iter()
            .find(|record| record.id == id)
            .map(|record| record.name)
            .unwrap_or_default();
        plan_gpx(&plan, &name)
    }
}
