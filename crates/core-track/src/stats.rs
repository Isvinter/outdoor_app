use serde::{Deserialize, Serialize};

use crate::filter::filter_segment;
use crate::geo::haversine_m;
use crate::{StatsParams, Track, TrackPoint};

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
    let mut s = TrackStats::default();

    for segment in &track.segments {
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
                let top = point_speeds(run, &t, params.speed_window_s)
                    .into_iter()
                    .fold(0.0, f64::max);
                s.max_speed_mps = s.max_speed_mps.max(top);
                s.moving_time_s +=
                    moving_time(&t, &point_speeds(run, &t, params.move_window_s), params);
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

    if s.moving_time_s > 0.0 {
        s.avg_moving_speed_mps = s.distance_m / s.moving_time_s;
    }
    if s.total_time_s > 0.0 {
        s.avg_total_speed_mps = s.distance_m / s.total_time_s;
    }
    s
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

/// Speed at every point of a run, over a time window centred on the point.
///
/// Uses the receiver's Doppler speed (median over the window) when every point
/// has one, otherwise the net displacement across the window divided by its
/// duration. Net displacement is robust against position jitter.
fn point_speeds(run: &[TrackPoint], t: &[i64], window_s: f64) -> Vec<f64> {
    let n = run.len();
    let half_ms = (window_s * 500.0) as i64;
    let reported: Option<Vec<f64>> = run
        .iter()
        .map(|p| {
            p.speed_mps
                .map(f64::from)
                .filter(|v| v.is_finite() && *v >= 0.0)
        })
        .collect();

    let (mut a, mut b) = (0usize, 0usize);
    let mut out = Vec::with_capacity(n);
    for k in 0..n {
        while t[a] < t[k] - half_ms {
            a += 1;
        }
        while b + 1 < n && t[b + 1] <= t[k] + half_ms {
            b += 1;
        }
        let v = match &reported {
            Some(speeds) => median(&speeds[a..=b]),
            None => {
                // Window holds a single point (sparse fixes): use the neighbouring interval.
                let (i, j) = match (a == b, k > 0) {
                    (false, _) => (a, b),
                    (true, true) => (k - 1, k),
                    (true, false) => (k, (k + 1).min(n - 1)),
                };
                let dt = (t[j] - t[i]) as f64 / 1000.0;
                if dt > 0.0 {
                    haversine_m(run[i].lat, run[i].lon, run[j].lat, run[j].lon) / dt
                } else {
                    0.0
                }
            }
        };
        out.push(v);
    }
    out
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

/// Time in motion with hysteresis: a slowdown shorter than `stop_delay_s` still
/// counts as moving, a longer one does not count at all.
fn moving_time(t: &[i64], speeds: &[f64], params: &StatsParams) -> f64 {
    let mut total = 0.0;
    let mut moving = false;
    let mut pending = 0.0;
    for k in 1..t.len() {
        let dt = (t[k] - t[k - 1]) as f64 / 1000.0;
        if dt > params.max_gap_s {
            moving = false;
            pending = 0.0;
            continue;
        }
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
