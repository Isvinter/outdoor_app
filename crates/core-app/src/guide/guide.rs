//! Following a planned route fix by fix: progress along it, leaving it, the
//! way back to the next waypoint and what remains to go.

use core_location::Fix;
use core_routing::LatLon;

use super::line::{LegKind, RouteLine};
use crate::error::AppError;
use crate::plan::LegSpec;

/// How far the search for the progress reaches back behind it.
const WINDOW_BACK_M: f64 = 200.0;
/// How far the search for the progress reaches ahead of it at least.
const WINDOW_AHEAD_M: f64 = 500.0;
/// Fastest plausible travel (downhill on a bike). After a gap between fixes,
/// the window reaches three times as far ahead as this speed would go.
const MAX_SPEED_MPS: f64 = 20.0;
/// Below this speed the course over ground is too noisy to pass on.
const MIN_HEADING_SPEED_MPS: f32 = 1.5;

/// Thresholds of the guide; see "Live-Routing" in `docs/ALGORITHMEN.md`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GuideParams {
    /// A fix farther than this from the active line counts as off it.
    pub off_route_m: f64,
    /// Off the line for at least this many fixes in a row …
    pub off_route_fixes: u32,
    /// … and at least this long is a deviation.
    pub off_route_min_s: f64,
    /// Closer than this to the planned line is back on the route.
    pub back_on_m: f64,
    /// Fixes less accurate than this are ignored.
    pub max_accuracy_m: f64,
    /// After a failed way back, try again after this long …
    pub retry_after_s: f64,
    /// … or after moving this far.
    pub retry_after_m: f64,
    /// Within this distance of the end, the destination is reached.
    pub arrived_m: f64,
}

impl Default for GuideParams {
    fn default() -> GuideParams {
        GuideParams {
            off_route_m: 40.0,
            off_route_fixes: 3,
            off_route_min_s: 10.0,
            back_on_m: 20.0,
            max_accuracy_m: 50.0,
            retry_after_s: 60.0,
            retry_after_m: 200.0,
            arrived_m: 30.0,
        }
    }
}

/// A routed way from where the user left the route to a waypoint on it.
#[derive(Debug, Clone, PartialEq)]
pub struct WayBack {
    pub line: Vec<LatLon>,
    pub distance_m: f64,
    pub time_s: Option<f64>,
}

/// Finds a way back to the route.
pub trait WayBackRouter {
    /// A way from `from` to `to` for `leg`'s profile and options, leaving in
    /// direction `heading_deg` if given.
    fn way_back(
        &self,
        from: LatLon,
        heading_deg: Option<f32>,
        to: LatLon,
        leg: LegSpec,
    ) -> Result<WayBack, AppError>;
}

/// Chooses the waypoint a way back leads to.
pub trait WayBackTarget {
    /// Index of the waypoint for progress `along_m` on `line`.
    fn target(&self, line: &RouteLine, along_m: f64) -> Option<usize>;
}

/// The first waypoint not yet reached.
#[derive(Debug, Clone, Copy, Default)]
pub struct NextWaypoint;

impl WayBackTarget for NextWaypoint {
    fn target(&self, line: &RouteLine, along_m: f64) -> Option<usize> {
        line.waypoint_m.iter().position(|&w| w > along_m)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuidePhase {
    /// No usable fix yet.
    Waiting,
    OnRoute,
    /// Off the route, with a way back.
    OffRoute,
    /// Off the route; no way back was found.
    NoWayBack,
    Arrived,
}

/// Distance and time still to go.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Remaining {
    pub distance_m: f64,
    pub time_s: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GuideStatus {
    pub phase: GuidePhase,
    pub along_m: f64,
    /// Distance from the last fix to the active line.
    pub offset_m: Option<f64>,
    pub next_waypoint: Option<usize>,
    /// Counts up with every new deviation; Trail tells the user when it grows.
    pub deviation: u32,
    pub way_back: Option<WayBack>,
    pub to_stage_end: Option<Remaining>,
    pub to_destination: Remaining,
}

/// A way back being followed.
struct ActiveWayBack {
    way: WayBack,
    line: RouteLine,
    /// Waypoint it leads to.
    target: usize,
    /// Progress along it.
    along_m: f64,
}

/// Fixes off the active line in a row.
#[derive(Clone, Copy)]
struct OffCount {
    fixes: u32,
    since_ms: i64,
}

/// Where and when a way back last failed.
#[derive(Clone, Copy)]
struct Attempt {
    at: LatLon,
    time_ms: i64,
}

/// Follows one planned route. It never changes the route.
pub struct Guide {
    line: RouteLine,
    params: GuideParams,
    target_rule: Box<dyn WayBackTarget + Send>,
    phase: GuidePhase,
    /// The progress is known, from a fix or a chosen waypoint.
    placed: bool,
    along_m: f64,
    offset_m: Option<f64>,
    last_fix: Option<(LatLon, i64)>,
    off: Option<OffCount>,
    deviation: u32,
    way_back: Option<ActiveWayBack>,
    failed: Option<Attempt>,
}

impl Guide {
    /// A guide along `line` that leads back to the next waypoint.
    #[must_use]
    pub fn new(line: RouteLine, params: GuideParams) -> Guide {
        Guide::with_target(line, params, Box::new(NextWaypoint))
    }

    /// A guide along `line` whose ways back lead where `target_rule` says.
    #[must_use]
    pub fn with_target(
        line: RouteLine,
        params: GuideParams,
        target_rule: Box<dyn WayBackTarget + Send>,
    ) -> Guide {
        Guide {
            line,
            params,
            target_rule,
            phase: GuidePhase::Waiting,
            placed: false,
            along_m: 0.0,
            offset_m: None,
            last_fix: None,
            off: None,
            deviation: 0,
            way_back: None,
            failed: None,
        }
    }

    /// Starts the progress at waypoint `index` instead of at the nearest
    /// point of the whole route; the next fix is searched around it.
    pub fn start_from_waypoint(&mut self, index: usize) {
        let Some(&at_m) = self.line.waypoint_m.get(index) else {
            return;
        };
        self.along_m = at_m;
        self.placed = true;
        self.phase = GuidePhase::Waiting;
        self.offset_m = None;
        self.last_fix = None;
        self.off = None;
        self.way_back = None;
        self.failed = None;
    }

    /// Takes in the next fix; asks `router` for a way back on a deviation.
    pub fn update(&mut self, fix: &Fix, router: &dyn WayBackRouter) -> GuideStatus {
        let usable = fix.lat.is_finite()
            && fix.lon.is_finite()
            && fix
                .h_accuracy_m
                .is_none_or(|a| f64::from(a) <= self.params.max_accuracy_m);
        if self.phase == GuidePhase::Arrived || !usable {
            return self.status();
        }
        let p = LatLon {
            lat: fix.lat,
            lon: fix.lon,
        };
        let (moved_m, dt_s) = self.last_fix.map_or((0.0, 0.0), |(q, t)| {
            (
                q.distance_m(&p),
                ((fix.time_ms - t) as f64 / 1000.0).max(0.0),
            )
        });
        let (window, hint_m) = if self.placed {
            (Some(self.window(dt_s)), self.along_m + moved_m)
        } else {
            // On a loop, start and end are equally near: take the start.
            (None, 0.0)
        };
        let Some(on_plan) = self.line.project_near(p, window, Some(hint_m)) else {
            return self.status();
        };
        // The first fix sets the progress, also at a waypoint chosen before.
        let first = self.last_fix.is_none();
        self.last_fix = Some((p, fix.time_ms));
        self.placed = true;
        if self.phase == GuidePhase::Waiting {
            self.phase = GuidePhase::OnRoute;
        }
        let free = self
            .line
            .legs
            .get(on_plan.leg)
            .is_none_or(|leg| leg.kind != LegKind::Routed);

        match self.phase {
            GuidePhase::OffRoute | GuidePhase::NoWayBack
                if on_plan.offset_m < self.params.back_on_m =>
            {
                self.phase = GuidePhase::OnRoute;
                self.way_back = None;
                self.failed = None;
                self.off = None;
                self.along_m = on_plan.along_m;
                self.offset_m = Some(on_plan.offset_m);
            }
            GuidePhase::OffRoute => {
                let on_way = self
                    .way_back
                    .as_mut()
                    .and_then(|wb| wb.line.project(p, None).map(|hit| (wb, hit)));
                if let Some((wb, hit)) = on_way {
                    wb.along_m = hit.along_m;
                    self.offset_m = Some(hit.offset_m);
                    if self.count_off(hit.offset_m > self.params.off_route_m, fix.time_ms) {
                        self.deviation += 1;
                        self.route_back(p, fix, router);
                    }
                }
            }
            GuidePhase::NoWayBack => {
                self.offset_m = Some(on_plan.offset_m);
                let due = self.failed.is_none_or(|a| {
                    (fix.time_ms - a.time_ms) as f64 / 1000.0 >= self.params.retry_after_s
                        || a.at.distance_m(&p) >= self.params.retry_after_m
                });
                if due {
                    self.route_back(p, fix, router);
                }
            }
            GuidePhase::Waiting | GuidePhase::OnRoute | GuidePhase::Arrived => {
                let near = on_plan.offset_m <= self.params.off_route_m;
                self.offset_m = Some(on_plan.offset_m);
                if first || near || free {
                    self.along_m = on_plan.along_m;
                }
                if self.count_off(!near && !free, fix.time_ms) {
                    self.deviation += 1;
                    self.route_back(p, fix, router);
                }
            }
        }

        if self.phase == GuidePhase::OnRoute
            && self.along_m >= self.line.length_m - self.params.arrived_m
            && on_plan.offset_m < self.params.off_route_m
        {
            self.phase = GuidePhase::Arrived;
        }
        self.status()
    }

    /// Where the guide stands now.
    #[must_use]
    pub fn status(&self) -> GuideStatus {
        let next_waypoint = match &self.way_back {
            Some(wb) => Some(wb.target),
            None if self.placed => self.target_rule.target(&self.line, self.along_m),
            None => None,
        };
        let (from_m, back) = match &self.way_back {
            Some(wb) => {
                let left_m = (wb.line.length_m - wb.along_m).max(0.0);
                let share = if wb.line.length_m > 0.0 {
                    left_m / wb.line.length_m
                } else {
                    0.0
                };
                let back = Remaining {
                    distance_m: wb.way.distance_m * share,
                    time_s: wb.way.time_s.map(|t| t * share),
                };
                (self.waypoint_at(wb.target), Some(back))
            }
            None => (self.along_m, None),
        };
        let with_back = |planned: Remaining| match back {
            None => planned,
            Some(back) => Remaining {
                distance_m: back.distance_m + planned.distance_m,
                time_s: back.time_s.zip(planned.time_s).map(|(a, b)| a + b),
            },
        };
        let to_destination = with_back(self.planned(from_m, self.line.length_m));
        let to_stage_end = if self.line.stage_end_m.is_empty() {
            None
        } else {
            Some(
                match self.line.stage_end_m.iter().find(|&&s| s > self.along_m) {
                    Some(&end_m) => with_back(self.planned(from_m, end_m)),
                    None => to_destination,
                },
            )
        };
        GuideStatus {
            phase: self.phase,
            along_m: self.along_m,
            offset_m: self.offset_m,
            next_waypoint,
            deviation: self.deviation,
            way_back: self.way_back.as_ref().map(|wb| wb.way.clone()),
            to_stage_end,
            to_destination,
        }
    }

    /// The part of the planned line searched for the next fix, `dt_s` after
    /// the last one.
    fn window(&self, dt_s: f64) -> (f64, f64) {
        let mut hi_m = self.along_m + WINDOW_AHEAD_M.max(3.0 * dt_s * MAX_SPEED_MPS);
        // Across a straight or missing leg the user may be far from its
        // line; let them come back anywhere on it and just after it.
        if let Some(leg) = self.current_leg()
            && self.line.legs[leg].kind != LegKind::Routed
        {
            hi_m = hi_m.max(self.line.legs[leg].to_m + WINDOW_AHEAD_M);
        }
        // Off the route, the user may come back anywhere up to the waypoint
        // the way back leads to.
        if matches!(self.phase, GuidePhase::OffRoute | GuidePhase::NoWayBack) {
            let target = self
                .way_back
                .as_ref()
                .map(|wb| wb.target)
                .or_else(|| self.target_rule.target(&self.line, self.along_m));
            if let Some(target) = target {
                hi_m = hi_m.max(self.waypoint_at(target) + WINDOW_AHEAD_M);
            }
        }
        (self.along_m - WINDOW_BACK_M, hi_m)
    }

    /// Counts a fix that is `off` the active line or not; true when that
    /// makes a deviation.
    fn count_off(&mut self, off: bool, time_ms: i64) -> bool {
        if !off {
            self.off = None;
            return false;
        }
        let count = self.off.get_or_insert(OffCount {
            fixes: 0,
            since_ms: time_ms,
        });
        count.fixes += 1;
        let deviates = count.fixes >= self.params.off_route_fixes
            && (time_ms - count.since_ms) as f64 / 1000.0 >= self.params.off_route_min_s;
        if deviates {
            self.off = None;
        }
        deviates
    }

    /// Asks `router` for a way from `p` back to the route.
    fn route_back(&mut self, p: LatLon, fix: &Fix, router: &dyn WayBackRouter) {
        self.way_back = None;
        self.failed = Some(Attempt {
            at: p,
            time_ms: fix.time_ms,
        });
        self.phase = GuidePhase::NoWayBack;
        let Some(target) = self.target_rule.target(&self.line, self.along_m) else {
            return;
        };
        let Some(spec) = self.way_back_spec() else {
            return;
        };
        let to = self.line.point_at(self.waypoint_at(target));
        let Ok(way) = router.way_back(p, heading(fix), to, spec) else {
            return;
        };
        if way.line.len() < 2 {
            return;
        }
        let line = RouteLine::from_points(&way.line, spec, way.time_s);
        let hit = line.project(p, None);
        self.offset_m = hit.map(|h| h.offset_m);
        self.way_back = Some(ActiveWayBack {
            way,
            line,
            target,
            along_m: hit.map_or(0.0, |h| h.along_m),
        });
        self.failed = None;
        self.phase = GuidePhase::OffRoute;
    }

    /// The profile of the leg at the progress if routed, else of the last
    /// routed leg before it, else of the first one after it.
    fn way_back_spec(&self) -> Option<LegSpec> {
        let legs = &self.line.legs;
        let at = self.current_leg()?;
        let routed = |i: &usize| legs[*i].kind == LegKind::Routed;
        let leg = Some(at)
            .filter(routed)
            .or_else(|| (0..at).rev().find(routed))
            .or_else(|| (at + 1..legs.len()).find(routed))?;
        Some(legs[leg].spec)
    }

    /// The leg the progress lies on.
    fn current_leg(&self) -> Option<usize> {
        let legs = &self.line.legs;
        legs.iter()
            .position(|leg| self.along_m < leg.to_m)
            .or_else(|| legs.len().checked_sub(1))
    }

    fn waypoint_at(&self, index: usize) -> f64 {
        self.line
            .waypoint_m
            .get(index)
            .copied()
            .unwrap_or(self.line.length_m)
    }

    /// The planned line from `from_m` to `to_m`, its time shared out by
    /// distance within each leg.
    fn planned(&self, from_m: f64, to_m: f64) -> Remaining {
        let mut time_s = Some(0.0);
        for leg in &self.line.legs {
            let overlap_m = leg.to_m.min(to_m) - leg.from_m.max(from_m);
            if overlap_m <= 0.0 {
                continue;
            }
            let share = overlap_m / (leg.to_m - leg.from_m);
            time_s = time_s.zip(leg.time_s).map(|(t, leg_s)| t + leg_s * share);
        }
        Remaining {
            distance_m: (to_m - from_m).max(0.0),
            time_s,
        }
    }
}

/// The course of `fix`, if it is moving fast enough for it to mean anything.
/// CoreLocation reports an unknown course as -1.
fn heading(fix: &Fix) -> Option<f32> {
    let moving = fix.speed_mps.is_some_and(|s| s >= MIN_HEADING_SPEED_MPS);
    fix.course_deg
        .filter(|c| moving && c.is_finite() && (0.0..360.0).contains(c))
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use core_routing::{Profile, RoutingError};

    use super::*;
    use crate::guide::testing::*;

    const A: LatLon = LatLon {
        lat: 47.6,
        lon: 11.7,
    };

    struct Call {
        from: LatLon,
        to: LatLon,
        leg: LegSpec,
    }

    /// Returns a straight way back, or an error if `fail` is set.
    #[derive(Default)]
    struct FakeRouter {
        calls: RefCell<Vec<Call>>,
        fail: Cell<bool>,
    }

    impl FakeRouter {
        fn count(&self) -> usize {
            self.calls.borrow().len()
        }
    }

    impl WayBackRouter for FakeRouter {
        fn way_back(
            &self,
            from: LatLon,
            _heading_deg: Option<f32>,
            to: LatLon,
            leg: LegSpec,
        ) -> Result<WayBack, AppError> {
            self.calls.borrow_mut().push(Call { from, to, leg });
            if self.fail.get() {
                return Err(RoutingError::NoRoute("off the map".into()).into());
            }
            let distance_m = from.distance_m(&to);
            Ok(WayBack {
                line: vec![from, to],
                distance_m,
                time_s: Some(distance_m),
            })
        }
    }

    fn fix(p: LatLon, t_s: f64) -> Fix {
        Fix {
            lat: p.lat,
            lon: p.lon,
            ele: None,
            time_ms: (t_s * 1000.0) as i64,
            h_accuracy_m: Some(5.0),
            speed_mps: None,
            course_deg: None,
        }
    }

    /// A route due east from [`A`] with legs of `legs` (profile, metres).
    fn east_route(legs: &[(Profile, f64)], stage_ends: Vec<usize>) -> RouteLine {
        let mut waypoints = vec![A];
        let mut at_m = 0.0;
        let mut plan_legs = Vec::new();
        for &(profile, m) in legs {
            let from = east(A, at_m);
            at_m += m;
            waypoints.push(east(A, at_m));
            plan_legs.push(leg(profile, vec![from, east(A, at_m)]));
        }
        RouteLine::from_plan(&plan(waypoints, plan_legs, stage_ends))
    }

    /// `north_m` north of the point `x_m` east of [`A`].
    fn at(x_m: f64, north_m: f64) -> LatLon {
        north(east(A, x_m), north_m)
    }

    /// Feeds fixes at `(x_m, north_m, t_s)`; the status after the last one.
    fn walk(
        guide: &mut Guide,
        router: &FakeRouter,
        fixes: impl IntoIterator<Item = (f64, f64, f64)>,
    ) -> GuideStatus {
        let mut status = guide.status();
        for (x_m, north_m, t_s) in fixes {
            status = guide.update(&fix(at(x_m, north_m), t_s), router);
        }
        status
    }

    /// On the line from `from_m` to `to_m` in 50 m steps, one fix a second
    /// from `t_s`.
    fn along(from_m: f64, to_m: f64, t_s: f64) -> impl Iterator<Item = (f64, f64, f64)> {
        let steps = ((to_m - from_m) / 50.0).round() as usize;
        (0..=steps).map(move |i| (from_m + 50.0 * i as f64, 0.0, t_s + i as f64))
    }

    fn two_legs() -> RouteLine {
        east_route(
            &[(Profile::Hike, 1000.0), (Profile::Hike, 1000.0)],
            Vec::new(),
        )
    }

    /// Walks to 300 m and leaves the route to the north until a way back is
    /// found at 20 s.
    fn deviate(guide: &mut Guide, router: &FakeRouter) -> GuideStatus {
        walk(guide, router, along(0.0, 300.0, 0.0));
        walk(
            guide,
            router,
            [
                (320.0, 45.0, 10.0),
                (330.0, 45.0, 12.0),
                (335.0, 45.0, 14.0),
                (340.0, 45.0, 20.0),
            ],
        )
    }

    #[test]
    fn on_route_never_calls_router() {
        let points = track_points(SYLVENSTEIN);
        let n = points.len();
        let line = RouteLine::from_plan(&plan_along(&points, &[n / 3, 2 * n / 3], Vec::new()));
        let mut guide = Guide::new(line, GuideParams::default());
        let router = FakeRouter::default();
        // Played back with the recorded times, gaps included.
        let doc = core_gpx::parse_gpx(SYLVENSTEIN).unwrap();
        let recorded = doc
            .tracks
            .iter()
            .flat_map(|t| &t.segments)
            .flat_map(|s| &s.points);
        let mut status = guide.status();
        for (i, point) in recorded.enumerate() {
            let fix = Fix {
                time_ms: point.time_ms.unwrap(),
                ..fix(points[i], 0.0)
            };
            status = guide.update(&fix, &router);
            assert_eq!(status.deviation, 0, "fix {i}: {status:?}");
        }
        assert_eq!(router.count(), 0);
        assert_eq!(status.phase, GuidePhase::Arrived);
    }

    #[test]
    fn deviation_after_three_fixes_and_ten_seconds() {
        let line = two_legs();
        let wp1 = line.point_at(line.waypoint_m[1]);
        let mut guide = Guide::new(line, GuideParams::default());
        let router = FakeRouter::default();
        walk(&mut guide, &router, along(0.0, 300.0, 0.0));
        let status = walk(
            &mut guide,
            &router,
            [
                (320.0, 45.0, 10.0),
                (330.0, 45.0, 12.0),
                (335.0, 45.0, 14.0),
            ],
        );
        assert_eq!(status.phase, GuidePhase::OnRoute);
        assert_eq!(status.deviation, 0);
        assert_eq!(router.count(), 0);

        let status = walk(&mut guide, &router, [(340.0, 45.0, 20.0)]);
        assert_eq!(status.phase, GuidePhase::OffRoute);
        assert_eq!(status.deviation, 1);
        assert_eq!(status.next_waypoint, Some(1));
        assert!(status.way_back.is_some());
        assert_eq!(router.count(), 1);
        let call = &router.calls.borrow()[0];
        assert!(call.to.distance_m(&wp1) < 1.0);
        assert!(call.from.distance_m(&at(340.0, 45.0)) < 1.0);
    }

    #[test]
    fn single_jump_is_no_deviation() {
        let mut guide = Guide::new(two_legs(), GuideParams::default());
        let router = FakeRouter::default();
        walk(&mut guide, &router, along(0.0, 500.0, 0.0));
        let status = walk(&mut guide, &router, [(550.0, 150.0, 11.0)]);
        assert_eq!(status.phase, GuidePhase::OnRoute);
        let status = walk(&mut guide, &router, along(600.0, 1200.0, 12.0));
        assert_eq!(status.deviation, 0);
        assert_eq!(status.phase, GuidePhase::OnRoute);
        assert!((status.along_m - 1200.0).abs() < 2.0, "{status:?}");
        assert_eq!(router.count(), 0);
    }

    #[test]
    fn inaccurate_fixes_do_not_count() {
        let mut guide = Guide::new(two_legs(), GuideParams::default());
        let router = FakeRouter::default();
        walk(&mut guide, &router, along(0.0, 300.0, 0.0));
        let mut status = guide.status();
        for i in 0..10 {
            let mut f = fix(
                at(320.0 + 5.0 * f64::from(i), 60.0),
                10.0 + 3.0 * f64::from(i),
            );
            f.h_accuracy_m = Some(80.0);
            status = guide.update(&f, &router);
        }
        assert_eq!(status.deviation, 0);
        assert_eq!(status.phase, GuidePhase::OnRoute);
        assert_eq!(router.count(), 0);
    }

    #[test]
    fn back_on_route_before_waypoint_drops_way_back() {
        let mut guide = Guide::new(two_legs(), GuideParams::default());
        let router = FakeRouter::default();
        assert_eq!(deviate(&mut guide, &router).deviation, 1);
        // Along the way back, which joins the route at waypoint 1 (1000 m).
        let on_way = (0..8).map(|i| {
            let x_m = 360.0 + 40.0 * f64::from(i);
            (x_m, 45.0 * (1000.0 - x_m) / 660.0, 21.0 + f64::from(i))
        });
        let status = walk(&mut guide, &router, on_way);
        assert_eq!(status.phase, GuidePhase::OffRoute);
        assert!(status.offset_m.unwrap() < 1.0, "{status:?}");

        let status = walk(&mut guide, &router, [(700.0, 0.0, 30.0)]);
        assert_eq!(status.way_back, None);
        assert_eq!(status.phase, GuidePhase::OnRoute);
        assert_eq!(status.deviation, 1);
        assert!((status.along_m - 700.0).abs() < 2.0, "{status:?}");
        assert_eq!(router.count(), 1);
    }

    #[test]
    fn leaving_the_way_back_is_a_new_deviation() {
        let mut guide = Guide::new(two_legs(), GuideParams::default());
        let router = FakeRouter::default();
        deviate(&mut guide, &router);
        let status = walk(
            &mut guide,
            &router,
            [
                (500.0, -45.0, 30.0),
                (510.0, -45.0, 35.0),
                (520.0, -45.0, 40.0),
            ],
        );
        assert_eq!(status.deviation, 2);
        assert_eq!(status.phase, GuidePhase::OffRoute);
        assert_eq!(router.count(), 2);
        assert!(router.calls.borrow()[1].from.distance_m(&at(520.0, -45.0)) < 1.0);
    }

    #[test]
    fn no_inner_waypoints_routes_to_destination() {
        let line = east_route(&[(Profile::Hike, 2000.0)], Vec::new());
        let end = line.point_at(line.length_m);
        let mut guide = Guide::new(line, GuideParams::default());
        let router = FakeRouter::default();
        let status = deviate(&mut guide, &router);
        assert_eq!(status.next_waypoint, Some(1));
        assert_eq!(router.count(), 1);
        assert!(router.calls.borrow()[0].to.distance_m(&end) < 1.0);
    }

    #[test]
    fn failed_way_back_waits_before_retry() {
        let mut guide = Guide::new(two_legs(), GuideParams::default());
        let router = FakeRouter::default();
        router.fail.set(true);
        let status = deviate(&mut guide, &router);
        assert_eq!(status.phase, GuidePhase::NoWayBack);
        assert_eq!(status.deviation, 1);
        assert_eq!(status.way_back, None);
        assert_eq!(router.count(), 1);

        walk(&mut guide, &router, [(345.0, 45.0, 25.0)]);
        assert_eq!(router.count(), 1, "too soon");
        walk(&mut guide, &router, [(345.0, 45.0, 80.0)]);
        assert_eq!(router.count(), 2, "after 60 s");
        walk(&mut guide, &router, [(450.0, 45.0, 85.0)]);
        assert_eq!(router.count(), 2, "moved only 105 m");
        let status = walk(&mut guide, &router, [(560.0, 45.0, 90.0)]);
        assert_eq!(router.count(), 3, "after 200 m");
        assert_eq!(status.deviation, 1);
        assert_eq!(status.phase, GuidePhase::NoWayBack);
    }

    #[test]
    fn straight_and_missing_legs_never_deviate() {
        let (w0, w1, w2, w3) = (A, east(A, 1000.0), east(A, 2000.0), east(A, 3000.0));
        let line = RouteLine::from_plan(&plan(
            vec![w0, w1, w2, w3],
            vec![
                leg(Profile::Hike, vec![w0, w1]),
                leg(Profile::Manual, vec![w1, w2]),
                failed_leg(),
            ],
            Vec::new(),
        ));
        let mut guide = Guide::new(line, GuideParams::default());
        let router = FakeRouter::default();
        walk(&mut guide, &router, along(0.0, 1100.0, 0.0));
        let across = (0..19).map(|i| {
            (
                1100.0 + 100.0 * f64::from(i),
                100.0,
                30.0 + 10.0 * f64::from(i),
            )
        });
        let status = walk(&mut guide, &router, across);
        assert_eq!(status.deviation, 0);
        assert_eq!(status.phase, GuidePhase::OnRoute);
        assert!(status.along_m > 2800.0, "{status:?}");
        assert_eq!(router.count(), 0);
    }

    #[test]
    fn way_back_profile_from_routed_neighbour() {
        let off = [
            (1560.0, 50.0, 40.0),
            (1565.0, 50.0, 45.0),
            (1570.0, 50.0, 50.0),
        ];

        let line = east_route(
            &[
                (Profile::Trekking, 1000.0),
                (Profile::Manual, 500.0),
                (Profile::Hike, 1000.0),
            ],
            Vec::new(),
        );
        let mut guide = Guide::new(line, GuideParams::default());
        let router = FakeRouter::default();
        walk(&mut guide, &router, along(0.0, 1450.0, 0.0));
        walk(&mut guide, &router, [(1480.0, 0.0, 35.0)]);
        let status = walk(&mut guide, &router, off);
        assert_eq!(status.deviation, 1);
        assert_eq!(router.calls.borrow()[0].leg.profile, Profile::Trekking);

        // Without a routed leg before, the one after.
        let line = east_route(
            &[(Profile::Manual, 1500.0), (Profile::Hike, 1000.0)],
            Vec::new(),
        );
        let mut guide = Guide::new(line, GuideParams::default());
        let router = FakeRouter::default();
        walk(&mut guide, &router, along(0.0, 1450.0, 0.0));
        walk(&mut guide, &router, [(1480.0, 0.0, 35.0)]);
        let status = walk(&mut guide, &router, off);
        assert_eq!(status.deviation, 1);
        assert_eq!(router.calls.borrow()[0].leg.profile, Profile::Hike);
    }

    #[test]
    fn remaining_with_stages() {
        let line = east_route(&[(Profile::Hike, 1000.0); 3], vec![1]);
        let mut guide = Guide::new(line, GuideParams::default());
        let router = FakeRouter::default();
        let status = walk(&mut guide, &router, along(0.0, 500.0, 0.0));
        let stage = status.to_stage_end.unwrap();
        let destination = status.to_destination;
        assert!(stage.distance_m < destination.distance_m);
        assert!((stage.distance_m - 500.0).abs() < 2.0, "{stage:?}");
        assert!((stage.time_s.unwrap() - 30.0).abs() < 0.1, "{stage:?}");
        assert!(
            (destination.distance_m - 2500.0).abs() < 2.0,
            "{destination:?}"
        );
        assert!(
            (destination.time_s.unwrap() - 150.0).abs() < 0.1,
            "{destination:?}"
        );

        let status = walk(&mut guide, &router, along(550.0, 1500.0, 11.0));
        assert_eq!(status.to_stage_end, Some(status.to_destination));
        assert!((status.to_destination.distance_m - 1500.0).abs() < 2.0);
    }

    #[test]
    fn remaining_includes_the_way_back() {
        let mut guide = Guide::new(two_legs(), GuideParams::default());
        let router = FakeRouter::default();
        let status = deviate(&mut guide, &router);
        let way = status.way_back.clone().unwrap();
        // The rest of the route from waypoint 1 on, plus the way back.
        let expected = way.distance_m + 1000.0;
        assert!(
            (status.to_destination.distance_m - expected).abs() < 2.0,
            "{status:?}"
        );
        let time = status.to_destination.time_s.unwrap();
        assert!(
            (time - (way.time_s.unwrap() + 60.0)).abs() < 1.0,
            "{status:?}"
        );
    }

    #[test]
    fn start_from_waypoint_skips_global_search() {
        // Out east 1000 m, 100 m north, and back west beside the way out.
        let (w0, w1) = (A, east(A, 1000.0));
        let (w2, w3) = (north(w1, 100.0), north(A, 100.0));
        let line = RouteLine::from_plan(&plan(
            vec![w0, w1, w2, w3],
            vec![
                leg(Profile::Hike, vec![w0, w1]),
                leg(Profile::Hike, vec![w1, w2]),
                leg(Profile::Hike, vec![w2, w3]),
            ],
            Vec::new(),
        ));
        let router = FakeRouter::default();
        let p = fix(at(700.0, 30.0), 0.0);

        let mut global = Guide::new(line.clone(), GuideParams::default());
        let status = global.update(&p, &router);
        assert!((status.along_m - 700.0).abs() < 2.0, "{status:?}");

        let mut guide = Guide::new(line, GuideParams::default());
        guide.start_from_waypoint(2);
        assert_eq!(guide.status().phase, GuidePhase::Waiting);
        assert!((guide.status().along_m - 1100.0).abs() < 2.0);
        let status = guide.update(&p, &router);
        assert!((status.along_m - 1400.0).abs() < 2.0, "{status:?}");
        assert_eq!(status.next_waypoint, Some(3));
    }

    #[test]
    fn fix_after_turnaround_moves_forward() {
        let out = track_points(OBERGRIES);
        let back: Vec<LatLon> = out.iter().rev().copied().collect();
        let (start, end) = (out[0], out[out.len() - 1]);
        let line = RouteLine::from_plan(&plan(
            vec![start, end, start],
            vec![
                leg(Profile::Hike, out.clone()),
                leg(Profile::Hike, back.clone()),
            ],
            Vec::new(),
        ));
        let turn_m = line.waypoint_m[1];
        let mut guide = Guide::new(line, GuideParams::default());
        let router = FakeRouter::default();
        let fixes = out[2 * out.len() / 3..]
            .iter()
            .chain(&back[1..back.len() / 3]);
        let mut last_m = f64::NEG_INFINITY;
        for (i, &p) in fixes.enumerate() {
            let status = guide.update(&fix(p, i as f64), &router);
            assert!(
                status.along_m >= last_m - 0.5,
                "fix {i}: {status:?} after {last_m}"
            );
            last_m = status.along_m;
        }
        assert!(last_m > turn_m, "{last_m} {turn_m}");
        assert_eq!(router.count(), 0);
    }

    #[test]
    fn heading_only_when_moving() {
        let mut f = fix(A, 0.0);
        f.course_deg = Some(90.0);
        assert_eq!(heading(&f), None);
        f.speed_mps = Some(1.5);
        assert_eq!(heading(&f), Some(90.0));
        f.course_deg = Some(-1.0);
        assert_eq!(heading(&f), None);
        f.course_deg = Some(360.0);
        assert_eq!(heading(&f), None);
        f.course_deg = Some(f32::NAN);
        assert_eq!(heading(&f), None);
    }
}
