//! A tour with more photos than an app may keep files open: iOS allows about
//! 256. The export runs again in a child process limited to 128 open files.
//! A file of its own, so the limit touches no other test.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use core_app::{App, AppListener, RecordingStatus};
use core_location::{Fix, SourceStatus};
use core_storage::{MediaKind, MediaSource};
use core_track::Activity;

const PHOTOS: usize = 200;

struct Quiet;

impl AppListener for Quiet {
    fn on_position(&self, _: &Fix) {}
    fn on_recording(&self, _: &RecordingStatus) {}
    fn on_source_status(&self, _: &SourceStatus) {}
    fn on_error(&self, _: &str) {}
}

#[test]
fn export_with_more_photos_than_open_files() {
    let exe = std::env::current_exe().unwrap();
    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(r#"ulimit -n 128 && exec "$0" many_photos_limited --exact --ignored --nocapture"#)
        .arg(exe)
        .status()
        .unwrap();
    assert!(status.success(), "export under 128 open files failed");
}

/// Run by the test above under the limit.
#[test]
#[ignore = "runs in a child process with few open files"]
fn many_photos_limited() {
    let dir = std::env::temp_dir().join(format!("core-app-open-files-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let app = App::open(&dir.join("app.sqlite"), Arc::new(Quiet)).unwrap();
    let track = app
        .start_recording(Activity::Hike)
        .unwrap()
        .track_id
        .unwrap();
    for i in 0..=10 {
        app.handle_fix(Fix {
            lat: 47.0 + i as f64 * 5.0 / 111_195.08,
            lon: 11.0,
            ele: Some(700.0),
            time_ms: 1_800_000_000_000 + i * 1000,
            h_accuracy_m: Some(4.0),
            speed_mps: None,
            course_deg: None,
        });
    }
    for n in 0..PHOTOS {
        let name = format!("p{n}.jpg");
        std::fs::write(app.media_dir().join(&name), b"jpeg").unwrap();
        app.add_recording_media(
            MediaKind::Photo,
            MediaSource::Camera,
            Some(&format!("media/{name}")),
            None,
        )
        .unwrap();
    }
    app.stop_recording().unwrap().unwrap();

    let out = dir.join("tour.zip");
    app.export_tour(track, &out, 60).unwrap();
    let archive = zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
    let photos = archive
        .file_names()
        .filter(|n| n.as_ref().unwrap().starts_with("fotos/"))
        .count();
    assert_eq!(photos, PHOTOS);
    let _ = std::fs::remove_dir_all(&dir);
}
