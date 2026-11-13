#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use core_app::{App, AppError, AppListener, RecordingPhase, RecordingStatus};
use core_gpx::parse_gpx;
use core_location::{Fix, GpxReplaySource, SourceStatus};
use core_storage::TrackState;
use core_track::{Activity, StatsParams, compute_stats};

const REAL_TRACK: &str = include_str!("../../../fixtures/gpx/real/tour_sylvensteinsee.gpx");
const T0: i64 = 1_800_000_000_000;

#[derive(Default)]
struct Events {
    positions: Vec<Fix>,
    recordings: Vec<RecordingStatus>,
    sources: Vec<SourceStatus>,
    errors: Vec<String>,
}

#[derive(Default)]
struct Listener(Mutex<Events>);

impl AppListener for Listener {
    fn on_position(&self, fix: &Fix) {
        self.0.lock().unwrap().positions.push(*fix);
    }
    fn on_recording(&self, status: &RecordingStatus) {
        self.0.lock().unwrap().recordings.push(status.clone());
    }
    fn on_source_status(&self, status: &SourceStatus) {
        self.0.lock().unwrap().sources.push(status.clone());
    }
    fn on_error(&self, message: &str) {
        self.0.lock().unwrap().errors.push(message.to_owned());
    }
}

fn temp_db(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("core-app-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("app.sqlite")
}

fn open(name: &str) -> (App, Arc<Listener>, PathBuf) {
    let path = temp_db(name);
    let listener = Arc::new(Listener::default());
    (App::open(&path, listener.clone()).unwrap(), listener, path)
}

/// A fix `i` seconds after T0, 5 m further north each second.
fn fix(i: i64) -> Fix {
    Fix {
        lat: 47.0 + i as f64 * 5.0 / 111_195.08,
        lon: 11.0,
        ele: Some(700.0),
        time_ms: T0 + i * 1000,
        h_accuracy_m: Some(4.0),
        speed_mps: None,
        course_deg: None,
    }
}

fn wait_for(what: &str, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn fixes_update_the_position_but_are_not_recorded_while_idle() {
    let (app, listener, _) = open("idle");
    app.handle_fix(fix(0));
    assert_eq!(listener.0.lock().unwrap().positions, [fix(0)]);
    assert_eq!(app.last_position(), Some(fix(0)));
    assert_eq!(app.recording_status().phase, RecordingPhase::Idle);
    assert!(app.list_tracks().unwrap().is_empty());
}

#[test]
fn recording_stores_fixes_and_stop_yields_a_finished_track_with_statistics() {
    let (app, _listener, _) = open("record");
    let status = app.start_recording(Activity::Bike).unwrap();
    assert_eq!(status.phase, RecordingPhase::Recording);
    for i in 0..=100 {
        app.handle_fix(fix(i));
    }
    let live = app.recording_status();
    assert!((live.stats.as_ref().unwrap().distance_m - 500.0).abs() < 1.0);
    assert_eq!(
        app.recording_line()
            .iter()
            .map(Vec::len)
            .collect::<Vec<_>>(),
        [101]
    );

    let record = app
        .stop_recording()
        .unwrap()
        .expect("track with points is kept");
    assert_eq!(record.activity, Activity::Bike);
    assert!((record.stats.as_ref().unwrap().distance_m - 500.0).abs() < 1.0);
    assert_eq!(app.recording_status().phase, RecordingPhase::Idle);
    assert_eq!(app.list_tracks().unwrap(), std::slice::from_ref(&record));

    let detail = app.track_detail(record.id).unwrap();
    assert_eq!(detail.line.len(), 1);
    assert_eq!(detail.line[0].len(), 101);
    assert_eq!(detail.line[0][0], [11.0, 47.0]);
}

#[test]
fn pause_drops_fixes_and_resume_continues_in_a_new_segment() {
    let (app, _listener, _) = open("pause");
    app.start_recording(Activity::Bike).unwrap();
    for i in 0..10 {
        app.handle_fix(fix(i));
    }
    assert_eq!(app.pause_recording().unwrap().phase, RecordingPhase::Paused);
    for i in 10..20 {
        app.handle_fix(fix(i));
    }
    assert_eq!(
        app.resume_recording().unwrap().phase,
        RecordingPhase::Recording
    );
    for i in 20..30 {
        app.handle_fix(fix(i));
    }
    let record = app.stop_recording().unwrap().unwrap();
    let lens: Vec<usize> = app
        .track_detail(record.id)
        .unwrap()
        .line
        .iter()
        .map(Vec::len)
        .collect();
    assert_eq!(lens, [10, 10]);
}

#[test]
fn recording_keeps_the_location_interval_of_the_activity() {
    let (app, _listener, _) = open("interval");
    app.start_recording(Activity::Hike).unwrap(); // one fix every 3 s
    for i in 0..=30 {
        app.handle_fix(fix(i));
    }
    assert_eq!(app.recording_line()[0].len(), 11);
}

#[test]
fn recording_without_points_is_discarded_on_stop() {
    let (app, _listener, _) = open("empty");
    app.start_recording(Activity::Bike).unwrap();
    assert_eq!(app.stop_recording().unwrap(), None);
    assert!(app.list_tracks().unwrap().is_empty());
}

#[test]
fn recording_commands_in_the_wrong_state_are_errors() {
    let (app, _listener, _) = open("wrong-state");
    assert!(matches!(app.pause_recording(), Err(AppError::NoRecording)));
    assert!(matches!(app.resume_recording(), Err(AppError::NoRecording)));
    assert!(matches!(app.stop_recording(), Err(AppError::NoRecording)));
    app.start_recording(Activity::Bike).unwrap();
    assert!(matches!(
        app.start_recording(Activity::Bike),
        Err(AppError::RecordingActive)
    ));
    assert!(matches!(app.resume_recording(), Err(AppError::Track(_))));
}

#[test]
fn listener_gets_recording_updates_with_the_new_line_points() {
    let (app, listener, _) = open("updates");
    app.start_recording(Activity::Bike).unwrap();
    for i in 0..5 {
        app.handle_fix(fix(i));
    }
    app.stop_recording().unwrap();
    let events = listener.0.lock().unwrap();
    let appended: usize = events.recordings.iter().map(|r| r.appended.len()).sum();
    assert_eq!(
        appended, 5,
        "every recorded point is announced exactly once"
    );
    assert_eq!(
        events.recordings.first().unwrap().phase,
        RecordingPhase::Recording
    );
    assert_eq!(
        events.recordings.last().unwrap().phase,
        RecordingPhase::Idle
    );
    assert!(events.errors.is_empty(), "{:?}", events.errors);
}

#[test]
fn interrupted_recording_survives_a_restart_and_can_be_continued() {
    let path = temp_db("crash");
    {
        let app = App::open(&path, Arc::new(Listener::default())).unwrap();
        assert!(app.interrupted_recording().unwrap().is_none());
        app.start_recording(Activity::Mtb).unwrap();
        for i in 0..50 {
            app.handle_fix(fix(i));
        }
        // the app goes away without stopping the recording
    }
    let app = App::open(&path, Arc::new(Listener::default())).unwrap();
    let interrupted = app
        .interrupted_recording()
        .unwrap()
        .expect("unfinished track is offered");
    assert_eq!(interrupted.activity, Activity::Mtb);
    assert_eq!(app.recording_status().phase, RecordingPhase::Idle);

    let status = app.continue_interrupted().unwrap();
    assert_eq!(
        (status.phase, status.track_id),
        (RecordingPhase::Recording, Some(interrupted.id))
    );
    for i in 100..150 {
        app.handle_fix(fix(i));
    }
    let record = app.stop_recording().unwrap().unwrap();
    assert_eq!(record.id, interrupted.id);
    let lens: Vec<usize> = app
        .track_detail(record.id)
        .unwrap()
        .line
        .iter()
        .map(Vec::len)
        .collect();
    assert_eq!(lens, [50, 50]);
    assert!(app.interrupted_recording().unwrap().is_none());
}

#[test]
fn interrupted_recording_can_be_finished_as_it_is() {
    let path = temp_db("crash-finish");
    {
        let app = App::open(&path, Arc::new(Listener::default())).unwrap();
        app.start_recording(Activity::Bike).unwrap();
        for i in 0..=20 {
            app.handle_fix(fix(i));
        }
    }
    let app = App::open(&path, Arc::new(Listener::default())).unwrap();
    let record = app.finish_interrupted().unwrap().unwrap();
    assert!((record.stats.unwrap().distance_m - 100.0).abs() < 0.5);
    assert!(app.interrupted_recording().unwrap().is_none());
    assert_eq!(app.list_tracks().unwrap().len(), 1);
}

#[test]
fn starting_anew_finishes_the_interrupted_recording_first() {
    let path = temp_db("crash-start");
    {
        let app = App::open(&path, Arc::new(Listener::default())).unwrap();
        app.start_recording(Activity::Bike).unwrap();
        for i in 0..=20 {
            app.handle_fix(fix(i));
        }
    }
    let app = App::open(&path, Arc::new(Listener::default())).unwrap();

    let status = app.start_recording(Activity::Hike).unwrap();

    // The old recording is a finished track of its own, the new one is the only open one.
    assert!(app.interrupted_recording().unwrap().is_none());
    let tracks = app.list_tracks().unwrap();
    assert_eq!(tracks.len(), 2);
    let old = tracks
        .iter()
        .find(|t| Some(t.id) != status.track_id)
        .unwrap();
    assert_eq!(old.state, TrackState::Finished);
    assert!((old.stats.as_ref().unwrap().distance_m - 100.0).abs() < 0.5);
}

#[test]
fn gpx_import_stores_tracks_and_export_reproduces_them() {
    let (app, _listener, _) = open("gpx");
    let imported = app.import_gpx(REAL_TRACK, None).unwrap();
    assert_eq!(imported.len(), 1);
    assert_eq!(imported[0].activity, Activity::Bike);
    assert_eq!(imported[0].name.as_deref(), Some("Tour"));
    assert!(imported[0].stats.as_ref().unwrap().distance_m > 27_000.0);

    let exported = parse_gpx(&app.export_gpx(imported[0].id).unwrap()).unwrap();
    let original = parse_gpx(REAL_TRACK).unwrap();
    assert_eq!(
        exported.tracks[0].segments.len(),
        original.tracks[0].segments.len()
    );
    let (a, b) = (
        &exported.tracks[0].segments[0].points,
        &original.tracks[0].segments[0].points,
    );
    assert_eq!(a.len(), b.len());
    for (p, q) in a.iter().zip(b) {
        assert!((p.lat - q.lat).abs() < 1e-7 && (p.lon - q.lon).abs() < 1e-7);
        assert_eq!(p.time_ms, q.time_ms);
    }
}

#[test]
fn gpx_import_needs_an_activity_from_the_file_or_the_caller() {
    let (app, _listener, _) = open("gpx-activity");
    let untyped = r#"<gpx><trk><trkseg><trkpt lat="47" lon="11"><time>2026-01-01T00:00:00Z</time></trkpt></trkseg></trk></gpx>"#;
    assert!(matches!(
        app.import_gpx(untyped, None),
        Err(AppError::ActivityRequired)
    ));
    assert!(app.list_tracks().unwrap().is_empty());
    let imported = app.import_gpx(untyped, Some(Activity::SkiTour)).unwrap();
    assert_eq!(imported[0].activity, Activity::SkiTour);
    assert!(matches!(
        app.import_gpx("nonsense", None),
        Err(AppError::Gpx(_))
    ));
}

#[test]
fn tracks_can_be_renamed_and_deleted() {
    let (app, _listener, _) = open("manage");
    let id = app.import_gpx(REAL_TRACK, None).unwrap()[0].id;
    app.rename_track(id, "Sylvenstein").unwrap();
    assert_eq!(
        app.list_tracks().unwrap()[0].name.as_deref(),
        Some("Sylvenstein")
    );
    app.delete_track(id).unwrap();
    assert!(app.list_tracks().unwrap().is_empty());
    assert!(matches!(app.track_detail(id), Err(AppError::Storage(_))));
}

/// The end-to-end path of milestone 2: GPX replay → recording → export.
#[test]
fn replayed_real_track_is_recorded_and_exported_faithfully() {
    let original = parse_gpx(REAL_TRACK).unwrap().tracks.remove(0);
    let points = original.segments[0].points.clone();
    let (app, listener, _) = open("replay");

    app.start_recording(Activity::Bike).unwrap();
    app.set_location_source(Box::new(GpxReplaySource::new(
        points.clone(),
        f64::INFINITY,
    )))
    .unwrap();
    wait_for("end of replay", || {
        listener
            .0
            .lock()
            .unwrap()
            .sources
            .contains(&SourceStatus::Finished)
    });
    app.stop_location_source();
    let record = app.stop_recording().unwrap().unwrap();
    assert!(listener.0.lock().unwrap().errors.is_empty());

    let exported = parse_gpx(&app.export_gpx(record.id).unwrap())
        .unwrap()
        .tracks
        .remove(0);
    let recorded = &exported.segments[0].points;
    assert_eq!(exported.segments.len(), 1);
    assert_eq!(recorded.len(), points.len());
    let shift = recorded[0].time_ms.unwrap() - points[0].time_ms.unwrap();
    for (r, o) in recorded.iter().zip(&points) {
        assert!((r.lat - o.lat).abs() < 1e-7 && (r.lon - o.lon).abs() < 1e-7);
        assert!((r.ele.unwrap() - o.ele.unwrap()).abs() <= 0.05);
        assert_eq!(
            r.time_ms.unwrap() - o.time_ms.unwrap(),
            shift,
            "spacing is preserved"
        );
    }

    let expected = compute_stats(&original, &StatsParams::for_activity(Activity::Bike));
    let stats = record.stats.unwrap();
    assert!((stats.distance_m - expected.distance_m).abs() < 1.0);
    assert!((stats.moving_time_s - expected.moving_time_s).abs() < 1.0);
    assert!((stats.ascent_m - expected.ascent_m).abs() < 1.0);
    assert_eq!(stats.points_total, expected.points_total);
}
