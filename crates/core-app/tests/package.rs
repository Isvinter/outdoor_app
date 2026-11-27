#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;

use core_app::{
    App, AppListener, Elevation, MediaInput, PACKAGE_FORMAT, PACKAGE_VERSION, RecordingStatus,
    TourManifest, notes_markdown, package_path, tour_manifest,
};
use core_location::{Fix, SourceStatus};
use core_storage::{MediaKind, MediaRecord, MediaSource};
use core_track::{Activity, RestParams, SegmentReason};
use uuid::Uuid;

/// 2027-01-15 08:00:00 UTC.
const T0: i64 = 1_800_000_000_000;

struct Quiet;

impl AppListener for Quiet {
    fn on_position(&self, _: &Fix) {}
    fn on_recording(&self, _: &RecordingStatus) {}
    fn on_source_status(&self, _: &SourceStatus) {}
    fn on_error(&self, _: &str) {}
}

fn temp_db(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("core-app-package-{}-{name}", std::process::id()));
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

fn at(place: i64, t: i64) -> Fix {
    Fix {
        time_ms: T0 + t * 1000,
        ..fix(place)
    }
}

fn media_file(app: &App, name: &str) -> String {
    std::fs::write(app.media_dir().join(name), b"data").unwrap();
    format!("media/{name}")
}

fn add(
    app: &App,
    kind: MediaKind,
    source: MediaSource,
    file: Option<&str>,
    text: Option<&str>,
) -> Uuid {
    app.add_recording_media(kind, source, file, text)
        .unwrap()
        .record
        .id
}

fn all_present(_: &MediaRecord) -> bool {
    true
}

fn manifest(app: &App, track: Uuid) -> TourManifest {
    let detail = app.track_detail(track).unwrap();
    let ids = app.section_ids(track).unwrap();
    tour_manifest(&detail, &ids, &all_present, T0 + 99_000_000, 60)
}

/// A finished track: 0..10 as bike, a rest, then 400..410 as hike, with a
/// camera photo, a library image (HEIC), a voice note, and a text note before
/// the first fix (whole tour).
fn rich_track(app: &App) -> (Uuid, [Uuid; 4]) {
    let track = app
        .start_recording(Activity::Bike)
        .unwrap()
        .track_id
        .unwrap();
    app.set_rest_detection(Some(RestParams::DEFAULT));
    let text = add(
        app,
        MediaKind::Text,
        MediaSource::Typed,
        None,
        Some("Nebel am Start."),
    );
    for i in 0..=10 {
        app.handle_fix(fix(i));
    }
    let jpg = media_file(app, "cam.jpg");
    let photo = add(app, MediaKind::Photo, MediaSource::Camera, Some(&jpg), None);
    let m4a = media_file(app, "voice.m4a");
    let voice = add(app, MediaKind::Voice, MediaSource::Spoken, Some(&m4a), None);
    app.set_media_transcript(voice, "Abzweig verpasst.")
        .unwrap();
    for t in 11..=370 {
        app.handle_fix(at(10, t));
    }
    assert!(app.recording_status().resting);
    for i in 0..10 {
        app.handle_fix(at(100 + i * 4, 400 + i));
    }
    app.switch_recording_activity(Activity::Hike).unwrap();
    for i in 0..10 {
        app.handle_fix(at(200 + i * 4, 420 + i));
    }
    app.stop_recording().unwrap().unwrap();
    let heic = media_file(app, "lib.HEIC");
    let image = app
        .add_track_media(
            track,
            Some(0),
            MediaInput {
                kind: MediaKind::Photo,
                source: MediaSource::Library,
                file: Some(heic),
                text: None,
                time_ms: None,
                lat: None,
                lon: None,
            },
        )
        .unwrap()
        .record
        .id;
    (track, [text, photo, voice, image])
}

#[test]
fn manifest_has_stable_ids() {
    let app = open("ids");
    let (track, media) = rich_track(&app);
    let first = manifest(&app, track);
    let ids = app.section_ids(track).unwrap();
    assert_eq!(first.tour.id, track);
    assert_eq!(first.sections.len(), ids.len());
    for (section, id) in first.sections.iter().zip(&ids) {
        assert_eq!(section.id, *id);
    }
    let mut listed: Vec<Uuid> = first.media.iter().map(|m| m.id).collect();
    let mut expected = media.to_vec();
    listed.sort();
    expected.sort();
    assert_eq!(listed, expected);
    let second = manifest(&app, track);
    assert_eq!(first, second);
}

#[test]
fn manifest_round_trips_json() {
    let app = open("json");
    let (track, _) = rich_track(&app);
    let m = manifest(&app, track);
    assert_eq!(m.format, PACKAGE_FORMAT);
    assert_eq!(PACKAGE_FORMAT, "zerogrid-tour");
    assert_eq!(m.version, PACKAGE_VERSION);
    assert_eq!(PACKAGE_VERSION, 1);
    assert_eq!(m.utc_offset_min, 60);
    let json = serde_json::to_string(&m).unwrap();
    let back: TourManifest = serde_json::from_str(&json).unwrap();
    assert_eq!(back, m);
}

#[test]
fn rest_section_is_rest() {
    let app = open("rest");
    let (track, _) = rich_track(&app);
    let m = manifest(&app, track);
    let rest = m
        .sections
        .iter()
        .find(|s| s.reason == SegmentReason::Rest)
        .expect("a rest section");
    assert_eq!(rest.activity, "rest");
    assert_eq!(m.sections[0].activity, "bike");
    assert_eq!(m.sections.last().unwrap().activity, "hike");
    assert_eq!(m.tour.activities, vec![Activity::Bike, Activity::Hike]);
    assert_eq!(m.tour.activity, Activity::Bike);
    assert!(m.tour.stats.is_some());
    assert_eq!(m.tour.elevation, Elevation::Receiver);
}

#[test]
fn media_points_to_package_files() {
    let app = open("files");
    let (track, [text, photo, voice, image]) = rich_track(&app);
    let m = manifest(&app, track);
    let ids = app.section_ids(track).unwrap();
    let of = |id: Uuid| m.media.iter().find(|x| x.id == id).unwrap();
    assert_eq!(
        of(photo).file.as_deref(),
        Some(format!("fotos/{photo}.jpg").as_str())
    );
    assert_eq!(
        of(image).file.as_deref(),
        Some(format!("fotos/{image}.heic").as_str())
    );
    assert_eq!(
        of(voice).file.as_deref(),
        Some(format!("notizen/{voice}.m4a").as_str())
    );
    assert_eq!(of(text).file, None);
    assert_eq!(of(text).section_id, None);
    assert_eq!(of(photo).section_id, Some(ids[0]));
    assert_eq!(of(image).section_id, Some(ids[0]));
    assert_eq!(of(image).source, MediaSource::Library);
    assert_eq!(of(voice).text.as_deref(), Some("Abzweig verpasst."));
    assert!(of(voice).transcribed);
    assert_eq!(of(text).text.as_deref(), Some("Nebel am Start."));

    let record = app
        .track_detail(track)
        .unwrap()
        .media
        .into_iter()
        .find(|d| d.record.id == image)
        .unwrap()
        .record;
    assert_eq!(package_path(&record), Some(format!("fotos/{image}.heic")));
}

#[test]
fn missing_file_is_null() {
    let app = open("missing");
    let (track, [_, photo, ..]) = rich_track(&app);
    let detail = app.track_detail(track).unwrap();
    let ids = app.section_ids(track).unwrap();
    let m = tour_manifest(&detail, &ids, &|r: &MediaRecord| r.id != photo, T0, 0);
    let entry = m.media.iter().find(|x| x.id == photo).unwrap();
    assert_eq!(entry.file, None);
    assert_eq!(entry.kind, MediaKind::Photo);
    assert!(entry.time_ms.is_some());
}

const UNTIMED_GPX: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="test" xmlns="http://www.topografix.com/GPX/1/1">
  <trk><trkseg>
    <trkpt lat="47.0" lon="11.0"/>
    <trkpt lat="47.001" lon="11.0"/>
    <trkpt lat="47.002" lon="11.0"/>
  </trkseg></trk>
</gpx>
"#;

#[test]
fn untimed_track_has_null_times() {
    let app = open("untimed");
    let record = app
        .import_gpx(UNTIMED_GPX, Some(Activity::Hike))
        .unwrap()
        .remove(0);
    let m = manifest(&app, record.id);
    assert_eq!(m.tour.started_at_ms, None);
    assert_eq!(m.sections.len(), 1);
    assert_eq!(m.sections[0].start_ms, None);
    assert_eq!(m.sections[0].end_ms, None);
}

#[test]
fn markdown_layout() {
    let app = open("md");
    let (track, [_, photo, voice, _]) = rich_track(&app);
    app.rename_track(track, "Testtour").unwrap();
    let md = notes_markdown(&manifest(&app, track));
    let lines: Vec<&str> = md.lines().collect();
    assert_eq!(lines[0], "# Testtour");
    assert!(lines.contains(&"## Ganze Tour"), "{md}");
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("## Abschnitt 1 · Rad · 09:00–")),
        "{md}"
    );
    assert!(
        lines.iter().any(|l| l.starts_with("## Rast · 09:0")),
        "{md}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("## Abschnitt ") && l.contains(" · Wandern · ")),
        "{md}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.ends_with(" · Notiz: Nebel am Start.")),
        "{md}"
    );
    assert!(
        lines.iter().any(|l| l
            .get(2..7)
            .is_some_and(|t| l.ends_with(&format!(" · Foto: ![Foto {t}](fotos/{photo}.jpg)")))),
        "{md}"
    );
    assert!(
        lines.iter().any(|l| l.ends_with(&format!(
            " · Sprachnotiz: „Abzweig verpasst.“ ([Audio](notizen/{voice}.m4a))"
        ))),
        "{md}"
    );
    // Summary below the title: local start date, distance, activities.
    assert!(lines[2].starts_with("15.01.2027, 09:00–"), "{md}");
    assert!(lines[2].ends_with(" · Rad, Wandern"), "{md}");
}

#[test]
fn markdown_rest_section_comes_between() {
    let app = open("md-order");
    let (track, _) = rich_track(&app);
    let md = notes_markdown(&manifest(&app, track));
    let first = md.find("## Abschnitt 1 ").unwrap();
    let rest = md.find("## Rast · ").unwrap();
    let whole = md.find("## Ganze Tour").unwrap();
    assert!(whole < first && first < rest, "{md}");
}

#[test]
fn markdown_without_name_or_times() {
    let app = open("md-untimed");
    let record = app
        .import_gpx(UNTIMED_GPX, Some(Activity::Hike))
        .unwrap()
        .remove(0);
    let m = manifest(&app, record.id);
    let md = notes_markdown(&TourManifest {
        tour: core_app::TourInfo {
            name: None,
            ..m.tour.clone()
        },
        ..m
    });
    let lines: Vec<&str> = md.lines().collect();
    assert_eq!(lines[0], "# Tour");
    assert!(lines[2].starts_with("– · "), "{md}");
    assert!(
        lines.contains(&"## Abschnitt 1 · Wandern · – · 0,2 km · ↑ 0 m"),
        "{md}"
    );
    assert!(!md.contains("## Ganze Tour"), "{md}");
}

#[test]
fn markdown_keeps_multiline_text() {
    let app = open("md-multi");
    let track = app
        .start_recording(Activity::Bike)
        .unwrap()
        .track_id
        .unwrap();
    let text = "Zeile 1\n# Zeile 2 *x*";
    add(&app, MediaKind::Text, MediaSource::Typed, None, Some(text));
    app.stop_recording().unwrap().unwrap();
    let m = manifest(&app, track);
    assert_eq!(m.media[0].text.as_deref(), Some(text));
    let md = notes_markdown(&m);
    assert!(md.contains(" · Notiz: Zeile 1\n  # Zeile 2 *x*\n"), "{md}");
}

#[test]
fn markdown_missing_file() {
    let app = open("md-missing");
    let (track, [_, photo, ..]) = rich_track(&app);
    let detail = app.track_detail(track).unwrap();
    let ids = app.section_ids(track).unwrap();
    let m = tour_manifest(&detail, &ids, &|r: &MediaRecord| r.id != photo, T0, 60);
    let md = notes_markdown(&m);
    assert!(md.contains(" · Foto (Datei fehlt)"), "{md}");
}

// --- The ZIP -------------------------------------------------------------

fn zip_of(path: &std::path::Path) -> zip::ZipArchive<std::fs::File> {
    zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap()
}

fn names(archive: &zip::ZipArchive<std::fs::File>) -> Vec<String> {
    let mut names: Vec<String> = archive
        .file_names()
        .map(|n| n.unwrap().into_owned())
        .collect();
    names.sort();
    names
}

fn read_entry(archive: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Vec<u8> {
    let mut out = Vec::new();
    std::io::Read::read_to_end(&mut archive.by_name(name).unwrap(), &mut out).unwrap();
    out
}

fn dest(app: &App, name: &str) -> PathBuf {
    app.media_dir()
        .parent()
        .unwrap()
        .join(format!("{name}.zip"))
}

fn part_of(path: &std::path::Path) -> PathBuf {
    let mut name = path.file_name().unwrap().to_os_string();
    name.push(".part");
    path.with_file_name(name)
}

#[test]
fn zip_has_expected_entries() {
    let app = open("zip-entries");
    let (track, [_, photo, voice, image]) = rich_track(&app);
    let out = dest(&app, "tour");
    app.export_tour(track, &out, 60).unwrap();
    let mut archive = zip_of(&out);
    let mut expected = vec![
        "track.gpx".to_owned(),
        "tour.json".to_owned(),
        "notizen.md".to_owned(),
        format!("fotos/{photo}.jpg"),
        format!("fotos/{image}.heic"),
        format!("notizen/{voice}.m4a"),
    ];
    expected.sort();
    assert_eq!(names(&archive), expected);
    for name in [format!("fotos/{photo}.jpg"), format!("notizen/{voice}.m4a")] {
        assert_eq!(
            archive.by_name(&name).unwrap().compression(),
            zip::CompressionMethod::Stored
        );
    }
    for name in ["track.gpx", "tour.json", "notizen.md"] {
        assert_eq!(
            archive.by_name(name).unwrap().compression(),
            zip::CompressionMethod::Deflated
        );
    }
}

#[test]
fn zip_content_matches_sources() {
    let app = open("zip-content");
    let (track, [_, photo, ..]) = rich_track(&app);
    std::fs::write(app.media_dir().join("cam.jpg"), b"\xff\xd8 real jpeg bytes").unwrap();
    let out = dest(&app, "tour");
    app.export_tour(track, &out, 60).unwrap();
    let mut archive = zip_of(&out);
    let gpx = String::from_utf8(read_entry(&mut archive, "track.gpx")).unwrap();
    assert_eq!(gpx, app.export_gpx(track).unwrap());
    assert_eq!(
        read_entry(&mut archive, &format!("fotos/{photo}.jpg")),
        b"\xff\xd8 real jpeg bytes"
    );
    let manifest: TourManifest =
        serde_json::from_slice(&read_entry(&mut archive, "tour.json")).unwrap();
    assert_eq!(manifest.tour.id, track);
    assert_eq!(manifest.utc_offset_min, 60);
    let md = String::from_utf8(read_entry(&mut archive, "notizen.md")).unwrap();
    assert_eq!(md, notes_markdown(&manifest));
}

#[test]
fn deleted_media_is_left_out() {
    let app = open("zip-deleted");
    let (track, [_, photo, ..]) = rich_track(&app);
    app.delete_media(photo).unwrap();
    let out = dest(&app, "tour");
    app.export_tour(track, &out, 0).unwrap();
    let mut archive = zip_of(&out);
    assert!(
        !names(&archive)
            .iter()
            .any(|n| n.contains(&photo.to_string()))
    );
    let manifest: TourManifest =
        serde_json::from_slice(&read_entry(&mut archive, "tour.json")).unwrap();
    assert!(manifest.media.iter().all(|m| m.id != photo));
}

#[test]
fn missing_file_still_exports() {
    let app = open("zip-missing");
    let (track, [_, photo, ..]) = rich_track(&app);
    std::fs::remove_file(app.media_dir().join("cam.jpg")).unwrap();
    let out = dest(&app, "tour");
    app.export_tour(track, &out, 0).unwrap();
    let mut archive = zip_of(&out);
    assert!(!names(&archive).contains(&format!("fotos/{photo}.jpg")));
    let manifest: TourManifest =
        serde_json::from_slice(&read_entry(&mut archive, "tour.json")).unwrap();
    let entry = manifest.media.iter().find(|m| m.id == photo).unwrap();
    assert_eq!(entry.file, None);
}

#[test]
fn plain_track_has_no_media_folders() {
    let app = open("zip-plain");
    let record = app
        .import_gpx(UNTIMED_GPX, Some(Activity::Hike))
        .unwrap()
        .remove(0);
    let out = dest(&app, "tour");
    app.export_tour(record.id, &out, 0).unwrap();
    assert_eq!(
        names(&zip_of(&out)),
        vec!["notizen.md", "tour.json", "track.gpx"]
    );
}

#[test]
fn track_without_points_but_notes_exports() {
    let app = open("zip-no-points");
    let track = app
        .start_recording(Activity::Bike)
        .unwrap()
        .track_id
        .unwrap();
    add(
        &app,
        MediaKind::Text,
        MediaSource::Typed,
        None,
        Some("Drinnen"),
    );
    app.stop_recording().unwrap().unwrap();
    let out = dest(&app, "tour");
    app.export_tour(track, &out, 0).unwrap();
    let mut archive = zip_of(&out);
    let md = String::from_utf8(read_entry(&mut archive, "notizen.md")).unwrap();
    assert!(md.contains("Notiz: Drinnen"), "{md}");
}

#[test]
fn recording_is_not_exported() {
    let app = open("zip-recording");
    let track = app
        .start_recording(Activity::Bike)
        .unwrap()
        .track_id
        .unwrap();
    app.handle_fix(fix(0));
    let out = dest(&app, "tour");
    let err = app.export_tour(track, &out, 0).unwrap_err();
    assert_eq!(err.code(), "recording_active");
    assert!(!out.exists());
}

#[test]
fn export_while_recording_other_track() {
    let app = open("zip-other");
    let (done, _) = rich_track(&app);
    let running = app
        .start_recording(Activity::Hike)
        .unwrap()
        .track_id
        .unwrap();
    for i in 0..5 {
        app.handle_fix(fix(1000 + i));
    }
    let points = |app: &App| {
        app.track_detail(running)
            .unwrap()
            .line
            .iter()
            .map(Vec::len)
            .sum::<usize>()
    };
    let before = points(&app);
    let out = dest(&app, "tour");
    app.export_tour(done, &out, 0).unwrap();
    for i in 5..60 {
        app.handle_fix(fix(1000 + i));
    }
    assert_eq!(app.recording_status().track_id, Some(running));
    assert!(points(&app) > before, "recording goes on after the export");
}

#[test]
fn failed_export_leaves_nothing() {
    let app = open("zip-fail");
    let (track, _) = rich_track(&app);
    let out = app
        .media_dir()
        .parent()
        .unwrap()
        .join("fehlt")
        .join("tour.zip");
    let err = app.export_tour(track, &out, 0).unwrap_err();
    assert_eq!(err.code(), "export_failed");
    assert!(!out.exists());
    assert!(!part_of(&out).exists());
}

#[test]
fn existing_dest_is_replaced() {
    let app = open("zip-replace");
    let (track, _) = rich_track(&app);
    let out = dest(&app, "tour");
    std::fs::write(&out, b"old").unwrap();
    app.export_tour(track, &out, 0).unwrap();
    app.export_tour(track, &out, 0).unwrap();
    assert_eq!(names(&zip_of(&out)).len(), 6);
    assert!(!part_of(&out).exists());
}

#[test]
fn unknown_track_is_not_found() {
    let app = open("zip-unknown");
    let out = dest(&app, "tour");
    let err = app.export_tour(Uuid::now_v7(), &out, 0).unwrap_err();
    assert!(matches!(
        err,
        core_app::AppError::Storage(core_storage::StorageError::NotFound(_))
    ));
    assert!(!out.exists());
}
