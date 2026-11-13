#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::{Command, Output};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/gpx")
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_track-cli"))
        .args(args)
        .output()
        .unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8(o.stdout.clone()).unwrap()
}

fn stderr(o: &Output) -> String {
    String::from_utf8(o.stderr.clone()).unwrap()
}

#[test]
fn prints_human_readable_statistics_for_a_gpx_file() {
    let file = fixtures().join("synthetic_two_segments.gpx");
    let o = run(&["stats", file.to_str().unwrap()]);
    assert!(o.status.success(), "{}", stderr(&o));
    let out = stdout(&o);
    assert!(out.contains("Kranzberg & Ferchensee"), "{out}");
    assert!(out.contains("Activity:      hike"), "{out}");
    assert!(out.contains("Distance:      0.03 km"), "{out}");
    assert!(out.contains("Total time:    0:00:30"), "{out}");
    assert!(out.contains("Points:        5 of 5 used"), "{out}");
}

#[test]
fn prints_json_statistics_on_request() {
    let file = fixtures().join("synthetic_two_segments.gpx");
    let o = run(&[
        "stats",
        file.to_str().unwrap(),
        "--json",
        "--activity",
        "mtb",
    ]);
    assert!(o.status.success(), "{}", stderr(&o));
    let v: serde_json::Value = serde_json::from_str(&stdout(&o)).unwrap();
    assert_eq!(v[0]["activity"], "mtb");
    assert_eq!(v[0]["name"], "Kranzberg & Ferchensee");
    let d = v[0]["stats"]["distance_m"].as_f64().unwrap();
    assert!((d - 33.36).abs() < 0.05, "{d}");
    assert_eq!(v[0]["stats"]["total_time_s"], 30.0);
}

#[test]
fn overrides_individual_statistics_parameters() {
    let file = fixtures().join("synthetic_two_segments.gpx");
    let o = run(&[
        "stats",
        file.to_str().unwrap(),
        "--json",
        "--set",
        "min_move_m=1000",
    ]);
    assert!(o.status.success(), "{}", stderr(&o));
    let v: serde_json::Value = serde_json::from_str(&stdout(&o)).unwrap();
    assert_eq!(v[0]["stats"]["distance_m"], 0.0);
}

#[test]
fn rejects_unknown_or_malformed_parameter_overrides() {
    let file = fixtures().join("synthetic_two_segments.gpx");
    for bad in ["no_such_param=1", "min_move_m=abc", "min_move_m"] {
        let o = run(&["stats", file.to_str().unwrap(), "--set", bad]);
        assert_eq!(o.status.code(), Some(1), "{bad}");
        assert!(
            stderr(&o).contains(bad.split('=').next().unwrap()),
            "{bad}: {}",
            stderr(&o)
        );
    }
}

#[test]
fn asks_for_the_activity_when_the_file_does_not_name_one() {
    let dir = std::env::temp_dir().join(format!("track-cli-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("untyped.gpx");
    std::fs::write(
        &file,
        r#"<gpx><trk><trkseg><trkpt lat="1" lon="2"/></trkseg></trk></gpx>"#,
    )
    .unwrap();
    let o = run(&["stats", file.to_str().unwrap()]);
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).contains("--activity"), "{}", stderr(&o));
}

#[test]
fn reports_unreadable_and_invalid_files_on_stderr() {
    let o = run(&["stats", "/nonexistent/file.gpx", "--activity", "bike"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        stderr(&o).contains("/nonexistent/file.gpx"),
        "{}",
        stderr(&o)
    );

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let o = run(&["stats", manifest.to_str().unwrap(), "--activity", "bike"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).contains("not a GPX document"), "{}", stderr(&o));
}

/// Every real recording in `fixtures/gpx/real` must parse and give plausible numbers.
#[test]
fn real_fixture_tracks_yield_plausible_statistics() {
    let Ok(entries) = std::fs::read_dir(fixtures().join("real")) else {
        return;
    };
    for path in entries
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "gpx"))
    {
        // Real fixtures must carry their activity in `<type>`.
        let o = run(&["stats", path.to_str().unwrap(), "--json"]);
        assert!(o.status.success(), "{}: {}", path.display(), stderr(&o));
        let v: serde_json::Value = serde_json::from_str(&stdout(&o)).unwrap();
        for t in v.as_array().unwrap() {
            let s = &t["stats"];
            let (dist, total, moving) = (
                s["distance_m"].as_f64().unwrap(),
                s["total_time_s"].as_f64().unwrap(),
                s["moving_time_s"].as_f64().unwrap(),
            );
            assert!(dist > 100.0, "{}: distance {dist}", path.display());
            assert!(
                moving <= total,
                "{}: moving {moving} > total {total}",
                path.display()
            );
            // Where another app's figures are known, stay close to them.
            let references: serde_json::Value = serde_json::from_str(
                &std::fs::read_to_string(fixtures().join("real/reference.json")).unwrap(),
            )
            .unwrap();
            let name = path.file_name().unwrap().to_str().unwrap();
            if let Some(reference) = references.get(name) {
                let within = |what: &str, actual: f64, expected: f64, tolerance: f64| {
                    assert!(
                        (actual / expected - 1.0).abs() <= tolerance,
                        "{name}: {what} is {actual:.1}, reference {expected:.1} (±{:.0} %)",
                        tolerance * 100.0
                    );
                };
                within(
                    "distance",
                    dist / 1000.0,
                    reference["distance_km"].as_f64().unwrap(),
                    0.01,
                );
                within(
                    "moving time",
                    moving,
                    reference["moving_time_s"].as_f64().unwrap(),
                    0.10,
                );
                within(
                    "ascent",
                    s["ascent_m"].as_f64().unwrap(),
                    reference["ascent_m"].as_f64().unwrap(),
                    0.20,
                );
                within(
                    "descent",
                    s["descent_m"].as_f64().unwrap(),
                    reference["descent_m"].as_f64().unwrap(),
                    0.20,
                );
            }
            let used = s["points_used"].as_f64().unwrap() / s["points_total"].as_f64().unwrap();
            assert!(
                used > 0.9,
                "{}: only {used:.2} of points used",
                path.display()
            );
        }
    }
}

/// A listener that ignores everything.
struct Quiet;

impl core_app::AppListener for Quiet {
    fn on_position(&self, _: &core_location::Fix) {}
    fn on_recording(&self, _: &core_app::RecordingStatus) {}
    fn on_source_status(&self, _: &core_location::SourceStatus) {}
    fn on_error(&self, _: &str) {}
}

fn temp_db(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("track-cli-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("app.sqlite")
}

#[test]
fn records_a_gpx_file_into_a_database() {
    let db = temp_db("record");
    let file = fixtures().join("real/tour_sylvensteinsee.gpx");
    let o = run(&[
        "record",
        "--db",
        db.to_str().unwrap(),
        file.to_str().unwrap(),
    ]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(
        stdout(&o).contains("Distance:      27.60 km"),
        "{}",
        stdout(&o)
    );

    let app = core_app::App::open(&db, std::sync::Arc::new(Quiet)).unwrap();
    let tracks = app.list_tracks().unwrap();
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].stats.as_ref().unwrap().points_total, 1542);
}

/// Crash safety: the process dies without any cleanup in the middle of a
/// recording. Every fix handed over before that must be in the database.
#[test]
fn recording_survives_the_process_being_killed() {
    let db = temp_db("abort");
    let file = fixtures().join("real/tour_sylvensteinsee.gpx");
    let o = run(&[
        "record",
        "--db",
        db.to_str().unwrap(),
        file.to_str().unwrap(),
        "--abort-after",
        "300",
    ]);
    assert!(!o.status.success(), "the process must have died");
    assert_eq!(
        o.status.code(),
        None,
        "killed by a signal, not a regular exit"
    );

    let app = core_app::App::open(&db, std::sync::Arc::new(Quiet)).unwrap();
    assert!(
        app.list_tracks().unwrap()[0].stats.is_none(),
        "track was never finished"
    );
    let interrupted = app
        .interrupted_recording()
        .unwrap()
        .expect("unfinished recording is found");
    let finished = app.finish_interrupted().unwrap().unwrap();
    assert_eq!(finished.id, interrupted.id);
    assert_eq!(finished.stats.unwrap().points_total, 300);
}
