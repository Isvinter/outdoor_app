#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use core_track::{
    Activity, Recorder, RecorderState, Segment, StatsParams, Track, TrackError, TrackPoint,
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
        }],
    };
    let mut r = Recorder::continue_from(stored, Activity::Mtb);
    assert_eq!(r.state(), RecorderState::Recording);
    assert!(!r.add_point(point(1)), "must not go back in time");
    assert!(r.add_point(point(60)));
    assert_eq!(r.track().segments.len(), 2);
    assert_eq!(r.track().segments[1].points, [point(60)]);
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
