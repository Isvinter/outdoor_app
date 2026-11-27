#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use core_app::{App, AppError, AppListener, MediaInput, RecordingStatus};
use core_location::{Fix, SourceStatus};
use core_storage::{MediaKind, MediaSource};
use core_track::{Activity, RestParams, SegmentReason};
use uuid::Uuid;

const T0: i64 = 1_800_000_000_000;

struct Quiet;

impl AppListener for Quiet {
    fn on_position(&self, _: &Fix) {}
    fn on_recording(&self, _: &RecordingStatus) {}
    fn on_source_status(&self, _: &SourceStatus) {}
    fn on_error(&self, _: &str) {}
}

fn temp_db(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("core-app-media-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("app.sqlite")
}

fn open(name: &str) -> App {
    App::open(&temp_db(name), Arc::new(Quiet)).unwrap()
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

/// Standing at place `place` (as in [`fix`]) at second `t`.
fn at(place: i64, t: i64) -> Fix {
    Fix {
        time_ms: T0 + t * 1000,
        ..fix(place)
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

/// Writes a file into the media folder and returns its path relative to the data folder.
fn media_file(app: &App, name: &str) -> String {
    std::fs::write(app.media_dir().join(name), b"data").unwrap();
    format!("media/{name}")
}

fn photo(app: &App, name: &str) -> Uuid {
    let file = media_file(app, name);
    app.add_recording_media(MediaKind::Photo, MediaSource::Camera, Some(&file), None)
        .unwrap()
        .record
        .id
}

fn note(app: &App, text: &str) -> Uuid {
    app.add_recording_media(MediaKind::Text, MediaSource::Typed, None, Some(text))
        .unwrap()
        .record
        .id
}

fn section_of(app: &App, track: Uuid, media: Uuid) -> Option<usize> {
    app.track_detail(track)
        .unwrap()
        .media
        .iter()
        .find(|m| m.record.id == media)
        .expect("medium is listed")
        .section
}

fn library_input(file: String, time_ms: Option<i64>, place: Option<(f64, f64)>) -> MediaInput {
    MediaInput {
        kind: MediaKind::Photo,
        source: MediaSource::Library,
        file: Some(file),
        text: None,
        time_ms,
        lat: place.map(|p| p.0),
        lon: place.map(|p| p.1),
    }
}

fn text_input(text: &str) -> MediaInput {
    MediaInput {
        kind: MediaKind::Text,
        source: MediaSource::Typed,
        file: None,
        text: Some(text.into()),
        time_ms: None,
        lat: None,
        lon: None,
    }
}

/// A finished track: 0..10 as bike, 20..30 as hike.
fn finished_track(app: &App) -> Uuid {
    let id = app
        .start_recording(Activity::Bike)
        .unwrap()
        .track_id
        .unwrap();
    for i in 0..10 {
        app.handle_fix(fix(i));
    }
    app.switch_recording_activity(Activity::Hike).unwrap();
    for i in 20..30 {
        app.handle_fix(fix(i));
    }
    app.stop_recording().unwrap().unwrap();
    id
}

#[test]
fn photo_during_recording_goes_to_running_section() {
    let app = open("running");
    let track = app
        .start_recording(Activity::Bike)
        .unwrap()
        .track_id
        .unwrap();
    for i in 0..3 {
        app.handle_fix(fix(i));
    }
    let before = now_ms();
    let file = media_file(&app, "a.jpg");
    let added = app
        .add_recording_media(MediaKind::Photo, MediaSource::Camera, Some(&file), None)
        .unwrap();
    assert_eq!(added.section, Some(0));
    let r = &added.record;
    assert_eq!((r.lat, r.lon), (Some(fix(2).lat), Some(fix(2).lon)));
    let time = r.time_ms.unwrap();
    assert!(time >= before && time <= now_ms() + 1, "{time}");
    assert_eq!(r.file.as_deref(), Some("media/a.jpg"));

    let detail = app.track_detail(track).unwrap();
    assert_eq!(detail.media.len(), 1);
    assert_eq!(detail.media[0], added);
}

#[test]
fn before_first_fix_is_whole_tour() {
    let app = open("before-fix");
    let track = app
        .start_recording(Activity::Bike)
        .unwrap()
        .track_id
        .unwrap();
    let added = app
        .add_recording_media(MediaKind::Text, MediaSource::Typed, None, Some("Los"))
        .unwrap();
    assert_eq!(added.section, None);
    assert_eq!((added.record.lat, added.record.lon), (None, None));
    assert_eq!(section_of(&app, track, added.record.id), None);
}

#[test]
fn without_recording_is_an_error() {
    let app = open("no-recording");
    let result = app.add_recording_media(MediaKind::Text, MediaSource::Typed, None, Some("x"));
    assert!(matches!(result, Err(AppError::NoRecording)));
}

#[test]
fn during_pause_goes_to_last_section() {
    let app = open("pause");
    let track = app
        .start_recording(Activity::Bike)
        .unwrap()
        .track_id
        .unwrap();
    for i in 0..5 {
        app.handle_fix(fix(i));
    }
    app.pause_recording().unwrap();
    let paused = photo(&app, "p.jpg");
    app.resume_recording().unwrap();
    for i in 10..15 {
        app.handle_fix(fix(i));
    }
    let later = photo(&app, "q.jpg");
    assert_eq!(section_of(&app, track, paused), Some(0));
    assert_eq!(section_of(&app, track, later), Some(1));
}

#[test]
fn after_switch_before_fix_goes_to_previous_section() {
    let app = open("switch");
    let track = app
        .start_recording(Activity::Bike)
        .unwrap()
        .track_id
        .unwrap();
    for i in 0..5 {
        app.handle_fix(fix(i));
    }
    app.switch_recording_activity(Activity::Hike).unwrap();
    let early = note(&app, "gleich nach dem Wechsel");
    app.handle_fix(fix(10));
    let late = note(&app, "nach dem ersten Fix");
    assert_eq!(section_of(&app, track, early), Some(0));
    assert_eq!(section_of(&app, track, late), Some(1));
}

#[test]
fn during_rest_goes_to_rest_section() {
    let app = open("rest");
    let track = app
        .start_recording(Activity::Bike)
        .unwrap()
        .track_id
        .unwrap();
    app.set_rest_detection(Some(RestParams::DEFAULT));
    for i in 0..=10 {
        app.handle_fix(fix(i));
    }
    for t in 11..=370 {
        app.handle_fix(at(10, t));
    }
    assert!(app.recording_status().resting);
    let id = note(&app, "Brotzeit");
    let detail = app.track_detail(track).unwrap();
    let section = section_of(&app, track, id).unwrap();
    assert_eq!(detail.sections[section].reason, SegmentReason::Rest);
}

#[test]
fn continued_recording_uses_last_stored_section() {
    let path = temp_db("continue");
    let (track, before) = {
        let app = App::open(&path, Arc::new(Quiet)).unwrap();
        let track = app
            .start_recording(Activity::Mtb)
            .unwrap()
            .track_id
            .unwrap();
        for i in 0..5 {
            app.handle_fix(fix(i));
        }
        let before = note(&app, "vor dem Absturz");
        (track, before)
        // the app goes away without stopping the recording
    };
    let app = App::open(&path, Arc::new(Quiet)).unwrap();
    app.continue_interrupted().unwrap();
    let resumed = note(&app, "gleich nach dem Fortsetzen");
    app.handle_fix(fix(100));
    let later = note(&app, "danach");
    assert_eq!(section_of(&app, track, before), Some(0));
    assert_eq!(section_of(&app, track, resumed), Some(0));
    assert_eq!(section_of(&app, track, later), Some(1));
    assert_eq!(app.track_detail(track).unwrap().media.len(), 3);
}

#[test]
fn index_survives_empty_recorder_segments() {
    let app = open("empty-segments");
    let track = app
        .start_recording(Activity::Bike)
        .unwrap()
        .track_id
        .unwrap();
    for i in 0..5 {
        app.handle_fix(fix(i));
    }
    for _ in 0..2 {
        app.pause_recording().unwrap();
        app.resume_recording().unwrap();
    }
    for i in 10..15 {
        app.handle_fix(fix(i));
    }
    let id = photo(&app, "e.jpg");
    let detail = app.track_detail(track).unwrap();
    assert_eq!(detail.sections.len(), 2);
    assert_eq!(section_of(&app, track, id), Some(1));
    assert_eq!(detail.sections[1].start_ms, fix(10).time_ms);
}

#[test]
fn library_photo_without_time_takes_section_start() {
    let app = open("library-untimed");
    let track = finished_track(&app);
    let file = media_file(&app, "screenshot.png");
    let added = app
        .add_track_media(track, Some(1), library_input(file, None, None))
        .unwrap();
    assert_eq!(added.section, Some(1));
    assert_eq!(added.record.time_ms, Some(fix(20).time_ms));
    assert_eq!((added.record.lat, added.record.lon), (None, None));
    assert_eq!(added.record.source, MediaSource::Library);
}

#[test]
fn library_photo_keeps_given_time_and_place() {
    let app = open("library-timed");
    let track = finished_track(&app);
    let file = media_file(&app, "b.heic");
    let added = app
        .add_track_media(
            track,
            Some(0),
            library_input(file, Some(T0 + 4000), Some((47.1, 11.2))),
        )
        .unwrap();
    assert_eq!(added.record.time_ms, Some(T0 + 4000));
    assert_eq!(
        (added.record.lat, added.record.lon),
        (Some(47.1), Some(11.2))
    );
}

#[test]
fn whole_tour_without_time_takes_track_start() {
    let app = open("whole-tour");
    let track = finished_track(&app);
    let added = app
        .add_track_media(track, None, text_input("Schöner Tag"))
        .unwrap();
    assert_eq!(added.section, None);
    assert_eq!(added.record.time_ms, Some(fix(0).time_ms));
}

#[test]
fn add_track_media_rejects_bad_index() {
    let app = open("bad-index");
    let track = finished_track(&app);
    let result = app.add_track_media(track, Some(2), text_input("x"));
    assert!(
        matches!(result, Err(AppError::InvalidSection)),
        "{result:?}"
    );
    assert!(app.track_detail(track).unwrap().media.is_empty());
}

#[test]
fn rejects_bad_combinations() {
    let app = open("bad-combinations");
    let track = finished_track(&app);
    let file = media_file(&app, "v.m4a");
    let bad = [
        MediaInput {
            text: None,
            ..text_input("")
        },
        text_input("   "),
        MediaInput {
            file: None,
            ..library_input(file.clone(), None, None)
        },
        MediaInput {
            kind: MediaKind::Voice,
            source: MediaSource::Camera,
            ..library_input(file.clone(), None, None)
        },
        MediaInput {
            source: MediaSource::Spoken,
            ..library_input(file.clone(), None, None)
        },
        MediaInput {
            file: Some(file),
            ..text_input("mit Datei")
        },
    ];
    for input in bad {
        let result = app.add_track_media(track, Some(0), input.clone());
        assert!(
            matches!(result, Err(AppError::InvalidMedia)),
            "{input:?}: {result:?}"
        );
        assert_eq!(result.unwrap_err().code(), "invalid_media");
    }
    assert!(app.track_detail(track).unwrap().media.is_empty());
}

#[test]
fn rejects_files_outside_media_dir() {
    let app = open("outside");
    let track = finished_track(&app);
    std::fs::create_dir_all(app.media_dir().join("a")).unwrap();
    std::fs::write(app.media_dir().join("a").join("b.jpg"), b"x").unwrap();
    for file in [
        "../app.sqlite",
        "media/../app.sqlite",
        "/etc/hosts",
        "media/a/b.jpg",
        "media/a",
        "media/fehlt.jpg",
        "app.sqlite",
        "",
    ] {
        let result = app.add_track_media(track, Some(0), library_input(file.into(), None, None));
        assert!(
            matches!(result, Err(AppError::MediaFile(_))),
            "{file}: {result:?}"
        );
        assert_eq!(result.unwrap_err().code(), "media_file");
    }
    assert!(app.track_detail(track).unwrap().media.is_empty());
}

#[test]
fn stop_keeps_track_with_media_but_no_points() {
    let app = open("no-points");
    let track = app
        .start_recording(Activity::Hike)
        .unwrap()
        .track_id
        .unwrap();
    note(&app, "drinnen, kein GPS");
    let record = app.stop_recording().unwrap().expect("kept for its note");
    assert_eq!(record.id, track);
    let detail = app.track_detail(track).unwrap();
    assert_eq!(detail.media.len(), 1);
    assert_eq!(detail.media[0].section, None);
    assert!(app.list_tracks().unwrap().iter().any(|t| t.id == track));

    app.start_recording(Activity::Hike).unwrap();
    assert!(app.stop_recording().unwrap().is_none(), "nothing to keep");
}

#[test]
fn delete_media_removes_file() {
    let app = open("delete-media");
    let track = finished_track(&app);
    let file = media_file(&app, "d.jpg");
    let id = app
        .add_track_media(track, Some(0), library_input(file, None, None))
        .unwrap()
        .record
        .id;
    app.delete_media(id).unwrap();
    assert!(!app.media_dir().join("d.jpg").exists());
    assert!(app.track_detail(track).unwrap().media.is_empty());
}

#[test]
fn delete_media_with_missing_file_succeeds() {
    let app = open("delete-missing");
    let track = finished_track(&app);
    let file = media_file(&app, "m.jpg");
    let id = app
        .add_track_media(track, Some(0), library_input(file, None, None))
        .unwrap()
        .record
        .id;
    std::fs::remove_file(app.media_dir().join("m.jpg")).unwrap();
    app.delete_media(id).unwrap();
    assert!(app.track_detail(track).unwrap().media.is_empty());
}

#[test]
fn delete_track_removes_media_files() {
    let app = open("delete-track");
    let track = finished_track(&app);
    let file = media_file(&app, "t.jpg");
    app.add_track_media(track, Some(0), library_input(file, None, None))
        .unwrap();
    let other = media_file(&app, "other.jpg");
    app.delete_track(track).unwrap();
    assert!(!app.media_dir().join("t.jpg").exists());
    assert!(app.media_dir().join("other.jpg").exists(), "{other}");
}

#[test]
fn edit_text_keeps_transcribed() {
    let app = open("transcribed");
    let track = finished_track(&app);
    let file = media_file(&app, "v.m4a");
    let id = app
        .add_track_media(
            track,
            Some(0),
            MediaInput {
                kind: MediaKind::Voice,
                source: MediaSource::Spoken,
                ..library_input(file, None, None)
            },
        )
        .unwrap()
        .record
        .id;
    let done = app.set_media_transcript(id, "Wir sind oben").unwrap();
    assert!(done.record.transcribed);
    assert_eq!(done.record.text.as_deref(), Some("Wir sind oben"));
    let edited = app.set_media_text(id, "Wir sind am Gipfel").unwrap();
    assert!(edited.record.transcribed);
    assert_eq!(edited.record.text.as_deref(), Some("Wir sind am Gipfel"));
    assert_eq!(edited.section, Some(0));
    let cleared = app.set_media_text(id, "").unwrap();
    assert_eq!(cleared.record.text, None);
    assert_eq!(cleared.record.file.as_deref(), Some("media/v.m4a"));
}

#[test]
fn transcript_only_for_voice() {
    let app = open("transcript-text");
    let track = finished_track(&app);
    let id = app
        .add_track_media(track, Some(0), text_input("getippt"))
        .unwrap()
        .record
        .id;
    assert!(matches!(
        app.set_media_transcript(id, "x"),
        Err(AppError::InvalidMedia)
    ));
    assert!(matches!(
        app.set_media_text(id, " "),
        Err(AppError::InvalidMedia)
    ));
    assert_eq!(
        app.set_media_text(id, "neu")
            .unwrap()
            .record
            .text
            .as_deref(),
        Some("neu")
    );
}
