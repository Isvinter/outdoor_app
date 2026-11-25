//! A planned route as one line with distances along it, so that a position
//! can be placed on the route and progress measured in metres.

use core_routing::{LatLon, Profile};
use core_track::geo::haversine_m;

use crate::plan::{LegSpec, RoutePlan};

/// How the line of a leg came about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegKind {
    /// Found by the router.
    Routed,
    /// Joined straight by hand ([`Profile::Manual`]).
    Straight,
    /// Could not be computed. Its line is a straight stand-in between its
    /// waypoints, only for placing positions on it; it is never shown.
    Missing,
}

/// Where one leg lies along the line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LegSpan {
    pub from_m: f64,
    pub to_m: f64,
    pub kind: LegKind,
    pub spec: LegSpec,
    pub time_s: Option<f64>,
}

/// A position placed on the line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Projection {
    /// Distance from the start of the line to the nearest point on it.
    pub along_m: f64,
    /// Distance from the position to that point.
    pub offset_m: f64,
    /// Index of the leg the nearest point lies on.
    pub leg: usize,
}

/// All legs of a plan joined into one line.
#[derive(Debug, Clone, PartialEq)]
pub struct RouteLine {
    points: Vec<LatLon>,
    /// Distance from the start to each point.
    cum_m: Vec<f64>,
    /// Leg of the segment from point `i` to point `i + 1`.
    segment_leg: Vec<usize>,
    pub legs: Vec<LegSpan>,
    /// Distance from the start to each waypoint.
    pub waypoint_m: Vec<f64>,
    /// Distance from the start to each stage end.
    pub stage_end_m: Vec<f64>,
    pub length_m: f64,
}

/// Metres per degree of latitude on the mean Earth sphere.
const M_PER_DEG: f64 = core_track::geo::EARTH_RADIUS_M * std::f64::consts::PI / 180.0;

/// Offsets this close to the nearest one count as equally near in
/// [`RouteLine::project_near`].
pub const TIE_M: f64 = 1.0;

impl RouteLine {
    /// The line of all legs of `plan`, in order. A failed leg gets a straight
    /// stand-in between its waypoints.
    #[must_use]
    pub fn from_plan(plan: &RoutePlan) -> RouteLine {
        let waypoints = &plan.request.waypoints;
        let mut line = RouteLine {
            points: Vec::new(),
            cum_m: Vec::new(),
            segment_leg: Vec::new(),
            legs: Vec::with_capacity(plan.legs.len()),
            waypoint_m: Vec::new(),
            stage_end_m: Vec::new(),
            length_m: 0.0,
        };
        for (index, leg) in plan.legs.iter().enumerate() {
            let kind = if leg.error.is_some() || leg.line.is_empty() {
                LegKind::Missing
            } else if leg.profile == Profile::Manual {
                LegKind::Straight
            } else {
                LegKind::Routed
            };
            let stand_in: Vec<LatLon>;
            let geometry = if kind == LegKind::Missing {
                stand_in = waypoints.iter().skip(index).take(2).copied().collect();
                &stand_in
            } else {
                &leg.line
            };
            let from_m = line.length_m;
            for &point in geometry {
                line.push(point, index);
            }
            line.legs.push(LegSpan {
                from_m,
                to_m: line.length_m,
                kind,
                spec: LegSpec {
                    profile: leg.profile,
                    options: leg.options,
                },
                time_s: leg.time_s,
            });
        }
        if line.points.is_empty()
            && let Some(&first) = waypoints.first()
        {
            line.push(first, 0);
        }
        line.waypoint_m = (0..waypoints.len())
            .map(|i| match i.checked_sub(1) {
                None => 0.0,
                Some(leg) => line.legs.get(leg).map_or(line.length_m, |span| span.to_m),
            })
            .collect();
        line.stage_end_m = plan
            .request
            .stage_ends
            .iter()
            .filter_map(|&w| line.waypoint_m.get(w).copied())
            .collect();
        line
    }

    /// Whether the line has no point at all (a plan without waypoints).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Appends `point` as part of leg `leg`; a repeated point is left out.
    fn push(&mut self, point: LatLon, leg: usize) {
        let Some(&last) = self.points.last() else {
            self.points.push(point);
            self.cum_m.push(0.0);
            return;
        };
        if last == point {
            return;
        }
        self.length_m += haversine_m(last.lat, last.lon, point.lat, point.lon);
        self.points.push(point);
        self.cum_m.push(self.length_m);
        self.segment_leg.push(leg);
    }

    /// A line of one routed leg along `points`, such as a way back.
    #[must_use]
    pub(crate) fn from_points(points: &[LatLon], spec: LegSpec, time_s: Option<f64>) -> RouteLine {
        let mut line = RouteLine {
            points: Vec::with_capacity(points.len()),
            cum_m: Vec::with_capacity(points.len()),
            segment_leg: Vec::new(),
            legs: Vec::new(),
            waypoint_m: Vec::new(),
            stage_end_m: Vec::new(),
            length_m: 0.0,
        };
        for &point in points {
            line.push(point, 0);
        }
        line.legs.push(LegSpan {
            from_m: 0.0,
            to_m: line.length_m,
            kind: LegKind::Routed,
            spec,
            time_s,
        });
        line.waypoint_m = vec![0.0, line.length_m];
        line
    }

    /// The nearest point of the line to `p`; `window` limits the search to
    /// `along_m` in `[from, to]`. Segments partly in the window count with
    /// their part inside it. `None` if no part of the line is in the window.
    #[must_use]
    pub fn project(&self, p: LatLon, window: Option<(f64, f64)>) -> Option<Projection> {
        self.project_near(p, window, None)
    }

    /// Like [`RouteLine::project`], but among points whose offsets lie within
    /// [`TIE_M`] of the nearest one, the one closest to `hint_m` along the
    /// line wins. On a road taken out and back, this keeps the progress on
    /// the way it is on, also right after the turnaround.
    #[must_use]
    pub fn project_near(
        &self,
        p: LatLon,
        window: Option<(f64, f64)>,
        hint_m: Option<f64>,
    ) -> Option<Projection> {
        let (lo, hi) = window.unwrap_or((f64::NEG_INFINITY, f64::INFINITY));
        if lo > hi {
            return None;
        }
        // A local plane around `p`, in metres.
        let kx = M_PER_DEG * p.lat.to_radians().cos();
        let local = |q: LatLon| ((q.lon - p.lon) * kx, (q.lat - p.lat) * M_PER_DEG);

        if self.points.len() == 1 {
            if lo > 0.0 || hi < 0.0 {
                return None;
            }
            let (x, y) = local(self.points[0]);
            return Some(Projection {
                along_m: 0.0,
                offset_m: x.hypot(y),
                leg: 0,
            });
        }

        let candidates = self.points.windows(2).enumerate().filter_map(|(i, ends)| {
            let (start_m, end_m) = (self.cum_m[i], self.cum_m[i + 1]);
            if end_m < lo || start_m > hi {
                return None;
            }
            let seg_m = end_m - start_m;
            let (t_lo, t_hi) = if seg_m > 0.0 {
                (
                    ((lo - start_m) / seg_m).clamp(0.0, 1.0),
                    ((hi - start_m) / seg_m).clamp(0.0, 1.0),
                )
            } else {
                (0.0, 0.0)
            };
            let (ax, ay) = local(ends[0]);
            let (bx, by) = local(ends[1]);
            let (dx, dy) = (bx - ax, by - ay);
            let len2 = dx * dx + dy * dy;
            let t = if len2 > 0.0 {
                -(ax * dx + ay * dy) / len2
            } else {
                0.0
            };
            let t = t.clamp(t_lo, t_hi);
            Some(Projection {
                along_m: (start_m + t * seg_m).clamp(lo, hi),
                offset_m: (ax + t * dx).hypot(ay + t * dy),
                leg: self.segment_leg[i],
            })
        });

        let Some(hint_m) = hint_m else {
            return candidates.reduce(|best, c| if c.offset_m < best.offset_m { c } else { best });
        };
        let candidates: Vec<Projection> = candidates.collect();
        let nearest_m = candidates
            .iter()
            .map(|c| c.offset_m)
            .fold(f64::INFINITY, f64::min);
        candidates
            .into_iter()
            .filter(|c| c.offset_m <= nearest_m + TIE_M)
            .reduce(|best, c| {
                if (c.along_m - hint_m).abs() < (best.along_m - hint_m).abs() {
                    c
                } else {
                    best
                }
            })
    }

    /// The point `along_m` from the start, clamped to the line. An empty
    /// line (a plan without waypoints) has only `0, 0`.
    #[must_use]
    pub fn point_at(&self, along_m: f64) -> LatLon {
        let (Some(&first), Some(&last)) = (self.points.first(), self.points.last()) else {
            return LatLon { lat: 0.0, lon: 0.0 };
        };
        if self.points.len() == 1 || along_m <= 0.0 {
            return first;
        }
        if along_m >= self.length_m {
            return last;
        }
        let i = self
            .cum_m
            .partition_point(|&c| c <= along_m)
            .saturating_sub(1)
            .min(self.points.len() - 2);
        let (a, b) = (self.points[i], self.points[i + 1]);
        let seg_m = self.cum_m[i + 1] - self.cum_m[i];
        let t = if seg_m > 0.0 {
            (along_m - self.cum_m[i]) / seg_m
        } else {
            0.0
        };
        LatLon {
            lat: a.lat + t * (b.lat - a.lat),
            lon: a.lon + t * (b.lon - a.lon),
        }
    }

    /// The part of the line from `from_m` to `to_m`, both clamped to the
    /// line and both included. Empty if `to_m` comes before `from_m`.
    #[must_use]
    pub fn line_between(&self, from_m: f64, to_m: f64) -> Vec<LatLon> {
        if to_m < from_m || self.points.is_empty() {
            return Vec::new();
        }
        let from_m = from_m.clamp(0.0, self.length_m);
        let to_m = to_m.clamp(0.0, self.length_m);
        let mut part = vec![self.point_at(from_m)];
        part.extend(
            self.points
                .iter()
                .zip(&self.cum_m)
                .filter(|&(_, &c)| c > from_m && c < to_m)
                .map(|(&q, _)| q),
        );
        part.push(self.point_at(to_m));
        part
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guide::testing::*;

    #[test]
    fn from_plan_places_waypoints_and_stage_ends() {
        let points = track_points(SYLVENSTEIN);
        let n = points.len();
        let plan = plan_along(&points, &[n / 3, 2 * n / 3], vec![2]);
        let line = RouteLine::from_plan(&plan);

        assert_eq!(line.waypoint_m.len(), 4);
        assert_eq!(line.waypoint_m[0], 0.0);
        assert_eq!(line.stage_end_m, vec![line.waypoint_m[2]]);
        let sum: f64 = plan.legs.iter().map(|l| line_length(&l.line)).sum();
        assert!(
            (line.length_m - sum).abs() <= 1.0,
            "{} vs {sum}",
            line.length_m
        );
        assert_eq!(line.legs.len(), 3);
        for (i, span) in line.legs.iter().enumerate() {
            assert_eq!(span.kind, LegKind::Routed);
            assert_eq!(span.from_m, line.waypoint_m[i]);
            assert_eq!(span.to_m, line.waypoint_m[i + 1]);
            assert_eq!(span.time_s, Some(60.0));
            assert_eq!(span.spec.profile, Profile::Hike);
        }
        assert_eq!(line.waypoint_m[3], line.length_m);
    }

    #[test]
    fn failed_leg_is_missing_with_straight_stand_in() {
        let a = LatLon {
            lat: 47.6,
            lon: 11.7,
        };
        let b = east(a, 1000.0);
        let c = north(b, 1000.0);
        let line = RouteLine::from_plan(&plan(
            vec![a, b, c],
            vec![leg(Profile::Hike, vec![a, b]), failed_leg()],
            Vec::new(),
        ));

        assert_eq!(line.legs[0].kind, LegKind::Routed);
        assert_eq!(line.legs[1].kind, LegKind::Missing);
        assert!((line.length_m - 2000.0).abs() < 2.0, "{}", line.length_m);
        let on_stand_in = north(b, 400.0);
        let hit = line.project(on_stand_in, None).unwrap();
        assert!(hit.offset_m < 1.0, "{hit:?}");
        assert_eq!(hit.leg, 1);
        assert!((hit.along_m - 1400.0).abs() < 2.0, "{hit:?}");
    }

    #[test]
    fn manual_leg_is_straight() {
        let a = LatLon {
            lat: 47.6,
            lon: 11.7,
        };
        let b = east(a, 500.0);
        let line = RouteLine::from_plan(&plan(
            vec![a, b],
            vec![leg(Profile::Manual, vec![a, b])],
            Vec::new(),
        ));
        assert_eq!(line.legs[0].kind, LegKind::Straight);
    }

    #[test]
    fn project_finds_offset_beside_the_line() {
        let a = LatLon {
            lat: 47.6,
            lon: 11.7,
        };
        let b = east(a, 1000.0);
        let line = RouteLine::from_plan(&plan(
            vec![a, b],
            vec![leg(Profile::Hike, vec![a, b])],
            Vec::new(),
        ));
        let beside = north(east(a, 300.0), 35.0);
        let hit = line.project(beside, None).unwrap();
        assert!((hit.offset_m - 35.0).abs() <= 2.0, "{hit:?}");
        assert!((hit.along_m - 300.0).abs() <= 2.0, "{hit:?}");
        assert_eq!(hit.leg, 0);
    }

    #[test]
    fn project_clamps_into_the_window() {
        let a = LatLon {
            lat: 47.6,
            lon: 11.7,
        };
        let b = east(a, 1000.0);
        let line = RouteLine::from_plan(&plan(
            vec![a, b],
            vec![leg(Profile::Hike, vec![a, b])],
            Vec::new(),
        ));
        let hit = line.project(east(a, 300.0), Some((500.0, 800.0))).unwrap();
        assert!((hit.along_m - 500.0).abs() < 0.01, "{hit:?}");
        assert!((hit.offset_m - 200.0).abs() < 1.0, "{hit:?}");
        assert!(line.project(a, Some((1500.0, 2000.0))).is_none());
    }

    #[test]
    fn out_and_back_keeps_direction() {
        let out = track_points(OBERGRIES);
        let back: Vec<LatLon> = out.iter().rev().copied().collect();
        let (start, end) = (out[0], out[out.len() - 1]);
        let line = RouteLine::from_plan(&plan(
            vec![start, end, start],
            vec![leg(Profile::Hike, out.clone()), leg(Profile::Hike, back)],
            Vec::new(),
        ));
        let turn_m = line.waypoint_m[1];
        assert!((line.length_m - 2.0 * turn_m).abs() < 1.0);

        // A position on the track, a third of the way out.
        let p = out[out.len() / 3];
        let out_m = line.project(p, Some((0.0, turn_m))).unwrap().along_m;
        let back_m = 2.0 * turn_m - out_m;

        let window = 500.0;
        let on_way_out = line
            .project(p, Some((out_m - window, out_m + window)))
            .unwrap();
        assert!(on_way_out.along_m < turn_m, "{on_way_out:?}");
        assert!((on_way_out.along_m - out_m).abs() < 1.0, "{on_way_out:?}");
        assert_eq!(on_way_out.leg, 0);

        let on_way_back = line
            .project(p, Some((back_m - window, back_m + window)))
            .unwrap();
        assert!(on_way_back.along_m > turn_m, "{on_way_back:?}");
        assert!(
            (on_way_back.along_m - back_m).abs() < 1.0,
            "{on_way_back:?}"
        );
        assert_eq!(on_way_back.leg, 1);
        assert!(on_way_back.offset_m < 0.5, "{on_way_back:?}");
    }

    #[test]
    fn point_at_and_line_between_follow_the_line() {
        let a = LatLon {
            lat: 47.6,
            lon: 11.7,
        };
        let b = east(a, 1000.0);
        let c = north(b, 1000.0);
        let line = RouteLine::from_plan(&plan(
            vec![a, c],
            vec![leg(Profile::Hike, vec![a, b, c])],
            Vec::new(),
        ));
        assert!(line.point_at(1500.0).distance_m(&north(b, 500.0)) < 1.0);
        assert_eq!(line.point_at(-10.0), a);
        assert_eq!(line.point_at(1e9), c);

        let part = line.line_between(500.0, 1500.0);
        assert_eq!(part.len(), 3);
        assert!(part[0].distance_m(&east(a, 500.0)) < 1.0);
        assert_eq!(part[1], b);
        assert!(part[2].distance_m(&north(b, 500.0)) < 1.0);
    }

    #[test]
    fn line_between_is_empty_when_reversed_beyond_the_end() {
        let a = LatLon {
            lat: 47.6,
            lon: 11.7,
        };
        let line = RouteLine::from_plan(&plan(
            vec![a, east(a, 1000.0)],
            vec![leg(Profile::Hike, vec![a, east(a, 1000.0)])],
            Vec::new(),
        ));
        assert!(line.line_between(5000.0, 2000.0).is_empty());
        assert!(line.line_between(-5.0, -10.0).is_empty());
        assert_eq!(line.line_between(-10.0, -5.0).len(), 2);
    }

    #[test]
    fn project_near_prefers_the_hint_on_a_tie() {
        let out = track_points(OBERGRIES);
        let back: Vec<LatLon> = out.iter().rev().copied().collect();
        let (start, end) = (out[0], out[out.len() - 1]);
        let line = RouteLine::from_plan(&plan(
            vec![start, end, start],
            vec![leg(Profile::Hike, out.clone()), leg(Profile::Hike, back)],
            Vec::new(),
        ));
        let turn_m = line.waypoint_m[1];
        let p = out[out.len() - 5];
        let out_m = line.project(p, None).unwrap().along_m;
        assert!(out_m < turn_m);
        let back_m = 2.0 * turn_m - out_m;

        let near_back = line.project_near(p, None, Some(back_m)).unwrap();
        assert!((near_back.along_m - back_m).abs() < 1.0, "{near_back:?}");
        let near_out = line.project_near(p, None, Some(out_m)).unwrap();
        assert!((near_out.along_m - out_m).abs() < 1.0, "{near_out:?}");
        // Without a hint, the nearest point still wins outright.
        let beside = north(p, 30.0);
        // Equal up to rounding: the two take different ways to the same point.
        let near = line.project_near(beside, None, Some(back_m)).unwrap();
        assert!((near.offset_m - line.project(beside, None).unwrap().offset_m).abs() < 1e-9);
    }
}
