//! Route planning for the apps: profiles, computing and editing a plan, the
//! saved routes, and Valhalla in the app as the core's router.

use std::path::Path;
use std::sync::Arc;

use core_app::{LegSpec, PlanRequest, RoutePlan, RouterFactory};
use core_routing::{JsonEngine, JsonRouter, Profile, RouteOptions, Router, RoutingError};
use core_storage::RouteRecord;
use uuid::Uuid;

use crate::{CoreApp, CoreError};

/// Valhalla in the app (valhalla-mobile on iOS). The core builds the requests
/// and reads the answers; the engine only passes the JSON text through.
#[uniffi::export(with_foreign)]
pub trait ValhallaEngine: Send + Sync {
    /// The Valhalla version, e.g. "3.9.1"; compared with the regions' routing data.
    fn version(&self) -> String;
    /// Valhalla's answer to `request_json` on the routing tiles `tiles_path`.
    fn route(&self, tiles_path: String, request_json: String) -> Result<String, EngineFailure>;
    /// The file at `tiles_path` was replaced or is to be read anew: whatever
    /// is open for it must be let go of before the next `route`.
    fn forget(&self, tiles_path: String);
}

#[derive(Debug, uniffi::Error)]
pub enum EngineFailure {
    Failed { message: String },
}

impl std::fmt::Display for EngineFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let EngineFailure::Failed { message } = self;
        f.write_str(message)
    }
}

impl std::error::Error for EngineFailure {}

impl From<uniffi::UnexpectedUniFFICallbackError> for EngineFailure {
    fn from(error: uniffi::UnexpectedUniFFICallbackError) -> Self {
        EngineFailure::Failed {
            message: error.reason,
        }
    }
}

/// One engine call for the routing tiles of one region.
struct RegionEngine {
    engine: Arc<dyn ValhallaEngine>,
    tiles: String,
}

impl JsonEngine for RegionEngine {
    fn route_json(&self, request: &str) -> Result<String, String> {
        self.engine
            .route(self.tiles.clone(), request.to_string())
            .map_err(|EngineFailure::Failed { message }| message)
    }
}

struct EngineFactory(Arc<dyn ValhallaEngine>);

impl RouterFactory for EngineFactory {
    fn open(&self, tiles: &Path) -> Result<Arc<dyn Router>, RoutingError> {
        let tiles = tiles.display().to_string();
        // The core opens a router at first use and again after the region
        // changed (update, switched off and on): from here on, no answer may
        // come from what the engine had open for the file before.
        self.0.forget(tiles.clone());
        Ok(Arc::new(JsonRouter::new(Arc::new(RegionEngine {
            engine: Arc::clone(&self.0),
            tiles,
        }))))
    }

    fn engine_version(&self) -> Option<String> {
        Some(self.0.version())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct LatLon {
    pub lat: f64,
    pub lon: f64,
}

/// A setting of a leg, by its key in the profile's options.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct OptionValue {
    pub key: String,
    pub value: f64,
}

/// How one leg is to be found.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct LegInput {
    /// As `core-routing` names it: "hike", "mtb", …
    pub profile: String,
    /// Settings that differ from the profile's defaults.
    pub options: Vec<OptionValue>,
}

/// A route as the user set it up; leg `i` runs from waypoint `i` to `i + 1`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PlanInput {
    pub waypoints: Vec<LatLon>,
    pub legs: Vec<LegInput>,
    /// Inner waypoints where a stage ends, ascending.
    pub stage_ends: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct OptionInfo {
    pub key: String,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub default: f64,
}

/// A profile as offered to the user.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ProfileChoice {
    pub profile: String,
    /// On the map and in the elevation profile, e.g. "#c62828".
    pub color: String,
    /// Whether it can be computed at all (routed profiles need an engine).
    pub available: bool,
    pub options: Vec<OptionInfo>,
}

/// A computed leg; a failed one has no line and adds nothing to the totals.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PlanLeg {
    pub profile: String,
    pub color: String,
    pub line: Vec<LatLon>,
    pub distance_m: f64,
    pub ascent_m: Option<f64>,
    pub descent_m: Option<f64>,
    pub time_s: Option<f64>,
    /// A code of the core's errors, e.g. "route_not_found".
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct ElevationSample {
    /// From the start of the route.
    pub distance_m: f64,
    pub lat: f64,
    pub lon: f64,
    pub ele: Option<f32>,
    /// Index of the leg the sample lies on.
    pub leg: u32,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct StageSummary {
    /// First and last waypoint of the stage.
    pub from: u32,
    pub to: u32,
    pub distance_m: f64,
    pub ascent_m: Option<f64>,
    pub descent_m: Option<f64>,
    pub time_s: Option<f64>,
    /// Colours of the profiles of its legs, in order of first appearance.
    pub colors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PlanTotals {
    pub distance_m: f64,
    pub ascent_m: Option<f64>,
    pub descent_m: Option<f64>,
    pub time_s: Option<f64>,
    pub min_ele_m: Option<f64>,
    pub max_ele_m: Option<f64>,
    pub failed_legs: u32,
}

/// A planned route with everything shown about it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PlanResult {
    pub request: PlanInput,
    pub legs: Vec<PlanLeg>,
    pub elevation: Vec<ElevationSample>,
    pub stages: Vec<StageSummary>,
    pub total: PlanTotals,
}

/// A saved route as shown in lists.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SavedRoute {
    pub id: String,
    pub name: String,
    /// The profiles of the route joined by "+".
    pub profile: String,
    pub distance_m: f64,
    pub ascent_m: Option<f64>,
    pub updated_at_ms: i64,
    pub rev: i64,
}

/// Samples of the elevation profile handed to the app; enough for a chart.
const CHART_SAMPLES: usize = 1000;

fn failed(code: &str, message: String) -> CoreError {
    CoreError::Failed {
        code: code.into(),
        message,
    }
}

fn index(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

impl From<LatLon> for core_routing::LatLon {
    fn from(p: LatLon) -> Self {
        core_routing::LatLon {
            lat: p.lat,
            lon: p.lon,
        }
    }
}

impl From<core_routing::LatLon> for LatLon {
    fn from(p: core_routing::LatLon) -> Self {
        LatLon {
            lat: p.lat,
            lon: p.lon,
        }
    }
}

fn options_from(values: &[OptionValue]) -> Result<RouteOptions, CoreError> {
    let mut options = RouteOptions::default();
    for OptionValue { key, value } in values {
        match key.as_str() {
            "use_hills" => options.use_hills = Some(*value),
            "use_roads" => options.use_roads = Some(*value),
            "avoid_bad_surfaces" => options.avoid_bad_surfaces = Some(*value),
            // A grade; out of range is reported when the leg is computed.
            "max_hiking_difficulty" => {
                options.max_hiking_difficulty = Some(value.round().clamp(0.0, 255.0) as u8);
            }
            other => {
                return Err(failed(
                    "route_plan",
                    format!("unknown route option {other}"),
                ));
            }
        }
    }
    Ok(options)
}

const OPTION_KEYS: [&str; 4] = [
    "use_hills",
    "use_roads",
    "avoid_bad_surfaces",
    "max_hiking_difficulty",
];

fn options_to(options: &RouteOptions) -> Vec<OptionValue> {
    OPTION_KEYS
        .iter()
        .filter_map(|key| {
            options.get(key).map(|value| OptionValue {
                key: (*key).into(),
                value,
            })
        })
        .collect()
}

fn profile_from(name: &str) -> Result<Profile, CoreError> {
    name.parse()
        .map_err(|_| failed("route_plan", format!("unknown profile {name}")))
}

fn leg_from(leg: &LegInput) -> Result<LegSpec, CoreError> {
    Ok(LegSpec {
        profile: profile_from(&leg.profile)?,
        options: options_from(&leg.options)?,
    })
}

fn leg_to(leg: &LegSpec) -> LegInput {
    LegInput {
        profile: leg.profile.as_str().into(),
        options: options_to(&leg.options),
    }
}

fn request_from(input: &PlanInput) -> Result<PlanRequest, CoreError> {
    Ok(PlanRequest {
        waypoints: input.waypoints.iter().map(|&p| p.into()).collect(),
        legs: input.legs.iter().map(leg_from).collect::<Result<_, _>>()?,
        stage_ends: input.stage_ends.iter().map(|&i| i as usize).collect(),
    })
}

fn request_to(request: &PlanRequest) -> PlanInput {
    PlanInput {
        waypoints: request.waypoints.iter().map(|&p| p.into()).collect(),
        legs: request.legs.iter().map(leg_to).collect(),
        stage_ends: request.stage_ends.iter().map(|&i| index(i)).collect(),
    }
}

pub(crate) fn result_to(plan: &RoutePlan) -> PlanResult {
    let plan = plan.for_display(CHART_SAMPLES);
    PlanResult {
        request: request_to(&plan.request),
        legs: plan
            .legs
            .iter()
            .map(|leg| PlanLeg {
                profile: leg.profile.as_str().into(),
                color: leg.profile.color().into(),
                line: leg.line.iter().map(|&p| p.into()).collect(),
                distance_m: leg.distance_m,
                ascent_m: leg.ascent_m,
                descent_m: leg.descent_m,
                time_s: leg.time_s,
                error_code: leg.error.as_ref().map(|e| e.code.clone()),
                error_message: leg.error.as_ref().map(|e| e.message.clone()),
            })
            .collect(),
        elevation: plan
            .elevation
            .iter()
            .map(|p| ElevationSample {
                distance_m: p.distance_m,
                lat: p.lat,
                lon: p.lon,
                ele: p.ele,
                leg: index(p.leg),
            })
            .collect(),
        stages: plan
            .stages
            .iter()
            .map(|stage| StageSummary {
                from: index(stage.from),
                to: index(stage.to),
                distance_m: stage.distance_m,
                ascent_m: stage.ascent_m,
                descent_m: stage.descent_m,
                time_s: stage.time_s,
                colors: stage.profiles.iter().map(|p| p.color().into()).collect(),
            })
            .collect(),
        total: PlanTotals {
            distance_m: plan.total.distance_m,
            ascent_m: plan.total.ascent_m,
            descent_m: plan.total.descent_m,
            time_s: plan.total.time_s,
            min_ele_m: plan.total.min_ele_m,
            max_ele_m: plan.total.max_ele_m,
            failed_legs: index(plan.total.failed_legs),
        },
    }
}

fn saved_to(record: RouteRecord) -> SavedRoute {
    SavedRoute {
        id: record.id.to_string(),
        name: record.name,
        profile: record.profile,
        distance_m: record.distance_m,
        ascent_m: record.ascent_m,
        updated_at_ms: record.updated_at_ms,
        rev: record.rev,
    }
}

pub(crate) fn route_id(id: &str) -> Result<Uuid, CoreError> {
    id.parse()
        .map_err(|_| failed("route_id", format!("not a route id: {id}")))
}

#[uniffi::export]
impl CoreApp {
    /// Routes from now on with `engine`; routers opened before are closed.
    pub fn set_valhalla_engine(&self, engine: Arc<dyn ValhallaEngine>) {
        self.app.set_router_factory(Arc::new(EngineFactory(engine)));
    }

    pub fn route_profiles(&self) -> Vec<ProfileChoice> {
        self.app
            .route_profiles()
            .into_iter()
            .map(|info| ProfileChoice {
                profile: info.profile.as_str().into(),
                color: info.profile.color().into(),
                available: info.available,
                options: info
                    .options
                    .iter()
                    .map(|spec| OptionInfo {
                        key: spec.key.into(),
                        min: spec.min,
                        max: spec.max,
                        step: spec.step,
                        default: spec.default,
                    })
                    .collect(),
            })
            .collect()
    }

    /// Computes a route leg by leg; a leg that fails is marked in the result.
    /// Takes a moment: call it off the main thread.
    pub fn plan_route(&self, request: PlanInput) -> Result<PlanResult, CoreError> {
        let plan = self.app.plan_route(&request_from(&request)?)?;
        Ok(result_to(&plan))
    }

    /// Forgets the planned route (fewer than two points are left); "save"
    /// still overwrites the saved route it belongs to.
    pub fn clear_planned_route(&self) {
        self.app.clear_planned_route();
    }

    /// "Leeren": no planned route, and the next "save" stores a new route.
    pub fn start_new_route(&self) {
        self.app.start_new_route();
    }

    /// Overwrites the saved route loaded or saved last, or stores a new one.
    pub fn save_route(&self, name: String) -> Result<SavedRoute, CoreError> {
        Ok(saved_to(self.app.save_planned_route(&name)?))
    }

    pub fn save_route_as_new(&self, name: String) -> Result<SavedRoute, CoreError> {
        Ok(saved_to(self.app.save_planned_route_as_new(&name)?))
    }

    /// Newest first.
    pub fn saved_routes(&self) -> Result<Vec<SavedRoute>, CoreError> {
        Ok(self.app.saved_routes()?.into_iter().map(saved_to).collect())
    }

    /// Loads a saved route; it becomes the planned route that "save" overwrites.
    pub fn load_route(&self, id: String) -> Result<PlanResult, CoreError> {
        Ok(result_to(&self.app.saved_route(route_id(&id)?)?))
    }

    pub fn rename_route(&self, id: String, name: String) -> Result<(), CoreError> {
        Ok(self.app.rename_route(route_id(&id)?, &name)?)
    }

    pub fn delete_route(&self, id: String) -> Result<(), CoreError> {
        Ok(self.app.delete_route(route_id(&id)?)?)
    }

    /// The saved route "save" overwrites, if any.
    pub fn loaded_route_id(&self) -> Option<String> {
        self.app.loaded_route().map(|id| id.to_string())
    }
}

/// Applies an edit of the core to the app's request. The app never sends an
/// unknown profile back (it only has those of the core), so a request that
/// cannot be read stays as it is.
fn edit(request: PlanInput, change: impl FnOnce(PlanRequest) -> PlanRequest) -> PlanInput {
    match request_from(&request) {
        Ok(parsed) => request_to(&change(parsed)),
        Err(_) => request,
    }
}

#[uniffi::export]
pub fn plan_empty() -> PlanInput {
    request_to(&PlanRequest::empty())
}

/// Adds a destination; the leg to it is `leg`.
#[uniffi::export]
pub fn plan_add_waypoint(request: PlanInput, at: LatLon, leg: LegInput) -> PlanInput {
    match leg_from(&leg) {
        Ok(leg) => edit(request, |r| r.add_waypoint(at.into(), leg)),
        Err(_) => request,
    }
}

#[uniffi::export]
pub fn plan_move_waypoint(request: PlanInput, index: u32, to: LatLon) -> PlanInput {
    edit(request, |r| r.move_waypoint(index as usize, to.into()))
}

/// Its neighbours are joined by the leg before it; a stage ending there ends no more.
#[uniffi::export]
pub fn plan_remove_waypoint(request: PlanInput, index: u32) -> PlanInput {
    edit(request, |r| r.remove_waypoint(index as usize))
}

/// Marks or unmarks an inner waypoint as the end of a stage.
#[uniffi::export]
pub fn plan_toggle_stage_end(request: PlanInput, index: u32) -> PlanInput {
    edit(request, |r| r.toggle_stage_end(index as usize))
}

/// The last leg takes `leg` (what the form shows).
#[uniffi::export]
pub fn plan_set_last_leg(request: PlanInput, leg: LegInput) -> PlanInput {
    match leg_from(&leg) {
        Ok(leg) => edit(request, |r| r.set_last_leg(leg)),
        Err(_) => request,
    }
}

/// Colour of a leg that could not be computed.
#[uniffi::export]
pub fn failed_leg_color() -> String {
    core_routing::FAILED_LEG_COLOR.into()
}
