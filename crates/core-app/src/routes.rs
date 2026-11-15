//! Route planning: routes from the router of the region or straight lines,
//! with elevation from the installed regions; saved routes.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use core_gpx::{GpxDocument, write_gpx};
use core_routing::{OptionSpec, Profile, Router, RoutingError};
use core_storage::RouteRecord;
use core_track::{Activity, Segment, Track, TrackPoint};
use serde::Serialize;
use uuid::Uuid;

use crate::AppError;
use crate::app::{App, now_ms};
use crate::plan::{LegCache, RoutePlan};

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
    /// Computed legs, reused while their inputs stay the same.
    legs: Mutex<LegCache>,
    planned: Mutex<Option<RoutePlan>>,
}

impl Routing {
    pub(crate) fn routers(&self) -> MutexGuard<'_, HashMap<String, Arc<dyn Router>>> {
        self.routers.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn legs(&self) -> MutexGuard<'_, LegCache> {
        self.legs.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The router of a region, opened on first use.
    pub(crate) fn router(
        &self,
        region: &str,
        tiles: &Path,
    ) -> Result<Arc<dyn Router>, RoutingError> {
        let mut routers = self.routers();
        if let Some(router) = routers.get(region) {
            return Ok(Arc::clone(router));
        }
        let router = open_router(tiles)?;
        routers.insert(region.to_string(), Arc::clone(&router));
        Ok(router)
    }

    pub(crate) fn planned(&self) -> MutexGuard<'_, Option<RoutePlan>> {
        self.planned.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Closes the router of a region whose files are about to change.
    /// Computed legs may change with any region (routing and elevation data).
    pub(crate) fn forget_region(&self, id: &str) {
        self.routers().remove(id);
        self.legs().clear();
    }
}

#[cfg(feature = "valhalla")]
fn open_router(tiles: &Path) -> Result<Arc<dyn Router>, RoutingError> {
    Ok(Arc::new(core_routing::ValhallaRouter::open(tiles)?))
}

#[cfg(not(feature = "valhalla"))]
fn open_router(_tiles: &Path) -> Result<Arc<dyn Router>, RoutingError> {
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
            activity: plan.legs.first().and_then(|leg| activity_of(leg.profile)),
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
            plan.legs.first().map_or("", |leg| leg.profile.as_str()),
            plan.total.distance_m,
            plan.total.ascent_m,
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
