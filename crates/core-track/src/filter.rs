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

    for p in points {
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
            if too_fast || too_abrupt {
                rejected_in_a_row += 1;
                if rejected_in_a_row > params.max_reject_run {
                    runs.push(std::mem::take(&mut current));
                    current.push(*p);
                    last_time = p.time_ms;
                    prev_speed = None;
                    rejected_in_a_row = 0;
                }
                continue;
            }
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
