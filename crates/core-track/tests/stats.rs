#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use core_track::{
    Activity, Segment, SegmentReason, StatsParams, Track, TrackPoint, compute_section_stats,
    compute_stats,
};

/// Metres per degree of latitude on the sphere used by the crate.
const M_PER_DEG_LAT: f64 = 111_195.08;
const T0: i64 = 1_700_000_000_000;

/// `n` points heading north from `start_lat`, `step_m` apart, `dt_s` seconds apart.
fn north_line(start_lat: f64, n: usize, step_m: f64, t0_ms: i64, dt_s: f64) -> Vec<TrackPoint> {
    (0..n)
        .map(|i| {
            let mut p = TrackPoint::new(start_lat + i as f64 * step_m / M_PER_DEG_LAT, 11.0);
            p.time_ms = Some(t0_ms + (i as f64 * dt_s * 1000.0) as i64);
            p
        })
        .collect()
}

fn track(segments: Vec<Vec<TrackPoint>>) -> Track {
    Track {
        name: None,
        activity: None,
        segments: segments
            .into_iter()
            .map(|points| Segment {
                points,
                ..Default::default()
            })
            .collect(),
    }
}

fn bike() -> StatsParams {
    StatsParams::for_activity(Activity::Bike)
}

fn assert_close(actual: f64, expected: f64, tolerance: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{what}: expected {expected} ± {tolerance}, got {actual}"
    );
}

#[test]
fn straight_line_has_expected_distance_time_and_speed() {
    let t = track(vec![north_line(47.0, 101, 5.0, T0, 1.0)]);
    let s = compute_stats(&t, &bike());
    assert_close(s.distance_m, 500.0, 0.5, "distance");
    assert_close(s.total_time_s, 100.0, 1e-9, "total time");
    assert_close(s.moving_time_s, 100.0, 1.0, "moving time");
    assert_close(s.max_speed_mps, 5.0, 0.05, "max speed");
    assert_close(s.avg_moving_speed_mps, 5.0, 0.1, "avg moving speed");
    assert_eq!(s.points_total, 101);
    assert_eq!(s.points_used, 101);
}

#[test]
fn single_gps_jump_does_not_affect_distance_or_max_speed() {
    let mut pts = north_line(47.0, 101, 5.0, T0, 1.0);
    pts[50].lon += 0.01; // ~760 m sideways for one fix
    let s = compute_stats(&track(vec![pts]), &bike());
    assert_close(s.distance_m, 500.0, 1.0, "distance");
    assert_close(s.max_speed_mps, 5.0, 0.1, "max speed");
    assert_eq!(s.points_used, 100);
}

#[test]
fn persistent_relocation_reanchors_and_does_not_count_the_jump() {
    let mut pts = north_line(47.0, 50, 5.0, T0, 1.0);
    pts.extend(north_line(47.5, 50, 5.0, T0 + 50_000, 1.0));
    let s = compute_stats(&track(vec![pts]), &bike());
    // 49 intervals before the jump; 5 fixes rejected, the 6th becomes the new anchor.
    assert_close(s.distance_m, 49.0 * 5.0 + 44.0 * 5.0, 1.0, "distance");
    assert_eq!(s.points_used, 95);
    assert_close(s.max_speed_mps, 5.0, 0.1, "max speed");
}

#[test]
fn standstill_with_jitter_is_neither_moving_time_nor_distance() {
    let mut pts = north_line(47.0, 101, 5.0, T0, 1.0); // 100 s moving
    let rest_lat = pts[100].lat;
    for i in 1..=60 {
        // 60 s standing still, fix jitters ±1 m
        let jitter = if i % 2 == 0 { 1.0 } else { -1.0 } / M_PER_DEG_LAT;
        let mut p = TrackPoint::new(rest_lat + jitter, 11.0);
        p.time_ms = Some(T0 + (100 + i) * 1000);
        pts.push(p);
    }
    let mut second = north_line(rest_lat, 101, 5.0, T0 + 161_000, 1.0);
    second.remove(0);
    pts.extend(second); // another 100 s moving
    let s = compute_stats(&track(vec![pts]), &bike());
    assert_close(s.total_time_s, 261.0, 1e-9, "total time");
    assert_close(s.moving_time_s, 200.0, 12.0, "moving time");
    assert_close(s.distance_m, 1000.0, 10.0, "distance");
}

#[test]
fn short_slowdown_is_bridged_by_stop_delay() {
    let mut pts = north_line(47.0, 61, 5.0, T0, 1.0);
    let lat = pts[60].lat;
    for i in 1..=2 {
        let mut p = TrackPoint::new(lat, 11.0);
        p.time_ms = Some(T0 + (60 + i) * 1000);
        pts.push(p);
    }
    let mut rest = north_line(lat, 61, 5.0, T0 + 62_000, 1.0);
    rest.remove(0);
    pts.extend(rest);
    let mut params = bike();
    params.speed_window_s = 1.0;
    params.move_window_s = 1.0;
    params.stop_delay_s = 5.0;
    let s = compute_stats(&track(vec![pts]), &params);
    assert_close(s.moving_time_s, 122.0, 0.01, "moving time");
}

#[test]
fn noisy_ramp_yields_true_ascent_and_descent() {
    let mut pts = north_line(47.0, 401, 5.0, T0, 1.0);
    for (i, p) in pts.iter_mut().enumerate() {
        let true_ele = if i <= 200 {
            1000.0 + 0.5 * i as f64
        } else {
            1100.0 - 0.2 * (i - 200) as f64
        };
        let noise = if i % 2 == 0 { 1.5 } else { -1.5 };
        p.ele = Some(true_ele + noise);
    }
    let s = compute_stats(&track(vec![pts]), &bike());
    assert_close(s.ascent_m, 100.0, 6.0, "ascent");
    assert_close(s.descent_m, 40.0, 6.0, "descent");
    assert_close(s.max_ele_m.unwrap(), 1100.0, 3.0, "max elevation");
    assert_close(s.min_ele_m.unwrap(), 1000.0, 3.0, "min elevation");
}

#[test]
fn flat_track_with_elevation_noise_has_no_ascent() {
    let mut pts = north_line(47.0, 301, 5.0, T0, 1.0);
    for (i, p) in pts.iter_mut().enumerate() {
        p.ele = Some(500.0 + [2.0, -1.0, 0.5, -2.0, 1.0][i % 5]);
    }
    let s = compute_stats(&track(vec![pts]), &bike());
    assert_eq!(s.ascent_m, 0.0);
    assert_eq!(s.descent_m, 0.0);
}

#[test]
fn invalid_and_out_of_order_points_are_dropped() {
    let mut pts = north_line(47.0, 21, 5.0, T0, 1.0);
    pts[5].lat = f64::NAN;
    pts[8].lon = 500.0;
    pts[12].time_ms = Some(T0); // goes back in time
    let s = compute_stats(&track(vec![pts]), &bike());
    assert_eq!(s.points_used, 18);
    assert_close(s.distance_m, 100.0, 0.5, "distance");
}

#[test]
fn out_of_order_time_is_detected_across_points_without_time() {
    let mut pts = north_line(47.0, 3, 5.0, T0, 1.0);
    pts[0].time_ms = Some(T0 + 10_000);
    pts[1].time_ms = None;
    pts[2].time_ms = Some(T0);
    let s = compute_stats(&track(vec![pts]), &bike());
    assert_eq!(s.points_used, 2);
    assert_eq!(s.total_time_s, 0.0);
}

#[test]
fn fixes_with_poor_accuracy_are_dropped() {
    let mut pts = north_line(47.0, 21, 5.0, T0, 1.0);
    pts[10].h_accuracy_m = Some(120.0);
    pts[10].lon += 0.0005;
    let s = compute_stats(&track(vec![pts]), &bike());
    assert_eq!(s.points_used, 20);
    assert_close(s.distance_m, 100.0, 0.5, "distance");
}

#[test]
fn pause_between_segments_counts_neither_time_nor_distance() {
    let a = north_line(47.0, 11, 5.0, T0, 1.0);
    let b = north_line(47.1, 11, 5.0, T0 + 3_600_000, 1.0);
    let s = compute_stats(&track(vec![a, b]), &bike());
    assert_close(s.total_time_s, 20.0, 1e-9, "total time");
    assert_close(s.distance_m, 100.0, 0.5, "distance");
}

/// Two 50 m stretches at 5 m/s around a gap without fixes.
fn track_with_gap(gap_m: f64, gap_s: i64) -> Track {
    let mut pts = north_line(47.0, 11, 5.0, T0, 1.0);
    let resume_lat = 47.0 + (50.0 + gap_m) / M_PER_DEG_LAT;
    pts.extend(north_line(
        resume_lat,
        11,
        5.0,
        T0 + (10 + gap_s) * 1000,
        1.0,
    ));
    track(vec![pts])
}

#[test]
fn gap_without_fixes_counts_distance_and_moving_time_at_average_pace() {
    // 300 s without fixes, 600 m further on: at the track's 5 m/s that is 120 s of riding
    let s = compute_stats(&track_with_gap(600.0, 300), &bike());
    assert_close(s.distance_m, 700.0, 1.0, "distance");
    assert_close(s.total_time_s, 320.0, 1e-9, "total time");
    assert_close(s.moving_time_s, 140.0, 2.0, "moving time");
    assert_close(s.avg_moving_speed_mps, 5.0, 0.1, "avg moving speed");
}

#[test]
fn gap_moving_time_never_exceeds_the_gap_duration() {
    // 400 m in 40 s is faster than the rest of the track: the whole gap counts
    let s = compute_stats(&track_with_gap(400.0, 40), &bike());
    assert_close(s.moving_time_s, 60.0, 1.0, "moving time");
}

#[test]
fn gap_without_progress_adds_no_moving_time() {
    let s = compute_stats(&track_with_gap(0.0, 600), &bike());
    assert_close(s.distance_m, 100.0, 1.0, "distance");
    assert_close(s.moving_time_s, 20.0, 1.0, "moving time");
}

#[test]
fn irregular_timestamps_do_not_inflate_max_speed() {
    // Steady 10 m/s with a fix every 20 m, but one timestamp is a second late
    // (seen in real phone recordings): intervals of 2, 2, 3, 1, 2, 2 … seconds.
    let mut pts = north_line(47.0, 41, 20.0, T0, 2.0);
    pts[20].time_ms = Some(T0 + 41_000);
    let s = compute_stats(&track(vec![pts]), &bike());
    assert_eq!(
        s.points_used, 41,
        "the position is fine, only its time is off"
    );
    assert_close(s.max_speed_mps, 10.0, 1.0, "max speed");
    assert_close(s.distance_m, 800.0, 1.0, "distance");
}

#[test]
fn compressed_timestamps_on_a_plausible_path_keep_every_point() {
    // Seen in real recordings: no progress for several seconds, then 150 m within
    // one second. The positions lie on the route; only the times are bunched up.
    let mut pts = north_line(47.0, 60, 10.0, T0, 2.0);
    for (i, p) in pts.iter_mut().enumerate().skip(30).take(8) {
        p.time_ms = Some(T0 + 74_000 + (i as i64 - 30) * 100);
    }
    let s = compute_stats(&track(vec![pts]), &bike());
    assert_eq!(s.points_used, 60);
    assert_close(s.distance_m, 590.0, 1.0, "distance");
    assert_close(s.max_speed_mps, 5.0, 1.5, "max speed");
}

#[test]
fn small_sideways_spike_is_removed_even_below_the_speed_limit() {
    // 25 m off the path for one fix: only 25 m/s, but out and back again.
    let mut pts = north_line(47.0, 101, 5.0, T0, 1.0);
    pts[50].lon += 25.0 / (M_PER_DEG_LAT * 47.0f64.to_radians().cos());
    let s = compute_stats(&track(vec![pts]), &bike());
    assert_eq!(s.points_used, 100);
    assert_close(s.distance_m, 500.0, 1.0, "distance");
}

#[test]
fn receiver_speed_is_preferred_over_position_derived_speed() {
    let mut pts = north_line(47.0, 31, 5.0, T0, 1.0);
    for p in &mut pts {
        p.speed_mps = Some(7.0);
    }
    let s = compute_stats(&track(vec![pts]), &bike());
    assert_close(s.max_speed_mps, 7.0, 1e-6, "max speed");
}

#[test]
fn track_without_timestamps_has_distance_but_no_time_statistics() {
    let mut pts = north_line(47.0, 21, 5.0, T0, 1.0);
    for p in &mut pts {
        p.time_ms = None;
    }
    let s = compute_stats(&track(vec![pts]), &bike());
    assert_close(s.distance_m, 100.0, 0.5, "distance");
    assert_eq!(s.total_time_s, 0.0);
    assert_eq!(s.moving_time_s, 0.0);
    assert_eq!(s.max_speed_mps, 0.0);
    assert_eq!(s.avg_moving_speed_mps, 0.0);
}

#[test]
fn empty_and_single_point_tracks_yield_zero_statistics() {
    for t in [
        track(vec![]),
        track(vec![vec![]]),
        track(vec![north_line(47.0, 1, 5.0, T0, 1.0)]),
    ] {
        let s = compute_stats(&t, &bike());
        assert_eq!(s.distance_m, 0.0);
        assert_eq!(s.total_time_s, 0.0);
        assert_eq!(s.ascent_m, 0.0);
        assert_eq!(s.min_ele_m, None);
    }
}

#[test]
fn every_activity_has_parameters_and_a_stable_name() {
    for a in Activity::ALL {
        let p = StatsParams::for_activity(a);
        assert!(p.v_move_mps > 0.0 && p.max_plausible_speed_mps > p.v_move_mps);
        assert_eq!(a.as_str().parse::<Activity>().unwrap(), a);
    }
    assert!("unicycle".parse::<Activity>().is_err());
}

mod properties {
    use super::*;
    use proptest::prelude::*;

    fn arb_point() -> impl Strategy<Value = TrackPoint> {
        (
            -91.0f64..91.0,
            -181.0f64..181.0,
            proptest::option::of(-500.0f64..9000.0),
            proptest::option::of(0i64..100_000),
            proptest::option::of(0.0f32..200.0),
        )
            .prop_map(|(lat, lon, ele, t, acc)| {
                let mut p = TrackPoint::new(lat, lon);
                p.ele = ele;
                p.time_ms = t.map(|t| T0 + t * 100);
                p.h_accuracy_m = acc;
                p
            })
    }

    proptest! {
        #[test]
        fn statistics_are_finite_and_consistent_for_arbitrary_input(
            segs in proptest::collection::vec(proptest::collection::vec(arb_point(), 0..60), 0..4),
            activity in 0usize..4,
        ) {
            let params = StatsParams::for_activity(Activity::ALL[activity]);
            let s = compute_stats(&track(segs), &params);
            for v in [s.distance_m, s.ascent_m, s.descent_m, s.max_speed_mps,
                      s.avg_moving_speed_mps, s.avg_total_speed_mps, s.total_time_s, s.moving_time_s] {
                prop_assert!(v.is_finite() && v >= 0.0, "{s:?}");
            }
            prop_assert!(s.moving_time_s <= s.total_time_s + 1e-6, "{s:?}");
            prop_assert!(s.points_used <= s.points_total);
            prop_assert!(s.max_speed_mps <= params.max_plausible_speed_mps + 1e-6, "{s:?}");
        }
    }
}

#[test]
fn elevation_gain_of_a_profile_ignores_ripples_below_the_threshold() {
    use core_track::elevation_gain;
    // 2 km: 100 m up, 40 m down, with a 1 m ripple on top.
    let profile: Vec<(f64, f64)> = (0..=200)
        .map(|i| {
            let d = f64::from(i) * 10.0;
            let base = if d <= 1000.0 {
                500.0 + d / 10.0
            } else {
                600.0 - (d - 1000.0) / 25.0
            };
            (d, base + if i % 2 == 0 { 0.5 } else { -0.5 })
        })
        .collect();
    let (up, down) = elevation_gain(&profile, 50.0, 3.0);
    assert!((up - 100.0).abs() < 3.0, "ascent {up}");
    assert!((down - 40.0).abs() < 3.0, "descent {down}");
    assert_eq!(elevation_gain(&[], 50.0, 3.0), (0.0, 0.0));
    assert_eq!(elevation_gain(&[(0.0, 700.0)], 50.0, 3.0), (0.0, 0.0));
}

fn section(points: Vec<TrackPoint>, activity: Activity, reason: SegmentReason) -> Segment {
    Segment {
        points,
        activity: Some(activity),
        reason,
    }
}

/// Slow walking (2 km/h, below the bike threshold), then crawling: the activity's
/// parameters matter.
fn slow_walk(start_lat: f64, t0_ms: i64) -> Vec<TrackPoint> {
    let mut pts = north_line(start_lat, 120, 0.6, t0_ms, 1.0);
    let lat = pts[119].lat;
    let mut slow = north_line(lat, 60, 0.15, t0_ms + 120_000, 1.0);
    slow.remove(0);
    pts.extend(slow);
    pts
}

/// Moving north with elevation rising 0.5 m per step.
fn climb(start_lat: f64, t0_ms: i64) -> Vec<TrackPoint> {
    let mut pts = north_line(start_lat, 201, 5.0, t0_ms, 1.0);
    for (i, p) in pts.iter_mut().enumerate() {
        p.ele = Some(1000.0 + 0.5 * i as f64);
    }
    pts
}

#[test]
fn sections_use_their_own_params() {
    let hike_pts = slow_walk(47.0, T0);
    let bike_pts = slow_walk(47.0, T0);
    let t = Track {
        name: None,
        activity: Some(Activity::Bike),
        segments: vec![
            section(hike_pts.clone(), Activity::Hike, SegmentReason::Start),
            section(
                bike_pts.clone(),
                Activity::Bike,
                SegmentReason::ActivityChange,
            ),
        ],
    };
    let hike = StatsParams::for_activity(Activity::Hike);
    let alone_hike = compute_stats(&track(vec![hike_pts]), &hike);
    let alone_bike = compute_stats(&track(vec![bike_pts]), &bike());
    assert!(
        (alone_hike.moving_time_s - alone_bike.moving_time_s).abs() > 10.0,
        "the profiles must differ on this track: {alone_hike:?} {alone_bike:?}"
    );

    let (total, sections) = compute_section_stats(&t, StatsParams::for_activity);
    assert_eq!(sections, [alone_hike.clone(), alone_bike.clone()]);
    let sum = |f: fn(&core_track::TrackStats) -> f64| f(&alone_hike) + f(&alone_bike);
    assert_close(
        total.moving_time_s,
        sum(|s| s.moving_time_s),
        1e-9,
        "moving",
    );
    assert_close(total.total_time_s, sum(|s| s.total_time_s), 1e-9, "total");
    assert_close(total.distance_m, sum(|s| s.distance_m), 1e-9, "distance");
    assert_close(
        total.avg_moving_speed_mps,
        total.distance_m / total.moving_time_s,
        1e-9,
        "average",
    );
    assert_eq!(total.points_total, 2 * alone_hike.points_total);
}

#[test]
fn rest_counts_total_time_only() {
    let ride = climb(47.0, T0);
    let end = ride[200];
    let rest: Vec<TrackPoint> = (1..=61)
        .map(|k| {
            // jittery fixes around the rest spot, far off in elevation
            let mut p = TrackPoint::new(end.lat + (k % 3) as f64 * 10.0 / M_PER_DEG_LAT, 11.0);
            p.ele = Some(900.0 + (k % 2) as f64 * 50.0);
            p.time_ms = Some(T0 + 200_000 + k * 60_000);
            p
        })
        .collect();
    let without = Track {
        name: None,
        activity: Some(Activity::Bike),
        segments: vec![section(ride.clone(), Activity::Bike, SegmentReason::Start)],
    };
    let mut with = without.clone();
    with.segments
        .push(section(rest, Activity::Bike, SegmentReason::Rest));

    let (before, _) = compute_section_stats(&without, StatsParams::for_activity);
    let (after, sections) = compute_section_stats(&with, StatsParams::for_activity);
    assert_close(
        after.total_time_s,
        before.total_time_s + 3600.0,
        1e-9,
        "total time",
    );
    assert_close(
        after.moving_time_s,
        before.moving_time_s,
        1e-9,
        "moving time",
    );
    assert_close(after.distance_m, before.distance_m, 1e-9, "distance");
    assert_close(after.ascent_m, before.ascent_m, 1e-9, "ascent");
    assert_close(after.descent_m, before.descent_m, 1e-9, "descent");
    assert_eq!(after.min_ele_m, before.min_ele_m);
    assert_eq!(after.max_speed_mps, before.max_speed_mps);
    assert_eq!(sections[1].distance_m, 0.0);
    assert_eq!(sections[1].moving_time_s, 0.0);
    assert_close(sections[1].total_time_s, 3600.0, 1e-9, "rest time");
}

#[test]
fn rest_end_adds_straight_line() {
    let ride = north_line(47.0, 101, 5.0, T0, 1.0);
    let spot = ride[100];
    let rest: Vec<TrackPoint> = (1..=10)
        .map(|k| {
            let mut p = spot;
            p.time_ms = Some(T0 + 100_000 + k * 60_000);
            p
        })
        .collect();
    let on = north_line(
        spot.lat + 120.0 / M_PER_DEG_LAT,
        101,
        5.0,
        T0 + 760_000,
        1.0,
    );
    let t = Track {
        name: None,
        activity: Some(Activity::Bike),
        segments: vec![
            section(ride.clone(), Activity::Bike, SegmentReason::Start),
            section(rest, Activity::Bike, SegmentReason::Rest),
            section(on.clone(), Activity::Bike, SegmentReason::RestEnd),
        ],
    };
    let d_ride = compute_stats(&track(vec![ride]), &bike()).distance_m;
    let d_on = compute_stats(&track(vec![on]), &bike()).distance_m;
    let (total, sections) = compute_section_stats(&t, StatsParams::for_activity);
    assert_close(total.distance_m, d_ride + d_on + 120.0, 1.0, "distance");
    assert_close(
        sections[2].distance_m,
        d_on + 120.0,
        1.0,
        "section after the rest",
    );
}
