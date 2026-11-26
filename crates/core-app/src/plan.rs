//! Routes made of legs: every leg between two waypoints has its own profile,
//! stages group the legs between the waypoints the user marked as stage ends.
//! Computed legs are kept, so that a change computes only the legs it touches.

use std::collections::HashMap;
use std::sync::Arc;

use core_maps::{ProfilePoint, RoutingTiles};
use core_routing::{
    DinParams, LatLon, Profile, Route, RouteOptions, RouteRequest, Router, RoutingError, Turn,
    din_time_s, manual_route,
};
use core_track::elevation_gain;
use serde::{Deserialize, Serialize};

use crate::AppError;
use crate::app::{App, Shared};

/// Largest distance between two samples of the elevation profile, in metres.
const PROFILE_STEP_M: f64 = 25.0;
/// Smoothing window for ascent and descent. Terrain models are smooth
/// compared to GPS elevation, a short window is enough.
const ELE_SMOOTH_M: f64 = 50.0;
/// Changes of direction smaller than this do not count as ascent or descent.
const ELE_THRESHOLD_M: f64 = 3.0;
/// A position older than this no longer says where the user is.
pub const MAX_POSITION_AGE_S: f64 = 120.0;
/// Format of [`RoutePlan`] as saved; plans without it are older.
pub const PLAN_FORMAT: u32 = 3;
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

/// The user's position and how old it is.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct KnownPosition {
    pub lat: f64,
    pub lon: f64,
    pub age_s: f64,
}

/// Where a route to a search result can start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteStart {
    /// At the user's position.
    Ready,
    /// No recent position is known.
    NoPosition,
    /// No routing data holds both the position and the destination.
    OtherMap,
}

impl LegError {
    /// Whether the leg can only be joined by hand: there is no routing data
    /// for it or no way was found. Other errors would not go away that way.
    #[must_use]
    pub fn can_connect_straight(&self) -> bool {
        matches!(
            self.code.as_str(),
            "routing_unavailable" | "route_not_found"
        )
    }
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
    /// Where the leg turns off, as indices into `line`. `None` if not computed
    /// (a manual leg, a failed one, or a plan saved before format 3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turns: Option<Vec<Turn>>,
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
    pub(crate) leg: Leg,
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

    /// Replaces the leg at `index`; nothing else changes.
    #[must_use]
    pub fn set_leg(&self, index: usize, leg: LegSpec) -> Self {
        let mut next = self.clone();
        if let Some(slot) = next.legs.get_mut(index) {
            *slot = leg;
        }
        next
    }

    /// Splits leg `index` with a new waypoint at half the length of `line`,
    /// the leg as computed. Without a line of some length the point goes
    /// halfway between the leg's waypoints. Both halves keep the leg's
    /// profile and settings; later stage ends stay at their waypoints.
    #[must_use]
    pub fn split_leg(&self, index: usize, line: &[LatLon]) -> Self {
        let (Some(&leg), Some(&from), Some(&to)) = (
            self.legs.get(index),
            self.waypoints.get(index),
            self.waypoints.get(index + 1),
        ) else {
            return self.clone();
        };
        let at = half_way(line).unwrap_or(LatLon {
            lat: (from.lat + to.lat) / 2.0,
            lon: (from.lon + to.lon) / 2.0,
        });
        let mut next = self.clone();
        next.waypoints.insert(index + 1, at);
        next.legs.insert(index + 1, leg);
        for end in &mut next.stage_ends {
            if *end > index {
                *end += 1;
            }
        }
        next
    }

    /// Only inner waypoints can end a stage.
    fn keep_inner_stage_ends(&mut self) {
        let n = self.waypoints.len();
        self.stage_ends.retain(|&end| end > 0 && end + 1 < n);
    }
}

/// The point at half the length of `line`; `None` for a line without length.
fn half_way(line: &[LatLon]) -> Option<LatLon> {
    let lengths: Vec<f64> = line.windows(2).map(|w| w[0].distance_m(&w[1])).collect();
    let total: f64 = lengths.iter().sum();
    if total <= 0.0 {
        return None;
    }
    let mut left = total / 2.0;
    for (i, &length) in lengths.iter().enumerate() {
        if left <= length && length > 0.0 {
            let t = left / length;
            let (a, b) = (line[i], line[i + 1]);
            return Some(LatLon {
                lat: a.lat + t * (b.lat - a.lat),
                lon: a.lon + t * (b.lon - a.lon),
            });
        }
        left -= length;
    }
    line.last().copied()
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
        turns: None,
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

    /// Whether a route to `target` can start at `position`.
    #[must_use]
    pub fn route_start(&self, position: Option<KnownPosition>, target: LatLon) -> RouteStart {
        let Some(position) = position.filter(|p| p.age_s <= MAX_POSITION_AGE_S) else {
            return RouteStart::NoPosition;
        };
        let points = [(position.lat, position.lon), (target.lat, target.lon)];
        if self.shared.maps.store().routing_tiles(&points).is_some() {
            RouteStart::Ready
        } else {
            RouteStart::OtherMap
        }
    }

    /// Computes a route leg by leg and remembers it as the planned route,
    /// unless a newer plan was begun meanwhile (or the plan was cleared).
    /// Legs computed before with the same inputs are reused. A leg that fails
    /// is marked in the plan; only an invalid request is an error. May take a
    /// moment; call it off the UI thread.
    pub fn plan_route(&self, request: &PlanRequest) -> Result<RoutePlan, AppError> {
        request.validate()?;
        let generation = self.shared.routing.begin_plan();
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
        if self.shared.routing.finish_plan(generation, plan.clone()) {
            self.shared.plan_changed();
        }
        Ok(plan)
    }

    fn leg(&self, from: LatLon, to: LatLon, spec: &LegSpec) -> Result<CachedLeg, AppError> {
        let shared = &self.shared;
        let tiles = shared.leg_tiles(from, to, spec)?;
        let key = leg_key(tiles.as_ref().map(|t| t.region.as_str()), from, to, spec);
        if let Some(cached) = shared.routing.legs().get(&key) {
            return Ok(cached);
        }
        let route = shared.route_on(tiles.as_ref(), from, None, to, spec)?;
        let cached = shared.measure(route, spec, tiles.as_ref())?;
        shared.routing.legs().insert(key, cached.clone());
        Ok(cached)
    }
}

impl Shared {
    /// The routing data a leg is routed on: `None` for a leg joined by hand.
    pub(crate) fn leg_tiles(
        &self,
        from: LatLon,
        to: LatLon,
        spec: &LegSpec,
    ) -> Result<Option<RoutingTiles>, AppError> {
        if spec.profile == Profile::Manual {
            return Ok(None);
        }
        let tiles = self
            .maps
            .store()
            .routing_tiles(&[(from.lat, from.lon), (to.lat, to.lon)])
            .ok_or_else(|| {
                RoutingError::Unavailable(
                    "no installed region with routing data covers this leg".into(),
                )
            })?;
        Ok(Some(tiles))
    }

    /// The way from `from` to `to` on `tiles` (straight without), leaving in
    /// direction `heading_deg` if given. Routing data of another Valhalla
    /// version than the engine's is not routed.
    pub(crate) fn route_on(
        &self,
        tiles: Option<&RoutingTiles>,
        from: LatLon,
        heading_deg: Option<f32>,
        to: LatLon,
        spec: &LegSpec,
    ) -> Result<Route, AppError> {
        let Some(tiles) = tiles else {
            return Ok(manual_route(&[from, to])?);
        };
        let engine = self
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
        let router = self.routing.router(&tiles.region, &tiles.path)?;
        Ok(router.route(&RouteRequest {
            waypoints: vec![from, to],
            profile: spec.profile,
            options: spec.options,
            heading_deg,
        })?)
    }

    /// Elevation, ascent and time of a computed leg.
    pub(crate) fn measure(
        &self,
        route: Route,
        spec: &LegSpec,
        tiles: Option<&RoutingTiles>,
    ) -> Result<CachedLeg, AppError> {
        let line: Vec<(f64, f64)> = route.line.iter().map(|p| (p.lat, p.lon)).collect();
        let elevation = self.maps.store().elevation_profile(&line, PROFILE_STEP_M)?;
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
                region: tiles.map(|t| t.region.clone()),
                max_mtb_scale: tiles.and_then(|t| t.info.as_ref()).map(|i| i.max_mtb_scale),
                turns: route.turns,
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::sync::Arc;

    use core_location::{Fix, SourceStatus};
    use core_routing::TurnKind;

    use super::*;
    use crate::{AppListener, RecordingStatus};

    struct Quiet;

    impl AppListener for Quiet {
        fn on_position(&self, _: &Fix) {}
        fn on_recording(&self, _: &RecordingStatus) {}
        fn on_source_status(&self, _: &SourceStatus) {}
        fn on_error(&self, _: &str) {}
    }

    fn point(lat: f64, lon: f64) -> LatLon {
        LatLon { lat, lon }
    }

    fn spec() -> LegSpec {
        LegSpec {
            profile: Profile::Hike,
            options: RouteOptions::default(),
        }
    }

    #[test]
    fn measured_leg_keeps_turns() {
        let dir = std::env::temp_dir().join(format!("core-app-plan-turns-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let app = App::open(&dir.join("app.sqlite"), Arc::new(Quiet)).unwrap();
        let turns = vec![Turn {
            at: 3,
            kind: TurnKind::Left,
            exit: None,
        }];
        let route = Route {
            profile: Profile::Hike,
            waypoints: vec![point(48.0, 11.0), point(48.0, 11.01)],
            line: (0..5)
                .map(|i| point(48.0, 11.0 + 0.002 * f64::from(i)))
                .collect(),
            distance_m: 745.0,
            duration_s: Some(600.0),
            turns: Some(turns.clone()),
        };
        let measured = app.shared.measure(route, &spec(), None).unwrap();
        assert_eq!(measured.leg.turns, Some(turns));
    }

    #[test]
    fn new_plans_have_format_3() {
        let request = PlanRequest {
            waypoints: vec![point(48.0, 11.0), point(48.0, 11.01)],
            legs: vec![spec()],
            stage_ends: Vec::new(),
        };
        let leg = failed_leg(&spec(), &AppError::RoutePlan("x".into()));
        assert_eq!(leg.turns, None);
        let plan = assemble(&request, vec![leg], &[Vec::new()]);
        assert_eq!(plan.format, 3);
        assert_eq!(PLAN_FORMAT, 3);
    }
}
