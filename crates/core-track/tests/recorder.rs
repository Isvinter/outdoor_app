#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use core_track::{
    Activity, Recorder, RecorderState, RestParams, Segment, SegmentReason, StatsParams, Track,
    TrackError, TrackPoint,
};

fn point(i: i64) -> TrackPoint {
    let mut p = TrackPoint::new(47.0 + i as f64 * 5.0 / 111_195.08, 11.0);
    p.time_ms = Some(1_700_000_000_000 + i * 1000);
    p
}

#[test]
fn new_recorder_records_points_into_the_first_segment() {
    let mut r = Recorder::start(Activity::Hike);
    assert_eq!(r.state(), RecorderState::Recording);
    assert!(r.add_point(point(0)));
    assert!(r.add_point(point(1)));
    assert_eq!(r.track().activity, Some(Activity::Hike));
    assert_eq!(r.track().segments.len(), 1);
    assert_eq!(r.track().segments[0].points, [point(0), point(1)]);
}

#[test]
fn points_are_ignored_while_paused_and_resume_starts_a_new_segment() {
    let mut r = Recorder::start(Activity::Bike);
    r.add_point(point(0));
    r.pause().unwrap();
    assert_eq!(r.state(), RecorderState::Paused);
    assert!(!r.add_point(point(1)));
    r.resume().unwrap();
    assert!(r.add_point(point(2)));
    let lens: Vec<usize> = r.track().segments.iter().map(|s| s.points.len()).collect();
    assert_eq!(lens, [1, 1]);
    assert_eq!(r.current_segment(), 1);
}

#[test]
fn resuming_without_points_in_between_does_not_leave_empty_segments() {
    let mut r = Recorder::start(Activity::Bike);
    r.pause().unwrap();
    r.resume().unwrap();
    r.add_point(point(0));
    assert_eq!(r.track().segments.len(), 1);
    assert_eq!(r.current_segment(), 0);
}

#[test]
fn invalid_duplicate_and_out_of_order_fixes_are_rejected() {
    let mut r = Recorder::start(Activity::Bike);
    assert!(r.add_point(point(5)));
    assert!(!r.add_point(point(5)), "same timestamp");
    assert!(!r.add_point(point(3)), "older timestamp");
    assert!(!r.add_point(TrackPoint::new(f64::NAN, 11.0)));
    assert!(!r.add_point(TrackPoint::new(47.0, 181.0)));
    let mut untimed = point(9);
    untimed.time_ms = None;
    assert!(!r.add_point(untimed), "a live fix must carry a time");
    assert_eq!(r.track().segments[0].points.len(), 1);
}

#[test]
fn stop_returns_the_track_and_ends_the_recording() {
    let mut r = Recorder::start(Activity::Bike);
    r.add_point(point(0));
    r.stop().unwrap();
    assert_eq!(r.state(), RecorderState::Stopped);
    assert!(!r.add_point(point(1)));
    assert_eq!(r.track().segments[0].points.len(), 1);
}

#[test]
fn transitions_that_make_no_sense_are_errors() {
    let mut r = Recorder::start(Activity::Bike);
    assert_eq!(
        r.resume(),
        Err(TrackError::InvalidTransition {
            state: "recording",
            action: "resume"
        })
    );
    r.pause().unwrap();
    assert_eq!(
        r.pause(),
        Err(TrackError::InvalidTransition {
            state: "paused",
            action: "pause"
        })
    );
    r.stop().unwrap();
    assert!(r.stop().is_err());
    assert!(r.pause().is_err());
    assert!(r.resume().is_err());
}

#[test]
fn recording_can_be_continued_from_a_stored_track_in_a_new_segment() {
    let stored = Track {
        name: Some("before the crash".into()),
        activity: Some(Activity::Mtb),
        segments: vec![Segment {
            points: vec![point(0), point(1)],
            ..Default::default()
        }],
    };
    let mut r = Recorder::continue_from(stored, Activity::Mtb);
    assert_eq!(r.state(), RecorderState::Recording);
    assert!(!r.add_point(point(1)), "must not go back in time");
    assert!(r.add_point(point(60)));
    assert_eq!(r.track().segments.len(), 2);
    assert_eq!(r.track().segments[1].points, [point(60)]);
    assert_eq!(r.track().segments[1].reason, SegmentReason::Continue);
    assert_eq!(r.track().name.as_deref(), Some("before the crash"));
}

#[test]
fn live_statistics_reflect_the_points_recorded_so_far() {
    let mut r = Recorder::start(Activity::Bike);
    for i in 0..=20 {
        r.add_point(point(i));
    }
    let s = r.stats(&StatsParams::for_activity(Activity::Bike));
    assert!((s.distance_m - 100.0).abs() < 0.5, "{s:?}");
    assert_eq!(s.total_time_s, 20.0);
}

/// A fix `north_m` metres north of the start at `t_s` seconds.
fn fix_at(north_m: f64, t_s: i64) -> TrackPoint {
    let mut p = TrackPoint::new(47.0 + north_m / 111_195.08, 11.0);
    p.time_ms = Some(1_700_000_000_000 + t_s * 1000);
    p
}

/// 60 s riding north at 5 m/s, then standing at 300 m for 6 min with a fix every 10 s.
/// The last rest point (an anchor) while riding is the fix at 55 s, 275 m: the rest is
/// detected with the first fix 5 min later, at 360 s.
fn ride_and_stand(r: &mut Recorder) {
    for t in 0..=60 {
        assert!(r.add_point(fix_at(5.0 * t as f64, t)));
    }
    for t in (70..=420).step_by(10) {
        assert!(r.add_point(fix_at(300.0, t)));
    }
}

/// Riding on north at 5 m/s from the rest spot, from 421 s to 480 s.
fn ride_on(r: &mut Recorder) {
    for t in 421..=480 {
        assert!(r.add_point(fix_at(300.0 + 5.0 * (t - 420) as f64, t)));
    }
}

fn reasons(r: &Recorder) -> Vec<SegmentReason> {
    r.track().segments.iter().map(|s| s.reason).collect()
}

fn activities(r: &Recorder) -> Vec<Activity> {
    (0..r.track().segments.len())
        .map(|i| r.track().segment_activity(i))
        .collect()
}

#[test]
fn switch_activity_starts_segment() {
    let mut r = Recorder::start(Activity::Bike);
    r.add_point(point(0));
    r.add_point(point(1));
    r.switch_activity(Activity::Mtb);
    assert_eq!(r.activity(), Activity::Mtb);
    r.add_point(point(2));
    r.add_point(point(3));
    assert_eq!(r.track().segments.len(), 2);
    assert_eq!(activities(&r), [Activity::Bike, Activity::Mtb]);
    assert_eq!(
        reasons(&r),
        [SegmentReason::Start, SegmentReason::ActivityChange]
    );
    assert_eq!(r.track().activity, Some(Activity::Bike), "first section");
}

#[test]
fn rest_becomes_own_segment() {
    let mut r = Recorder::start(Activity::Bike);
    r.set_rest_detection(Some(RestParams::DEFAULT));
    ride_and_stand(&mut r);
    assert!(r.resting());
    ride_on(&mut r);
    assert!(!r.resting());
    assert_eq!(
        reasons(&r),
        [
            SegmentReason::Start,
            SegmentReason::Rest,
            SegmentReason::RestEnd
        ]
    );
    let segments = &r.track().segments;
    assert_eq!(segments[0].points.last(), Some(&fix_at(300.0, 350)));
    assert_eq!(segments[1].points.first(), Some(&fix_at(300.0, 360)));
    assert!(segments[1].is_rest());
    assert!(!segments[2].is_rest());
    assert_eq!(activities(&r), [Activity::Bike; 3]);
    assert_eq!(r.activity(), Activity::Bike);
    assert_eq!(r.current_segment(), 2);
}

#[test]
fn rest_off_by_default() {
    let mut r = Recorder::start(Activity::Bike);
    ride_and_stand(&mut r);
    ride_on(&mut r);
    assert!(!r.resting());
    assert_eq!(reasons(&r), [SegmentReason::Start]);
}

#[test]
fn pause_during_rest_ends_it() {
    let mut r = Recorder::start(Activity::Bike);
    r.set_rest_detection(Some(RestParams::DEFAULT));
    ride_and_stand(&mut r);
    assert!(r.resting());
    r.pause().unwrap();
    r.resume().unwrap();
    assert!(r.add_point(fix_at(300.0, 430)));
    assert!(!r.resting());
    assert_eq!(reasons(&r).last(), Some(&SegmentReason::Resume));
    assert_eq!(r.track().segment_activity(2), Activity::Bike);
}

#[test]
fn switch_during_rest_ends_it() {
    let mut r = Recorder::start(Activity::Bike);
    r.set_rest_detection(Some(RestParams::DEFAULT));
    ride_and_stand(&mut r);
    r.switch_activity(Activity::Hike);
    assert!(!r.resting());
    assert!(r.add_point(fix_at(300.0, 430)));
    assert_eq!(reasons(&r).last(), Some(&SegmentReason::ActivityChange));
    assert_eq!(r.track().segment_activity(2), Activity::Hike);
    assert!(!r.resting());
}

#[test]
fn switch_before_first_point_sets_first_segment() {
    let mut r = Recorder::start(Activity::Bike);
    r.switch_activity(Activity::Hike);
    assert!(r.track().segments.is_empty());
    r.add_point(point(0));
    assert_eq!(r.track().segments.len(), 1);
    assert_eq!(r.track().segment_activity(0), Activity::Hike);
    assert_eq!(r.track().segments[0].reason, SegmentReason::Start);
    assert_eq!(r.track().activity, Some(Activity::Hike));
}

#[test]
fn gap_break_records_its_reason() {
    let mut r = Recorder::start(Activity::Bike);
    r.add_point(point(0));
    r.break_segment(SegmentReason::Gap);
    r.add_point(point(200));
    assert_eq!(reasons(&r), [SegmentReason::Start, SegmentReason::Gap]);
}

#[test]
fn segment_reason_has_stable_names() {
    use std::str::FromStr;
    for reason in [
        SegmentReason::Start,
        SegmentReason::ActivityChange,
        SegmentReason::Resume,
        SegmentReason::Gap,
        SegmentReason::Rest,
        SegmentReason::RestEnd,
        SegmentReason::Continue,
    ] {
        assert_eq!(SegmentReason::from_str(reason.as_str()), Ok(reason));
    }
    assert_eq!(SegmentReason::RestEnd.as_str(), "rest_end");
    assert_eq!(SegmentReason::default(), SegmentReason::Start);
    assert!(SegmentReason::from_str("nap").is_err());
}
