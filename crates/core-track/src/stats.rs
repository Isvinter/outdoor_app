use serde::{Deserialize, Serialize};

use crate::filter::filter_segment;
use crate::geo::haversine_m;
use crate::{Activity, Segment, SegmentReason, StatsParams, Track, TrackPoint};

/// Summary statistics of a track.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TrackStats {
    pub distance_m: f64,
    pub ascent_m: f64,
    pub descent_m: f64,
    pub max_speed_mps: f64,
    /// Distance divided by time in motion.
    pub avg_moving_speed_mps: f64,
    /// Distance divided by total time.
    pub avg_total_speed_mps: f64,
    /// Recording time without the pauses between segments.
    pub total_time_s: f64,
    pub moving_time_s: f64,
    pub min_ele_m: Option<f64>,
    pub max_ele_m: Option<f64>,
    /// Number of points in the track.
    pub points_total: usize,
    /// Number of points that passed the filters.
    pub points_used: usize,
}

/// Computes the statistics of a track. Pure and infallible: invalid input points
/// are skipped, an empty track yields all-zero statistics.
pub fn compute_stats(track: &Track, params: &StatsParams) -> TrackStats {
    segments_stats(&track.segments, params)
}

/// Statistics of a tour and of each of its sections (segments), each section with
/// the parameters of its own activity ([`Track::segment_activity`]), the tour as their
/// sum. Usually `params_of` is [`StatsParams::for_activity`].
///
/// A rest section counts only towards total time (and its points), not moving time,
/// distance or elevation. The section after a rest ([`SegmentReason::RestEnd`])
/// additionally gets the straight line from the first fix of the rest (the one it was
/// detected at, near the rest spot) to its own first fix as distance. Fixes during a
/// rest may be coarse or sparse, and the last ones are already on the way out, so that
/// part of the way is otherwise lost.
pub fn compute_section_stats(
    track: &Track,
    params_of: impl Fn(Activity) -> StatsParams,
) -> (TrackStats, Vec<TrackStats>) {
    let sections: Vec<TrackStats> = track
        .segments
        .iter()
        .enumerate()
        .map(|(i, segment)| {
            let s = segments_stats(
                std::slice::from_ref(segment),
                &params_of(track.segment_activity(i)),
            );
            if segment.is_rest() {
                return TrackStats {
                    total_time_s: s.total_time_s,
                    points_total: s.points_total,
                    points_used: s.points_used,
                    ..TrackStats::default()
                };
            }
            let after_rest = i
                .checked_sub(1)
                .map(|j| &track.segments[j])
                .filter(|prev| segment.reason == SegmentReason::RestEnd && prev.is_rest());
            match after_rest.and_then(|prev| straight_line_m(prev, segment)) {
                Some(d) => with_averages(TrackStats {
                    distance_m: s.distance_m + d,
                    ..s
                }),
                None => s,
            }
        })
        .collect();

    let mut total = TrackStats::default();
    for s in &sections {
        total.distance_m += s.distance_m;
        total.ascent_m += s.ascent_m;
        total.descent_m += s.descent_m;
        total.total_time_s += s.total_time_s;
        total.moving_time_s += s.moving_time_s;
        total.points_total += s.points_total;
        total.points_used += s.points_used;
        total.max_speed_mps = total.max_speed_mps.max(s.max_speed_mps);
        total.min_ele_m = merge_option(total.min_ele_m, s.min_ele_m, f64::min);
        total.max_ele_m = merge_option(total.max_ele_m, s.max_ele_m, f64::max);
    }
    (with_averages(total), sections)
}

/// Distance from the first valid fix of `from` to the first valid fix of `to`.
fn straight_line_m(from: &Segment, to: &Segment) -> Option<f64> {
    let valid = |p: &&TrackPoint| p.lat.is_finite() && p.lon.is_finite();
    let a = from.points.iter().find(valid)?;
    let b = to.points.iter().find(valid)?;
    Some(haversine_m(a.lat, a.lon, b.lat, b.lon))
}

/// Combines two optional values with `f`, or takes the one that is present.
fn merge_option(a: Option<f64>, b: Option<f64>, f: fn(f64, f64) -> f64) -> Option<f64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(f(a, b)),
        (a, b) => a.or(b),
    }
}

/// Recomputes the average speeds from distance and times.
fn with_averages(mut s: TrackStats) -> TrackStats {
    s.avg_moving_speed_mps = if s.moving_time_s > 0.0 {
        s.distance_m / s.moving_time_s
    } else {
        0.0
    };
    s.avg_total_speed_mps = if s.total_time_s > 0.0 {
        s.distance_m / s.total_time_s
    } else {
        0.0
    };
    s
}

fn segments_stats(segments: &[Segment], params: &StatsParams) -> TrackStats {
    let mut s = TrackStats::default();
    // (duration, distance) of every interval longer than `max_gap_s`
    let mut gaps: Vec<(f64, f64)> = Vec::new();

    for segment in segments {
        s.points_total += segment.points.len();
        let runs = filter_segment(&segment.points, params);

        let mut times = runs.iter().flatten().filter_map(|p| p.time_ms);
        if let Some(first) = times.next() {
            let last = times.next_back().unwrap_or(first);
            s.total_time_s += (last - first) as f64 / 1000.0;
        }

        // (distance along the segment, elevation) of every accepted point with elevation
        let mut profile: Vec<(f64, f64)> = Vec::new();
        let mut segment_distance = 0.0;
        for run in &runs {
            s.points_used += run.len();
            let along = cumulative_distance(run, params.min_move_m);
            for (p, d) in run.iter().zip(&along) {
                if let Some(ele) = p.ele.filter(|e| e.is_finite()) {
                    profile.push((segment_distance + d, ele));
                }
            }
            segment_distance += along.last().copied().unwrap_or(0.0);

            if let Some(t) = run.iter().map(|p| p.time_ms).collect::<Option<Vec<i64>>>() {
                // Speeds are evaluated between gaps; a window must not reach across one.
                let mut start = 0;
                for k in 1..=run.len() {
                    let gap_s = (k < run.len())
                        .then(|| (t[k] - t[k - 1]) as f64 / 1000.0)
                        .filter(|dt| *dt > params.max_gap_s);
                    if k < run.len() && gap_s.is_none() {
                        continue;
                    }
                    let (chunk, times) = (&run[start..k], &t[start..k]);
                    let fastest = SpeedEstimate::MedianOfIntervals;
                    let top = point_speeds(chunk, times, params.speed_window_s, fastest)
                        .into_iter()
                        .fold(0.0, f64::max);
                    s.max_speed_mps = s.max_speed_mps.max(top);
                    let net = SpeedEstimate::NetDisplacement;
                    let speeds = point_speeds(chunk, times, params.move_window_s, net);
                    s.moving_time_s += moving_time(times, &speeds, params);
                    if let Some(dt) = gap_s {
                        let (a, b) = (&run[k - 1], &run[k]);
                        gaps.push((dt, haversine_m(a.lat, a.lon, b.lat, b.lon)));
                    }
                    start = k;
                }
            }
        }
        s.distance_m += segment_distance;

        let smoothed = smooth_by_distance(&profile, params.ele_smooth_m);
        let (up, down) = ascent_descent(&smoothed, params.ele_threshold_m);
        s.ascent_m += up;
        s.descent_m += down;
        for e in smoothed {
            s.min_ele_m = Some(s.min_ele_m.map_or(e, |m| m.min(e)));
            s.max_ele_m = Some(s.max_ele_m.map_or(e, |m| m.max(e)));
        }
    }

    // Nothing is known about a gap except its length and duration. Assume the
    // average moving pace of the rest of the track, capped by the gap's duration.
    let gap_distance: f64 = gaps.iter().map(|(_, d)| d).sum();
    if s.moving_time_s > 0.0 && s.distance_m > gap_distance {
        let pace = (s.distance_m - gap_distance) / s.moving_time_s;
        s.moving_time_s += gaps.iter().map(|(dt, d)| dt.min(d / pace)).sum::<f64>();
    }

    with_averages(s)
}

/// Distance travelled up to each point. Distance only advances once the position
/// is at least `min_move_m` away from the last point that advanced it, so jitter
/// while standing still adds nothing.
fn cumulative_distance(run: &[TrackPoint], min_move_m: f64) -> Vec<f64> {
    let mut out = Vec::with_capacity(run.len());
    let mut total = 0.0;
    let mut anchor = run.first();
    for p in run {
        if let Some(a) = anchor {
            let d = haversine_m(a.lat, a.lon, p.lat, p.lon);
            if d >= min_move_m {
                total += d;
                anchor = Some(p);
            }
        }
        out.push(total);
    }
    out
}

/// How speed is derived from positions when the receiver reports none.
#[derive(Clone, Copy)]
enum SpeedEstimate {
    /// Straight-line distance across the window divided by its duration. Position
    /// jitter while standing still cancels out, so this decides "moving or not".
    NetDisplacement,
    /// Time-weighted median of the fix-to-fix speeds inside the window. A few
    /// wrong timestamps or positions cannot raise it, so this is used for the
    /// maximum speed.
    MedianOfIntervals,
}

/// Speed at every point of a run, over a time window around the point.
///
/// Uses the receiver's Doppler speed (median over the window) when every point
/// has one, otherwise an estimate derived from the positions, see [`SpeedEstimate`].
///
/// The window always spans at least `window_s` (less only if the whole run is
/// shorter): it reaches out to the nearest fixes beyond `±window_s / 2`. Real
/// recordings have irregular timestamps, and two fixes a second apart would
/// otherwise yield wildly wrong speeds.
fn point_speeds(run: &[TrackPoint], t: &[i64], window_s: f64, estimate: SpeedEstimate) -> Vec<f64> {
    let n = run.len();
    // intervals[i] is (speed, duration) between fix i and fix i + 1
    let intervals: Vec<(f64, f64)> = (1..n)
        .map(|i| {
            let dt = (t[i] - t[i - 1]) as f64 / 1000.0;
            let d = haversine_m(run[i - 1].lat, run[i - 1].lon, run[i].lat, run[i].lon);
            (d / dt, dt)
        })
        .collect();
    let window_ms = (window_s * 1000.0) as i64;
    let half_ms = window_ms / 2;
    let reported: Option<Vec<f64>> = run
        .iter()
        .map(|p| {
            p.speed_mps
                .map(f64::from)
                .filter(|v| v.is_finite() && *v >= 0.0)
        })
        .collect();

    let mut out = Vec::with_capacity(n);
    for k in 0..n {
        // last fix at or before the window start, first fix at or after its end
        let mut a = t
            .partition_point(|&x| x <= t[k] - half_ms)
            .saturating_sub(1);
        let b = t
            .partition_point(|&x| x < (t[k] + half_ms).max(t[a] + window_ms))
            .min(n - 1);
        if t[b] - t[a] < window_ms {
            a = t
                .partition_point(|&x| x <= t[b] - window_ms)
                .saturating_sub(1);
        }
        let v = match &reported {
            Some(speeds) => median(&speeds[a..=b]),
            None => match estimate {
                SpeedEstimate::NetDisplacement => {
                    let dt = (t[b] - t[a]) as f64 / 1000.0;
                    if dt > 0.0 {
                        haversine_m(run[a].lat, run[a].lon, run[b].lat, run[b].lon) / dt
                    } else {
                        0.0
                    }
                }
                SpeedEstimate::MedianOfIntervals => time_weighted_median(&intervals[a..b]),
            },
        };
        out.push(v);
    }
    out
}

/// The speed that was not exceeded for half of the time covered by `intervals`
/// (pairs of speed and duration). Unlike a plain median it is not swayed by
/// many very short intervals, which is what bunched-up timestamps produce.
fn time_weighted_median(intervals: &[(f64, f64)]) -> f64 {
    let mut sorted = intervals.to_vec();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    let half = sorted.iter().map(|(_, dt)| dt).sum::<f64>() / 2.0;
    let mut elapsed = 0.0;
    for (speed, dt) in &sorted {
        elapsed += dt;
        if elapsed >= half {
            return *speed;
        }
    }
    0.0
}

fn median(values: &[f64]) -> f64 {
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    match v.len() {
        0 => 0.0,
        n if n % 2 == 1 => v[n / 2],
        n => (v[n / 2 - 1] + v[n / 2]) / 2.0,
    }
}

/// Time in motion within a stretch without gaps, with hysteresis: a slowdown shorter than `stop_delay_s` still
/// counts as moving, a longer one does not count at all.
fn moving_time(t: &[i64], speeds: &[f64], params: &StatsParams) -> f64 {
    let mut total = 0.0;
    let mut moving = false;
    let mut pending = 0.0;
    for k in 1..t.len() {
        let dt = (t[k] - t[k - 1]) as f64 / 1000.0;
        let v = (speeds[k - 1] + speeds[k]) / 2.0;
        if v >= params.v_move_mps {
            if moving {
                total += pending;
            }
            pending = 0.0;
            moving = true;
            total += dt;
        } else if moving {
            pending += dt;
            if pending > params.stop_delay_s {
                moving = false;
                pending = 0.0;
            }
        }
    }
    total
}

/// Total ascent and descent in metres of an elevation profile given as
/// `(distance from start, elevation)` pairs in metres. The profile is smoothed
/// over `smooth_window_m` of distance; changes of direction smaller than
/// `threshold_m` are ignored. The same method as for recorded tracks.
pub fn elevation_gain(
    profile: &[(f64, f64)],
    smooth_window_m: f64,
    threshold_m: f64,
) -> (f64, f64) {
    ascent_descent(&smooth_by_distance(profile, smooth_window_m), threshold_m)
}

/// Moving average of elevation over a window of `window_m` metres of travelled
/// distance, centred on each point. Independent of the recording interval.
fn smooth_by_distance(profile: &[(f64, f64)], window_m: f64) -> Vec<f64> {
    let half = window_m / 2.0;
    let mut prefix = Vec::with_capacity(profile.len() + 1);
    prefix.push(0.0);
    for (_, e) in profile {
        prefix.push(prefix[prefix.len() - 1] + e);
    }
    let (mut a, mut b) = (0usize, 0usize);
    profile
        .iter()
        .map(|(d, _)| {
            while profile[a].0 < d - half {
                a += 1;
            }
            while b + 1 < profile.len() && profile[b + 1].0 <= d + half {
                b += 1;
            }
            (prefix[b + 1] - prefix[a]) / (b + 1 - a) as f64
        })
        .collect()
}

/// Total ascent and descent with hysteresis: a change of direction is only
/// recognised once elevation has moved `threshold_m` away from the last extreme.
fn ascent_descent(elevations: &[f64], threshold_m: f64) -> (f64, f64) {
    enum Trend {
        Unknown { low: f64, high: f64 },
        Up { high: f64 },
        Down { low: f64 },
    }
    let Some(&first) = elevations.first() else {
        return (0.0, 0.0);
    };
    let (mut up, mut down) = (0.0, 0.0);
    let mut trend = Trend::Unknown {
        low: first,
        high: first,
    };
    for &e in &elevations[1..] {
        trend = match trend {
            Trend::Unknown { low, high } => {
                if e - low >= threshold_m {
                    up += e - low;
                    Trend::Up { high: e }
                } else if high - e >= threshold_m {
                    down += high - e;
                    Trend::Down { low: e }
                } else {
                    Trend::Unknown {
                        low: low.min(e),
                        high: high.max(e),
                    }
                }
            }
            Trend::Up { high } => {
                if e > high {
                    up += e - high;
                    Trend::Up { high: e }
                } else if high - e >= threshold_m {
                    down += high - e;
                    Trend::Down { low: e }
                } else {
                    Trend::Up { high }
                }
            }
            Trend::Down { low } => {
                if e < low {
                    down += low - e;
                    Trend::Down { low: e }
                } else if e - low >= threshold_m {
                    up += e - low;
                    Trend::Up { high: e }
                } else {
                    Trend::Down { low }
                }
            }
        };
    }
    (up, down)
}
