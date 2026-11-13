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
        let o = run(&[
            "stats",
            path.to_str().unwrap(),
            "--json",
            "--activity",
            "hike",
        ]);
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
            let used = s["points_used"].as_f64().unwrap() / s["points_total"].as_f64().unwrap();
            assert!(
                used > 0.9,
                "{}: only {used:.2} of points used",
                path.display()
            );
        }
    }
}
