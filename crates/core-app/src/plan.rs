//! Routes made of legs: every leg between two waypoints has its own profile,
//! stages group the legs between the waypoints the user marked as stage ends.
//! Computed legs are kept, so that a change computes only the legs it touches.

use std::collections::HashMap;
use std::sync::Arc;

use core_maps::ProfilePoint;
use core_routing::{
    DinParams, LatLon, Profile, Route, RouteOptions, RouteRequest, Router, RoutingError,
    din_time_s, manual_route,
};
use core_track::elevation_gain;
use serde::{Deserialize, Serialize};

use crate::AppError;
use crate::app::App;

/// Largest distance between two samples of the elevation profile, in metres.
const PROFILE_STEP_M: f64 = 25.0;
/// Smoothing window for ascent and descent. Terrain models are smooth
/// compared to GPS elevation, a short window is enough.
const ELE_SMOOTH_M: f64 = 50.0;
/// Changes of direction smaller than this do not count as ascent or descent.
const ELE_THRESHOLD_M: f64 = 3.0;
/// Format of [`RoutePlan`] as saved; plans without it are older.
pub const PLAN_FORMAT: u32 = 2;
/// More computed legs than this are forgotten all at once.
const MAX_CACHED_LEGS: usize = 5000;

/// How one leg is to be found.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LegSpec {
    pub profile: Profile,
    #[serde(default)]
    pub options: RouteOptions,
}

/// A route as the user has set it up.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanRequest {
    pub waypoints: Vec<LatLon>,
    /// Leg `i` runs from waypoint `i` to waypoint `i + 1`.
    pub legs: Vec<LegSpec>,
    /// Inner waypoints where a stage ends, ascending.
    #[serde(default)]
    pub stage_ends: Vec<usize>,
}

/// Why a leg could not be computed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegError {
    /// As [`AppError::code`].
    pub code: String,
    pub message: String,
}

/// One computed leg. A failed leg has no line and adds nothing to the totals.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Leg {
    pub profile: Profile,
    pub options: RouteOptions,
    pub line: Vec<LatLon>,
    pub distance_m: f64,
    /// Travel time as estimated by the router.
    pub duration_s: Option<f64>,
    /// `None` without elevation data along the leg.
    pub ascent_m: Option<f64>,
    pub descent_m: Option<f64>,
    /// On foot the walking time after DIN 33466, otherwise the router's time.
    pub time_s: Option<f64>,
    /// The region whose routing data was used.
    pub region: Option<String>,
    /// Ways above this `mtb:scale` are closed to bicycles in that routing data.
    pub max_mtb_scale: Option<u8>,
    pub error: Option<LegError>,
}

/// A sample of the elevation profile along the whole route.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RoutePoint {
    /// From the start of the route.
    pub distance_m: f64,
    pub lat: f64,
    pub lon: f64,
    pub ele: Option<f32>,
    /// Index of the leg the point lies on.
    pub leg: usize,
}

/// The legs between two stage ends (or the start or destination).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stage {
    /// First and last waypoint of the stage.
    pub from: usize,
    pub to: usize,
    pub distance_m: f64,
    pub ascent_m: Option<f64>,
    pub descent_m: Option<f64>,
    pub time_s: Option<f64>,
    /// The profiles of its legs, in order of first appearance.
    pub profiles: Vec<Profile>,
}

/// The whole route: the sum of its stages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Totals {
    pub distance_m: f64,
    pub ascent_m: Option<f64>,
    pub descent_m: Option<f64>,
    pub time_s: Option<f64>,
    pub min_ele_m: Option<f64>,
    pub max_ele_m: Option<f64>,
    /// Legs that could not be computed and are missing from the totals.
    pub failed_legs: usize,
}

/// A planned route with everything shown about it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutePlan {
    /// [`PLAN_FORMAT`].
    pub format: u32,
    pub request: PlanRequest,
    pub legs: Vec<Leg>,
    pub elevation: Vec<RoutePoint>,
    pub stages: Vec<Stage>,
    pub total: Totals,
}

impl RoutePlan {
    /// The plan with an elevation profile of about `max_points` samples, for
    /// showing it: a chart needs no sample every 25 m of a long route. The
    /// first sample and the last one of every leg are always kept.
    #[must_use]
    pub fn for_display(&self, max_points: usize) -> RoutePlan {
        let samples = &self.elevation;
        if samples.len() <= max_points {
            return self.clone();
        }
        let stride = samples.len().div_ceil(max_points.max(1));
        let elevation = samples
            .iter()
            .enumerate()
            .filter(|&(i, point)| {
                let leg_ends = samples.get(i + 1).is_none_or(|next| next.leg != point.leg);
                i % stride == 0 || leg_ends
            })
            .map(|(_, point)| *point)
            .collect();
        RoutePlan {
            format: self.format,
            request: self.request.clone(),
            legs: self.legs.clone(),
            elevation,
            stages: self.stages.clone(),
            total: self.total.clone(),
        }
    }
}

/// A computed leg with its elevation samples (distances from the leg's start).
#[derive(Clone)]
pub(crate) struct CachedLeg {
    leg: Leg,
    elevation: Vec<ProfilePoint>,
}

/// Computed legs by everything their result depends on.
#[derive(Default)]
pub(crate) struct LegCache {
    legs: HashMap<String, CachedLeg>,
}

impl LegCache {
    pub(crate) fn clear(&mut self) {
        self.legs.clear();
    }

    fn get(&self, key: &str) -> Option<CachedLeg> {
        self.legs.get(key).cloned()
    }

    fn insert(&mut self, key: String, leg: CachedLeg) {
        if self.legs.len() >= MAX_CACHED_LEGS {
            self.legs.clear();
        }
        self.legs.insert(key, leg);
    }
}

fn leg_key(region: Option<&str>, from: LatLon, to: LatLon, spec: &LegSpec) -> String {
    let options = serde_json::to_string(&spec.options).unwrap_or_default();
    format!(
        "{}|{:.7},{:.7}|{:.7},{:.7}|{}|{options}",
        region.unwrap_or(""),
        from.lat,
        from.lon,
        to.lat,
        to.lon,
        spec.profile.as_str(),
    )
}

fn invalid(message: &str) -> AppError {
    AppError::RoutePlan(message.to_string())
}

impl PlanRequest {
    /// No waypoints, no legs.
    #[must_use]
    pub fn empty() -> Self {
        PlanRequest {
            waypoints: Vec::new(),
            legs: Vec::new(),
            stage_ends: Vec::new(),
        }
    }

    /// Whether the request can be computed: start and destination, one leg
    /// between each two waypoints, stage ends at inner waypoints in order.
    pub fn validate(&self) -> Result<(), AppError> {
        let n = self.waypoints.len();
        if n < 2 {
            return Err(RoutingError::TooFewWaypoints.into());
        }
        if self.legs.len() != n - 1 {
            return Err(invalid("there must be one leg less than waypoints"));
        }
        let inner = |i: &usize| (1..n - 1).contains(i);
        if !self.stage_ends.iter().all(inner) || !self.stage_ends.windows(2).all(|w| w[0] < w[1]) {
            return Err(invalid(
                "stage ends must be inner waypoints in ascending order",
            ));
        }
        Ok(())
    }

    /// Adds a destination; the leg to it is `leg` (what the form shows).
    #[must_use]
    pub fn add_waypoint(&self, at: LatLon, leg: LegSpec) -> Self {
        let mut next = self.clone();
        if !next.waypoints.is_empty() {
            next.legs.push(leg);
        }
        next.waypoints.push(at);
        next
    }

    #[must_use]
    pub fn move_waypoint(&self, index: usize, to: LatLon) -> Self {
        let mut next = self.clone();
        if let Some(point) = next.waypoints.get_mut(index) {
            *point = to;
        }
        next
    }

    /// Removes a waypoint. Its neighbours are joined by the leg before it (the
    /// first point takes its leg along); a stage ending there ends no more.
    #[must_use]
    pub fn remove_waypoint(&self, index: usize) -> Self {
        if index >= self.waypoints.len() {
            return self.clone();
        }
        let mut next = self.clone();
        next.waypoints.remove(index);
        if !next.legs.is_empty() {
            let leg = if index == 0 {
                0
            } else if index - 1 == next.legs.len() - 1 {
                index - 1
            } else {
                index
            };
            next.legs.remove(leg);
        }
        next.stage_ends = next
            .stage_ends
            .iter()
            .filter(|&&end| end != index)
            .map(|&end| if end > index { end - 1 } else { end })
            .collect();
        next.keep_inner_stage_ends();
        next
    }

    /// Marks or unmarks an inner waypoint as the end of a stage.
    #[must_use]
    pub fn toggle_stage_end(&self, index: usize) -> Self {
        let mut next = self.clone();
        if index == 0 || index + 1 >= next.waypoints.len() {
            return next;
        }
        if let Some(at) = next.stage_ends.iter().position(|&end| end == index) {
            next.stage_ends.remove(at);
        } else {
            next.stage_ends.push(index);
            next.stage_ends.sort_unstable();
        }
        next
    }

    /// The form changed: the last leg follows it.
    #[must_use]
    pub fn set_last_leg(&self, leg: LegSpec) -> Self {
        let mut next = self.clone();
        if let Some(last) = next.legs.last_mut() {
            *last = leg;
        }
        next
    }

    /// Only inner waypoints can end a stage.
    fn keep_inner_stage_ends(&mut self) {
        let n = self.waypoints.len();
        self.stage_ends.retain(|&end| end > 0 && end + 1 < n);
    }
}

fn on_foot(profile: Profile) -> bool {
    matches!(profile, Profile::Hike | Profile::Manual)
}

/// Adds up values that are all known; `None` if one is unknown or there is none.
fn sum_known(values: impl Iterator<Item = Option<f64>>) -> Option<f64> {
    let mut sum = None;
    for value in values {
        sum = Some(sum.unwrap_or(0.0) + value?);
    }
    sum
}

fn failed_leg(spec: &LegSpec, error: &AppError) -> Leg {
    Leg {
        profile: spec.profile,
        options: spec.options,
        line: Vec::new(),
        distance_m: 0.0,
        duration_s: None,
        ascent_m: None,
        descent_m: None,
        time_s: None,
        region: None,
        max_mtb_scale: None,
        error: Some(LegError {
            code: error.code().to_string(),
            message: error.to_string(),
        }),
    }
}

impl App {
    /// Puts a router in place of the region's routing data; for tests.
    #[doc(hidden)]
    pub fn set_router_for_tests(&self, region: &str, router: Arc<dyn Router>) {
        self.shared
            .routing
            .routers()
            .insert(region.to_string(), router);
    }

    /// Computes a route leg by leg and remembers it as the planned route.
    /// Legs computed before with the same inputs are reused. A leg that fails
    /// is marked in the plan; only an invalid request is an error. May take a
    /// moment; call it off the UI thread.
    pub fn plan_route(&self, request: &PlanRequest) -> Result<RoutePlan, AppError> {
        request.validate()?;
        let mut legs = Vec::with_capacity(request.legs.len());
        let mut samples = Vec::with_capacity(request.legs.len());
        for (i, spec) in request.legs.iter().enumerate() {
            let (from, to) = (request.waypoints[i], request.waypoints[i + 1]);
            match self.leg(from, to, spec) {
                Ok(cached) => {
                    legs.push(cached.leg);
                    samples.push(cached.elevation);
                }
                // Problems with the elevation data concern the whole route.
                Err(error @ AppError::Maps(_)) => return Err(error),
                Err(error) => {
                    legs.push(failed_leg(spec, &error));
                    samples.push(Vec::new());
                }
            }
        }
        let plan = assemble(request, legs, &samples);
        *self.shared.routing.planned() = Some(plan.clone());
        Ok(plan)
    }

    fn leg(&self, from: LatLon, to: LatLon, spec: &LegSpec) -> Result<CachedLeg, AppError> {
        let tiles = if spec.profile == Profile::Manual {
            None
        } else {
            Some(
                self.shared
                    .maps
                    .store()
                    .routing_tiles(&[(from.lat, from.lon), (to.lat, to.lon)])
                    .ok_or_else(|| {
                        RoutingError::Unavailable(
                            "no installed region with routing data covers this leg".into(),
                        )
                    })?,
            )
        };
        let key = leg_key(tiles.as_ref().map(|t| t.region.as_str()), from, to, spec);
        if let Some(cached) = self.shared.routing.legs().get(&key) {
            return Ok(cached);
        }
        let route = match &tiles {
            None => manual_route(&[from, to])?,
            Some(tiles) => {
                let engine = self
                    .shared
                    .routing
                    .factory()
                    .and_then(|factory| factory.engine_version());
                if let (Some(info), Some(engine)) = (&tiles.info, engine)
                    && info.valhalla != engine
                {
                    return Err(RoutingError::Incompatible {
                        data: info.valhalla.clone(),
                        engine,
                    }
                    .into());
                }
                let router = self.shared.routing.router(&tiles.region, &tiles.path)?;
                router.route(&RouteRequest {
                    waypoints: vec![from, to],
                    profile: spec.profile,
                    options: spec.options,
                })?
            }
        };
        let cached = self.measure(
            route,
            spec,
            tiles.as_ref().map(|t| t.region.clone()),
            tiles
                .as_ref()
                .and_then(|t| t.info.as_ref())
                .map(|i| i.max_mtb_scale),
        )?;
        self.shared.routing.legs().insert(key, cached.clone());
        Ok(cached)
    }

    /// Elevation, ascent and time of a computed leg.
    fn measure(
        &self,
        route: Route,
        spec: &LegSpec,
        region: Option<String>,
        max_mtb_scale: Option<u8>,
    ) -> Result<CachedLeg, AppError> {
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
        let complete = !known.is_empty() && known.len() == elevation.len();
        let (ascent_m, descent_m) = if !complete {
            (None, None)
        } else if known.len() < 2 {
            // A leg of no length (both ends snapped to the same place).
            (Some(0.0), Some(0.0))
        } else {
            let (up, down) = elevation_gain(&known, ELE_SMOOTH_M, ELE_THRESHOLD_M);
            (Some(up), Some(down))
        };
        let time_s = match (on_foot(spec.profile), ascent_m, descent_m) {
            (true, Some(up), Some(down)) => Some(din_time_s(
                route.distance_m,
                up,
                down,
                &DinParams::default(),
            )),
            _ => route.duration_s,
        };
        Ok(CachedLeg {
            leg: Leg {
                profile: spec.profile,
                options: spec.options,
                line: route.line,
                distance_m: route.distance_m,
                duration_s: route.duration_s,
                ascent_m,
                descent_m,
                time_s,
                region,
                max_mtb_scale,
                error: None,
            },
            elevation,
        })
    }
}

/// Puts the legs together: one elevation profile, the stages and the totals.
fn assemble(request: &PlanRequest, legs: Vec<Leg>, samples: &[Vec<ProfilePoint>]) -> RoutePlan {
    let mut elevation = Vec::new();
    let mut start = 0.0;
    for (index, (leg, points)) in legs.iter().zip(samples).enumerate() {
        elevation.extend(points.iter().map(|p| RoutePoint {
            distance_m: start + p.distance_m,
            lat: p.lat,
            lon: p.lon,
            ele: p.ele,
            leg: index,
        }));
        // The samples measure the line themselves; continue where they end.
        start += points.last().map_or(leg.distance_m, |p| p.distance_m);
    }

    let mut bounds = vec![0];
    bounds.extend(&request.stage_ends);
    bounds.push(request.waypoints.len() - 1);
    let stages: Vec<Stage> = bounds
        .windows(2)
        .map(|w| stage(w[0], w[1], &legs[w[0]..w[1]]))
        .collect();

    let complete = legs
        .iter()
        .filter(|leg| leg.error.is_none())
        .all(|leg| leg.ascent_m.is_some());
    let eles = || elevation.iter().filter_map(|p| p.ele.map(f64::from));
    let total = Totals {
        distance_m: stages.iter().map(|s| s.distance_m).sum(),
        ascent_m: sum_known(stages.iter().map(|s| s.ascent_m)),
        descent_m: sum_known(stages.iter().map(|s| s.descent_m)),
        time_s: sum_known(stages.iter().map(|s| s.time_s)),
        min_ele_m: eles().reduce(f64::min).filter(|_| complete),
        max_ele_m: eles().reduce(f64::max).filter(|_| complete),
        failed_legs: legs.iter().filter(|leg| leg.error.is_some()).count(),
    };
    RoutePlan {
        format: PLAN_FORMAT,
        request: request.clone(),
        legs,
        elevation,
        stages,
        total,
    }
}

fn stage(from: usize, to: usize, legs: &[Leg]) -> Stage {
    let computed = || legs.iter().filter(|leg| leg.error.is_none());
    let mut profiles = Vec::new();
    for leg in legs {
        if !profiles.contains(&leg.profile) {
            profiles.push(leg.profile);
        }
    }
    Stage {
        from,
        to,
        distance_m: computed().map(|leg| leg.distance_m).sum(),
        ascent_m: sum_known(computed().map(|leg| leg.ascent_m)),
        descent_m: sum_known(computed().map(|leg| leg.descent_m)),
        time_s: sum_known(computed().map(|leg| leg.time_s)),
        profiles,
    }
}
