use crate::geo::haversine_m;
use crate::{StatsParams, TrackPoint};

fn has_valid_position(p: &TrackPoint) -> bool {
    p.lat.is_finite() && p.lon.is_finite() && p.lat.abs() <= 90.0 && p.lon.abs() <= 180.0
}

/// Removes invalid fixes and GPS jumps from one segment.
///
/// Returns the accepted points as one or more runs. A new run starts where the
/// position relocated for good (more than `max_reject_run` consecutive fixes far
/// from the last accepted one); no distance is counted between runs.
pub fn filter_segment(points: &[TrackPoint], params: &StatsParams) -> Vec<Vec<TrackPoint>> {
    let mut runs: Vec<Vec<TrackPoint>> = Vec::new();
    let mut current: Vec<TrackPoint> = Vec::new();
    let mut prev_speed: Option<f64> = None;
    let mut rejected_in_a_row = 0u32;
    let mut last_time: Option<i64> = None;

    for (index, p) in points.iter().enumerate() {
        if !has_valid_position(p) {
            continue;
        }
        if p.h_accuracy_m
            .is_some_and(|acc| f64::from(acc) > params.max_h_accuracy_m)
        {
            continue;
        }
        if let (Some(last), Some(t)) = (last_time, p.time_ms)
            && t <= last
        {
            continue;
        }
        if let Some(prev) = current.last()
            && let (Some(t0), Some(t1)) = (prev.time_ms, p.time_ms)
        {
            let dt = (t1 - t0) as f64 / 1000.0;
            let speed = haversine_m(prev.lat, prev.lon, p.lat, p.lon) / dt;
            let too_fast = speed > params.max_plausible_speed_mps;
            let too_abrupt = prev_speed.is_some_and(|v| (speed - v) / dt > params.max_accel_mps2);
            let judged = if too_fast || too_abrupt {
                // Implausible from one fix to the next. Still fine if only the
                // timestamps are bunched up and the position follows the path.
                sustained_speed(&current, p, points.get(index + 1), params)
            } else {
                Some(speed)
            };
            let Some(speed) = judged else {
                rejected_in_a_row += 1;
                if rejected_in_a_row > params.max_reject_run {
                    runs.push(std::mem::take(&mut current));
                    current.push(*p);
                    last_time = p.time_ms;
                    prev_speed = None;
                    rejected_in_a_row = 0;
                }
                continue;
            };
            prev_speed = Some(speed);
        }
        rejected_in_a_row = 0;
        last_time = p.time_ms.or(last_time);
        current.push(*p);
    }
    if !current.is_empty() {
        runs.push(current);
    }
    runs
}

/// Decides whether a fix that looks implausible against its predecessor is
/// nevertheless part of the track. Returns its speed over a longer window if so.
///
/// Phone recordings often carry correct positions with wrong times: no progress
/// for several seconds, then a hundred metres within one. Such a fix is kept if
/// - the average speed over the last `speed_window_s` seconds is plausible, and
/// - it is not a spike, i.e. the path does not go out to it and straight back.
fn sustained_speed(
    accepted: &[TrackPoint],
    p: &TrackPoint,
    next: Option<&TrackPoint>,
    params: &StatsParams,
) -> Option<f64> {
    let prev = accepted.last()?;
    let step = haversine_m(prev.lat, prev.lon, p.lat, p.lon);
    let t_p = p.time_ms?;

    let mut path = step;
    let mut span_s = 0.0;
    for pair in accepted.windows(2).rev() {
        let (a, b) = (&pair[0], &pair[1]);
        span_s = (t_p - b.time_ms?) as f64 / 1000.0;
        if span_s >= params.speed_window_s {
            break;
        }
        path += haversine_m(a.lat, a.lon, b.lat, b.lon);
        span_s = (t_p - a.time_ms?) as f64 / 1000.0;
    }
    if span_s < params.speed_window_s {
        return None; // too little history to tell
    }
    let speed = path / span_s;
    if speed > params.max_plausible_speed_mps {
        return None;
    }
    if let Some(n) = next.filter(|n| has_valid_position(n)) {
        let detour = step + haversine_m(p.lat, p.lon, n.lat, n.lon);
        let direct = haversine_m(prev.lat, prev.lon, n.lat, n.lon);
        if detour > 2.0 * direct + 2.0 * params.min_move_m {
            return None;
        }
    }
    Some(speed)
}
