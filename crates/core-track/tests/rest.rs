#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use core_track::{RestDetector, RestEvent, RestParams, TrackPoint};

const M_PER_DEG_LAT: f64 = 111_195.08;
const T0: i64 = 1_700_000_000_000;

/// A fix `north_m` metres north of a fixed spot at `t_s` seconds, with the given accuracy.
fn fix(north_m: f64, t_s: i64, accuracy_m: Option<f32>) -> TrackPoint {
    let mut p = TrackPoint::new(47.0 + north_m / M_PER_DEG_LAT, 11.0);
    p.time_ms = Some(T0 + t_s * 1000);
    p.h_accuracy_m = accuracy_m;
    p
}

/// Feeds fixes every 10 s from `from_s` to `to_s` (inclusive) at `north_m`, returning the
/// events seen.
fn stand(
    d: &mut RestDetector,
    north_m: f64,
    from_s: i64,
    to_s: i64,
    accuracy_m: Option<f32>,
) -> Vec<(i64, RestEvent)> {
    (from_s..=to_s)
        .step_by(10)
        .filter_map(|t| d.offer(&fix(north_m, t, accuracy_m)).map(|e| (t, e)))
        .collect()
}

/// A detector that is resting at the spot `0 m`, the rest started at 300 s.
fn resting_detector() -> RestDetector {
    let mut d = RestDetector::new(RestParams::DEFAULT);
    assert_eq!(
        stand(&mut d, 0.0, 0, 300, Some(5.0)),
        [(300, RestEvent::Started)]
    );
    assert!(d.resting());
    d
}

#[test]
fn default_parameters_are_five_minutes_within_fifty_metres() {
    assert_eq!(RestParams::DEFAULT.dwell_ms, 300_000);
    assert_eq!(RestParams::DEFAULT.radius_m, 50.0);
}

#[test]
fn starts_after_dwell_within_radius() {
    let mut d = RestDetector::new(RestParams::DEFAULT);
    let mut events = Vec::new();
    for (k, t) in (0..=300).step_by(10).enumerate() {
        // jitter up to 20 m around the spot
        let north = if k % 2 == 0 { 20.0 } else { -15.0 };
        if let Some(e) = d.offer(&fix(north, t, Some(8.0))) {
            events.push((t, e));
        }
        assert_eq!(d.resting(), t >= 300, "at {t} s");
    }
    assert_eq!(events, [(300, RestEvent::Started)]);
}

#[test]
fn moving_resets_anchor() {
    let mut d = RestDetector::new(RestParams::DEFAULT);
    assert!(stand(&mut d, 0.0, 0, 190, None).is_empty());
    assert_eq!(d.offer(&fix(80.0, 200, None)), None);
    assert!(stand(&mut d, 80.0, 210, 490, None).is_empty());
    assert_eq!(d.offer(&fix(80.0, 500, None)), Some(RestEvent::Started));
}

#[test]
fn inaccurate_fixes_neither_start_nor_reset() {
    let mut d = RestDetector::new(RestParams::DEFAULT);
    // only inaccurate fixes: never a rest
    assert!(stand(&mut d, 0.0, 0, 900, Some(120.0)).is_empty());
    assert!(!d.resting());

    // a usable fix sets the rest point; inaccurate fixes far away do not move it
    let mut d = RestDetector::new(RestParams::DEFAULT);
    assert_eq!(d.offer(&fix(0.0, 0, Some(10.0))), None);
    assert!(stand(&mut d, 400.0, 10, 400, Some(120.0)).is_empty());
    assert_eq!(
        d.offer(&fix(10.0, 410, Some(10.0))),
        Some(RestEvent::Started)
    );
}

#[test]
fn ends_after_two_fixes_outside_radius_plus_accuracy() {
    let mut d = resting_detector();
    // 200 m away with accuracy 100: outside 50 + 100
    assert_eq!(d.offer(&fix(200.0, 310, Some(100.0))), None);
    // 60 m away with accuracy 100: not away, resets the count
    assert_eq!(d.offer(&fix(60.0, 320, Some(100.0))), None);
    assert!(d.resting());
    assert_eq!(d.offer(&fix(200.0, 330, Some(30.0))), None);
    assert!(d.resting());
    assert_eq!(
        d.offer(&fix(200.0, 340, Some(30.0))),
        Some(RestEvent::Ended)
    );
    assert!(!d.resting());
}

#[test]
fn single_jump_does_not_end_rest() {
    let mut d = resting_detector();
    for t in [310, 330, 350] {
        assert_eq!(d.offer(&fix(200.0, t, Some(5.0))), None, "away at {t}");
        assert_eq!(d.offer(&fix(5.0, t + 10, Some(5.0))), None, "back at {t}");
    }
    assert!(d.resting());
}

#[test]
fn reset_forgets_rest_point_and_rest() {
    let mut d = resting_detector();
    d.reset();
    assert!(!d.resting());
    assert!(stand(&mut d, 0.0, 310, 600, Some(5.0)).is_empty());
    assert_eq!(d.offer(&fix(0.0, 610, Some(5.0))), Some(RestEvent::Started));
}
