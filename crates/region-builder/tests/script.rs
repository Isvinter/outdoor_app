#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Runs the real `scripts/build-region.sh` with a temporary definition. None of
//! these tests builds anything: they only look at plans, checks and manifests.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/build-region.sh")
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("region-script-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Writes a definition (`template` is the raw JSON fragment, or empty).
fn definition(dir: &Path, id: &str, template: &str) -> PathBuf {
    let path = dir.join(format!("{id}.json"));
    let template = if template.is_empty() {
        String::new()
    } else {
        format!(r#","template":{template}"#)
    };
    std::fs::write(
        &path,
        format!(
            r#"{{"id":"{id}","name":"Alps","bbox":[10.0,46.0,11.0,47.0],"osm":["https://example.org/a.osm.pbf"],
            "basemap_maxzoom":14,"dem_maxzoom":13,"attribution":["x"]{template}}}"#
        ),
    )
    .unwrap();
    path
}

/// Runs the script on `id` with the given definition, a package directory
/// that does not exist yet (`dir/packages`) and a data directory of its own.
fn run(dir: &Path, id: &str, config: &Path, extra: &[&str]) -> Output {
    Command::new(script())
        .arg(id)
        .arg("--config")
        .arg(config)
        .arg("--package-dir")
        .arg(dir.join("packages"))
        .args(extra)
        .env("OUTDOOR_DATA", dir.join("data"))
        .output()
        .unwrap()
}

fn plan(dir: &Path, id: &str, template: &str) -> String {
    let config = definition(dir, id, template);
    let output = run(dir, id, &config, &["--print-plan"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Nothing may be created just for a plan.
    assert!(!dir.join("packages").exists());
    assert!(!dir.join("data").exists());
    String::from_utf8(output.stdout).unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn urlaub_plan() {
    let dir = temp_dir("urlaub-plan");
    assert_eq!(
        plan(&dir, "alps-urlaub", r#""urlaub""#),
        "steps=osm,basemap,search,routing,manifest\n\
         files=basemap.pmtiles,search.sqlite,routing.tar\n\
         group=alps\n"
    );
}

#[test]
fn outdoor_plan() {
    let dir = temp_dir("outdoor-plan");
    assert_eq!(
        plan(&dir, "alps", r#""outdoor""#),
        "steps=osm,basemap,outdoor,dem,slope,contours,search,routing,manifest\n\
         files=basemap.pmtiles,outdoor.pmtiles,dem.pmtiles,slope.pmtiles,contours.pmtiles,search.sqlite,routing.tar\n\
         group=alps\n"
    );
}

#[test]
fn fliegen_plan() {
    let dir = temp_dir("fliegen-plan");
    let text = plan(&dir, "alps-fliegen", r#""fliegen""#);
    assert_eq!(
        text,
        "steps=osm,basemap,outdoor,dem,contours,search,manifest\n\
         files=basemap.pmtiles,outdoor.pmtiles,dem.pmtiles,contours.pmtiles,search.sqlite\n\
         group=alps\n"
    );
    assert!(!text.contains("routing.tar"));
}

#[test]
fn old_definition_is_outdoor() {
    let dir = temp_dir("old-definition");
    // Without a template the id is not checked, even with a template ending.
    let text = plan(&dir, "alps-urlaub", "");
    assert!(
        text.starts_with("steps=osm,basemap,outdoor,dem,slope,contours,search,routing,manifest\n")
    );
    assert!(text.ends_with("group=alps-urlaub\n"), "{text}");
}

#[test]
fn explicit_steps_win_over_the_template() {
    let dir = temp_dir("explicit-steps");
    let config = definition(&dir, "alps-urlaub", r#""urlaub""#);
    let output = run(
        &dir,
        "alps-urlaub",
        &config,
        &["--print-plan", "--steps", "manifest"],
    );
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.starts_with("steps=manifest\n"), "{text}");
}

#[test]
fn id_and_template_must_match() {
    for (id, template) in [
        ("alps", r#""urlaub""#),
        ("alps", r#""fliegen""#),
        ("alps-urlaub", r#""fliegen""#),
        ("alps-fliegen", r#""outdoor""#),
        ("alps-urlaub", r#""outdoor""#),
    ] {
        let dir = temp_dir("mismatch");
        let config = definition(&dir, id, template);
        let output = run(&dir, id, &config, &[]);
        assert!(!output.status.success(), "{id} {template}");
        assert!(stderr(&output).contains("template"), "{}", stderr(&output));
        assert!(!dir.join("packages").exists(), "{id} {template}");
        assert!(!dir.join("data").exists(), "{id} {template}");
    }
}

#[test]
fn unknown_template_stops_the_build() {
    let dir = temp_dir("unknown-template");
    let config = definition(&dir, "alps", r#""strand""#);
    let output = run(&dir, "alps", &config, &[]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("strand"), "{}", stderr(&output));
    assert!(!dir.join("packages").exists());
}

#[test]
fn other_template_in_the_directory_stops_the_build() {
    let dir = temp_dir("other-template");
    let out = dir.join("packages/alps");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("manifest.json"), r#"{"template":"urlaub"}"#).unwrap();
    let config = definition(&dir, "alps", r#""outdoor""#);
    let output = run(&dir, "alps", &config, &[]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("urlaub"), "{}", stderr(&output));
    assert!(!dir.join("data").exists());
}

#[test]
fn manifest_lists_only_the_files_of_the_template() {
    let dir = temp_dir("manifest");
    let out = dir.join("packages/alps-urlaub");
    std::fs::create_dir_all(&out).unwrap();
    for name in [
        "basemap.pmtiles",
        "search.sqlite",
        "routing.tar",
        "dem.pmtiles",
    ] {
        std::fs::write(out.join(name), name).unwrap();
    }
    std::fs::write(out.join("routing.json"), r#"{"engine":"valhalla"}"#).unwrap();
    std::fs::write(
        out.join("search.json"),
        r#"{"format":1,"places":3,"streets":4,"pois":5,"named_pois":2}"#,
    )
    .unwrap();
    let config = definition(&dir, "alps-urlaub", r#""urlaub""#);
    let output = run(&dir, "alps-urlaub", &config, &["--steps", "manifest"]);
    assert!(output.status.success(), "{}", stderr(&output));

    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out.join("manifest.json")).unwrap()).unwrap();
    let names: Vec<&str> = manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["basemap.pmtiles", "search.sqlite", "routing.tar"]);
    assert_eq!(manifest["template"], "urlaub");
    assert_eq!(manifest["group"], "alps");
    assert_eq!(manifest["search"]["pois"], 5);
    assert_eq!(manifest["routing"]["engine"], "valhalla");

    let catalog: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("packages/catalog.json")).unwrap())
            .unwrap();
    assert_eq!(catalog["regions"][0]["template"], "urlaub");
    assert_eq!(catalog["regions"][0]["group"], "alps");
}

#[test]
fn manifest_without_a_search_file_has_no_search_entry() {
    let dir = temp_dir("manifest-no-search");
    let out = dir.join("packages/alps");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("basemap.pmtiles"), "x").unwrap();
    let config = definition(&dir, "alps", "");
    let output = run(&dir, "alps", &config, &["--steps", "manifest"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out.join("manifest.json")).unwrap()).unwrap();
    assert!(manifest.get("search").is_none());
    assert_eq!(manifest["template"], "outdoor");
}
