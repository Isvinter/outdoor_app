//! Route planning: routes from the router of the region or straight lines,
//! with elevation from the installed regions; saved routes.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use core_gpx::{GpxDocument, GpxWaypoint, write_gpx};
use core_routing::{LatLon, OptionSpec, Profile, RouteOptions, Router, RoutingError};
use core_storage::RouteRecord;
use core_track::{Activity, Segment, Track, TrackPoint};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppError;
use crate::app::{App, now_ms};
use crate::plan::{LegCache, LegSpec, PlanRequest, RoutePlan};

/// A routing profile as offered to the user.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProfileInfo {
    pub profile: Profile,
    pub options: Vec<OptionSpec>,
    /// Whether this build can compute the profile at all; routed profiles
    /// additionally need a region with routing data.
    pub available: bool,
}

/// Opens the router for the routing tiles of a region. The mobile apps give
/// their own (Valhalla outside the core); otherwise the one built in with the
/// feature `valhalla` is used.
pub trait RouterFactory: Send + Sync {
    fn open(&self, tiles: &Path) -> Result<Arc<dyn Router>, RoutingError>;
    /// Version of Valhalla behind the routers; regions whose routing data name
    /// another version are not routed. `None` skips the check.
    fn engine_version(&self) -> Option<String>;
}

#[cfg(feature = "valhalla")]
struct BuiltIn;

#[cfg(feature = "valhalla")]
impl RouterFactory for BuiltIn {
    fn open(&self, tiles: &Path) -> Result<Arc<dyn Router>, RoutingError> {
        Ok(Arc::new(core_routing::ValhallaRouter::open(tiles)?))
    }

    fn engine_version(&self) -> Option<String> {
        None
    }
}

/// Routing state owned by the app.
#[derive(Default)]
pub(crate) struct Routing {
    /// Set by the app; otherwise the built-in factory, if there is one.
    factory: Mutex<Option<Arc<dyn RouterFactory>>>,
    /// Open routers by region id.
    routers: Mutex<HashMap<String, Arc<dyn Router>>>,
    /// Computed legs, reused while their inputs stay the same.
    legs: Mutex<LegCache>,
    planned: Mutex<Option<RoutePlan>>,
    /// Counts the plans begun and the changes of `planned` by other means, so
    /// that a plan finishing after a newer one does not replace it.
    generation: AtomicU64,
    /// The saved route the planned route belongs to: loaded or saved last.
    loaded: Mutex<Option<Uuid>>,
}

impl Routing {
    pub(crate) fn routers(&self) -> MutexGuard<'_, HashMap<String, Arc<dyn Router>>> {
        self.routers.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn legs(&self) -> MutexGuard<'_, LegCache> {
        self.legs.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn factory(&self) -> Option<Arc<dyn RouterFactory>> {
        let set = self
            .factory
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        #[cfg(feature = "valhalla")]
        let set = set.or_else(|| Some(Arc::new(BuiltIn) as Arc<dyn RouterFactory>));
        set
    }

    /// Replaces the factory; routers opened by the old one are closed.
    pub(crate) fn set_factory(&self, factory: Arc<dyn RouterFactory>) {
        *self.factory.lock().unwrap_or_else(PoisonError::into_inner) = Some(factory);
        self.routers().clear();
        self.legs().clear();
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
        let factory = self
            .factory()
            .ok_or_else(|| RoutingError::Unavailable("this build has no routing engine".into()))?;
        let router = factory.open(tiles)?;
        routers.insert(region.to_string(), Arc::clone(&router));
        Ok(router)
    }

    pub(crate) fn loaded(&self) -> MutexGuard<'_, Option<Uuid>> {
        self.loaded.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn planned(&self) -> MutexGuard<'_, Option<RoutePlan>> {
        self.planned.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A plan is begun; its result counts if nothing newer happens meanwhile.
    pub(crate) fn begin_plan(&self) -> u64 {
        self.generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Keeps the result of the plan begun as `generation`, unless a newer plan
    /// was begun or the planned route was replaced since.
    pub(crate) fn finish_plan(&self, generation: u64, plan: RoutePlan) {
        let mut planned = self.planned();
        if self.generation.load(Ordering::SeqCst) == generation {
            *planned = Some(plan);
        }
    }

    /// Replaces the planned route; plans still being computed no longer count.
    pub(crate) fn set_planned(&self, plan: Option<RoutePlan>) {
        let mut planned = self.planned();
        self.generation.fetch_add(1, Ordering::SeqCst);
        *planned = plan;
    }

    /// Closes the router of a region whose files are about to change.
    /// Computed legs may change with any region (routing and elevation data).
    pub(crate) fn forget_region(&self, id: &str) {
        self.routers().remove(id);
        self.legs().clear();
    }
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

/// The profiles of a route in order of first appearance, joined by `+`.
fn profile_label(plan: &RoutePlan) -> String {
    let mut profiles: Vec<&str> = Vec::new();
    for leg in &plan.request.legs {
        let name = leg.profile.as_str();
        if !profiles.contains(&name) {
            profiles.push(name);
        }
    }
    profiles.join("+")
}

/// One track per stage, one segment per run of legs with the same activity;
/// the ends of the stages as waypoints.
fn plan_gpx(plan: &RoutePlan, name: &str) -> Result<String, AppError> {
    let several = plan.stages.len() > 1;
    let mut tracks = Vec::with_capacity(plan.stages.len());
    for (number, stage) in (1..).zip(&plan.stages) {
        let mut segments: Vec<(Option<Activity>, Vec<TrackPoint>)> = Vec::new();
        for leg in stage.from..stage.to {
            let activity = activity_of(plan.legs[leg].profile);
            let points = plan
                .elevation
                .iter()
                .filter(|point| point.leg == leg)
                .map(|point| TrackPoint {
                    lat: point.lat,
                    lon: point.lon,
                    ele: point.ele.map(f64::from),
                    time_ms: None,
                    h_accuracy_m: None,
                    speed_mps: None,
                });
            match segments.last_mut() {
                Some((last, run)) if *last == activity => run.extend(points),
                _ => segments.push((activity, points.collect())),
            }
        }
        segments.retain(|(_, points)| !points.is_empty());
        let first = segments.first().and_then(|(activity, _)| *activity);
        let uniform = segments.iter().all(|(activity, _)| *activity == first);
        tracks.push(Track {
            name: Some(if several {
                format!("{name} – Etappe {number}")
            } else {
                name.to_string()
            }),
            activity: if uniform { first } else { None },
            segments: segments
                .into_iter()
                .map(|(_, points)| Segment { points })
                .collect(),
        });
    }
    let waypoints = (1..)
        .zip(&plan.request.stage_ends)
        .map(|(number, &end)| {
            let at = plan.request.waypoints[end];
            let ele = plan
                .elevation
                .iter()
                .rev()
                .find(|point| point.leg + 1 == end)
                .and_then(|point| point.ele.map(f64::from));
            GpxWaypoint {
                lat: at.lat,
                lon: at.lon,
                ele,
                name: Some(format!("Etappe {number} Ende")),
            }
        })
        .collect();
    Ok(write_gpx(&GpxDocument {
        name: Some(name.to_string()),
        waypoints,
        tracks,
    })?)
}

/// What is read of a route saved before routes had legs.
#[derive(Deserialize)]
struct OldPlan {
    route: OldRoute,
    #[serde(default)]
    options: RouteOptions,
}

#[derive(Deserialize)]
struct OldRoute {
    profile: Profile,
    waypoints: Vec<LatLon>,
}

impl App {
    /// Routes from now on with routers of `factory`, e.g. Valhalla in the app.
    pub fn set_router_factory(&self, factory: Arc<dyn RouterFactory>) {
        self.shared.routing.set_factory(factory);
    }

    /// The routing profiles with their adjustable settings.
    pub fn route_profiles(&self) -> Vec<ProfileInfo> {
        let routed = self.shared.routing.factory().is_some();
        Profile::ALL
            .into_iter()
            .map(|profile| ProfileInfo {
                profile,
                options: profile.options().to_vec(),
                available: profile == Profile::Manual || routed,
            })
            .collect()
    }

    /// The route planned or loaded last.
    pub fn planned_route(&self) -> Option<RoutePlan> {
        self.shared.routing.planned().clone()
    }

    /// Forgets the planned route (fewer than two points are left). The saved
    /// route it belongs to stays: "save" still overwrites it.
    pub fn clear_planned_route(&self) {
        self.shared.routing.set_planned(None);
    }

    /// Starts afresh: no planned route, and "save" stores a new one.
    pub fn start_new_route(&self) {
        self.clear_planned_route();
        *self.shared.routing.loaded() = None;
    }

    /// The saved route that "save" overwrites: the one loaded or saved last.
    pub fn loaded_route(&self) -> Option<Uuid> {
        *self.shared.routing.loaded()
    }

    /// Stores the planned route under a name. It replaces the saved route it
    /// was loaded from or saved as before; otherwise it is stored anew.
    pub fn save_planned_route(&self, name: &str) -> Result<RouteRecord, AppError> {
        self.store_planned_route(name, self.loaded_route())
    }

    /// Stores the planned route as a new saved route, which "save" overwrites
    /// from now on.
    pub fn save_planned_route_as_new(&self, name: &str) -> Result<RouteRecord, AppError> {
        self.store_planned_route(name, None)
    }

    fn store_planned_route(
        &self,
        name: &str,
        replace: Option<Uuid>,
    ) -> Result<RouteRecord, AppError> {
        let plan = self.planned_route().ok_or(AppError::NoRoutePlanned)?;
        let json = serde_json::to_string(&plan).map_err(|e| AppError::RoutePlan(e.to_string()))?;
        let (profile, now) = (profile_label(&plan), now_ms());
        let mut inner = self.shared.inner();
        let replaced = match replace {
            Some(id) => match inner.storage.replace_route(
                id,
                name,
                &profile,
                plan.total.distance_m,
                plan.total.ascent_m,
                &json,
                now,
            ) {
                Ok(()) => Some(id),
                // Deleted meanwhile: store it anew.
                Err(core_storage::StorageError::NotFound(_)) => None,
                Err(error) => return Err(error.into()),
            },
            None => None,
        };
        let id = match replaced {
            Some(id) => id,
            None => inner.storage.save_route(
                name,
                &profile,
                plan.total.distance_m,
                plan.total.ascent_m,
                &json,
                now,
            )?,
        };
        *self.shared.routing.loaded() = Some(id);
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

    /// A saved plan; one saved before routes had legs is computed anew from
    /// its waypoints, all legs in its one profile.
    fn load_plan(&self, id: Uuid) -> Result<RoutePlan, AppError> {
        let json = self.shared.inner().storage.route_plan(id)?;
        let broken = |e: serde_json::Error| AppError::RoutePlan(e.to_string());
        let value: serde_json::Value = serde_json::from_str(&json).map_err(broken)?;
        if value.get("format").is_some() {
            return serde_json::from_value(value).map_err(broken);
        }
        let old: OldPlan = serde_json::from_value(value).map_err(broken)?;
        let spec = LegSpec {
            profile: old.route.profile,
            options: old.options,
        };
        let legs = vec![spec; old.route.waypoints.len().saturating_sub(1)];
        self.plan_route(&PlanRequest {
            waypoints: old.route.waypoints,
            legs,
            stage_ends: Vec::new(),
        })
    }

    /// Loads a saved route; it becomes the planned route.
    pub fn saved_route(&self, id: Uuid) -> Result<RoutePlan, AppError> {
        let plan = self.load_plan(id)?;
        self.shared.routing.set_planned(Some(plan.clone()));
        *self.shared.routing.loaded() = Some(id);
        Ok(plan)
    }

    pub fn rename_route(&self, id: Uuid, name: &str) -> Result<(), AppError> {
        Ok(self
            .shared
            .inner()
            .storage
            .rename_route(id, name, now_ms())?)
    }

    /// Deletes a saved route; if it was loaded, "save" stores the planned
    /// route anew.
    pub fn delete_route(&self, id: Uuid) -> Result<(), AppError> {
        self.shared.inner().storage.delete_route(id, now_ms())?;
        let mut loaded = self.shared.routing.loaded();
        if *loaded == Some(id) {
            *loaded = None;
        }
        Ok(())
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
