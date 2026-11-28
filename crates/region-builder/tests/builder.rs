#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use region_builder::{BuildError, Builder, RegionDefinition, StepProgress};

const INDEX: &str = include_str!("fixtures/index.json");

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("region-builder-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn fake_script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-build.sh")
}

/// A builder with the fake script, an own data directory and the fixture index.
fn builder(name: &str) -> (Builder, PathBuf) {
    let dir = temp_dir(name);
    let builder = Builder::with_paths(&dir.join("app"), &dir.join("repo"), &fake_script());
    builder.use_index(INDEX).unwrap();
    (builder, dir)
}

/// Writes a definition for region `id` the way `define` would.
fn defined(dir: &Path, id: &str) {
    let definition = RegionDefinition::new(id, id, [10.5, 46.5, 11.5, 47.5], vec![]);
    let defs = dir.join("app/region-defs");
    std::fs::create_dir_all(&defs).unwrap();
    std::fs::write(
        defs.join(format!("{id}.json")),
        serde_json::to_string(&definition).unwrap(),
    )
    .unwrap();
}

fn build(builder: &Builder, id: &str) -> (Result<PathBuf, BuildError>, Vec<StepProgress>) {
    let mut steps = Vec::new();
    let result = builder.build(
        id,
        false,
        &mut |step| steps.push(step),
        &AtomicBool::new(false),
    );
    (result, steps)
}

#[test]
fn steps_are_reported_and_the_package_is_returned() {
    let (builder, dir) = builder("ok");
    defined(&dir, "mini");

    let (result, steps) = build(&builder, "mini");

    let package = result.unwrap();
    assert_eq!(package, dir.join("app/region-packages/mini"));
    assert!(package.join("manifest.json").is_file());
    let seen: Vec<(&str, u32, u32)> = steps
        .iter()
        .map(|s| (s.step.as_str(), s.index, s.total))
        .collect();
    assert_eq!(seen, [("osm", 1, 2), ("manifest", 2, 2)]);
    assert!(steps.iter().all(|s| s.region == "mini"));
    let log = std::fs::read_to_string(dir.join("app/region-build/mini/build.log")).unwrap();
    assert!(log.contains("== step osm (1/2)"), "{log}");
    assert!(log.contains("== step manifest (2/2)"), "{log}");
    // The script is told where the definition is and where the package goes.
    let config = dir.join("app/region-defs/mini.json");
    assert!(
        log.contains(&format!("--config {}", config.display())),
        "{log}"
    );
    assert!(log.contains("--package-dir"), "{log}");
    assert!(!log.contains("--refresh-osm"), "{log}");
}

#[test]
fn rebuilding_asks_for_fresh_data_and_chooses_the_extracts_again() {
    let (builder, dir) = builder("refresh");
    defined(&dir, "mini");

    builder
        .build("mini", true, &mut |_| {}, &AtomicBool::new(false))
        .unwrap();

    let log = std::fs::read_to_string(dir.join("app/region-build/mini/build.log")).unwrap();
    assert!(log.contains("--force --refresh-osm"), "{log}");
    let definition: RegionDefinition = serde_json::from_str(
        &std::fs::read_to_string(dir.join("app/region-defs/mini.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(definition.osm, ["https://example.org/a-latest.osm.pbf"]);
}

/// Standard output and standard error reach the log and the tail in the order
/// the script wrote them, as on a terminal.
#[test]
fn output_keeps_the_order_of_both_streams() {
    let (builder, dir) = builder("interleaved");
    defined(&dir, "interleaved");

    let (result, _) = build(&builder, "interleaved");

    let Err(BuildError::Failed { tail, log }) = result else {
        panic!("{result:?}");
    };
    let expected: Vec<String> = (1..=200)
        .map(|i| format!("{} {i}", if i % 2 == 0 { "out" } else { "err" }))
        .collect();
    let written = std::fs::read_to_string(log).unwrap();
    assert_eq!(written.lines().collect::<Vec<_>>(), expected);
    assert_eq!(tail, expected[185..].join("\n"));
}

#[test]
fn failure_carries_the_tail_of_the_output() {
    let (builder, dir) = builder("fail");
    defined(&dir, "fail");

    let (result, steps) = build(&builder, "fail");

    let Err(BuildError::Failed { tail, log }) = result else {
        panic!("{result:?}");
    };
    let lines: Vec<&str> = tail.lines().collect();
    assert_eq!(lines.len(), 15, "{tail}");
    assert!(tail.contains("error: something broke"), "{tail}");
    assert!(tail.contains("line 20"), "{tail}");
    assert!(!tail.contains("line 3\n"), "{tail}");
    assert!(log.is_file());
    assert_eq!(steps.len(), 1);
}

#[test]
fn cancel_stops_the_whole_process_group() {
    let (builder, dir) = builder("hang");
    defined(&dir, "hang");
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        flag.store(true, Ordering::SeqCst);
    });

    let started = Instant::now();
    let result = builder.build("hang", false, &mut |_| {}, &cancel);

    assert!(matches!(result, Err(BuildError::Cancelled)), "{result:?}");
    assert!(started.elapsed() < Duration::from_secs(5));
    let survivors = std::process::Command::new("pgrep")
        .args(["-f", "sleep 611"])
        .output()
        .unwrap();
    assert!(
        survivors.stdout.is_empty(),
        "{}",
        String::from_utf8_lossy(&survivors.stdout)
    );
}

#[test]
fn output_need_not_be_text() {
    let (builder, dir) = builder("binary");
    defined(&dir, "binary");
    let (result, _) = build(&builder, "binary");
    assert!(result.is_ok(), "{result:?}");
    let log = std::fs::read(dir.join("app/region-build/binary/build.log")).unwrap();
    assert!(String::from_utf8_lossy(&log).contains("no newline at the end"));
}

#[test]
fn unknown_region_is_reported() {
    let (builder, _) = builder("unknown");
    let (result, _) = build(&builder, "nope");
    assert!(
        matches!(result, Err(BuildError::UnknownRegion(_))),
        "{result:?}"
    );
    let (result, _) = build(&builder, "../etc");
    assert!(
        matches!(result, Err(BuildError::UnknownRegion(_))),
        "{result:?}"
    );
}

#[test]
fn define_writes_a_unique_definition() {
    let (builder, dir) = builder("define");
    let bbox = [10.5, 46.5, 11.5, 47.5];

    let first = builder
        .define(" Dolomiten West ", bbox, &|_| false)
        .unwrap();
    let second = builder.define("Dolomiten West", bbox, &|_| false).unwrap();
    let third = builder
        .define("Dolomiten West", bbox, &|id| id == "dolomiten-west-3")
        .unwrap();

    assert_eq!(first.id, "dolomiten-west");
    assert_eq!(first.name, "Dolomiten West");
    assert_eq!(first.osm, ["https://example.org/a-latest.osm.pbf"]);
    assert_eq!(second.id, "dolomiten-west-2");
    assert_eq!(third.id, "dolomiten-west-4");
    let stored: RegionDefinition = serde_json::from_str(
        &std::fs::read_to_string(dir.join("app/region-defs/dolomiten-west-2.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(stored, second);
}

#[test]
fn define_avoids_template_suffixes() {
    let (builder, _) = builder("define-suffix");
    let bbox = [10.5, 46.5, 11.5, 47.5];
    let urlaub = builder.define("Kroatien Urlaub", bbox, &|_| false).unwrap();
    let fliegen = builder
        .define("Kroatien Fliegen", bbox, &|_| false)
        .unwrap();
    assert_eq!(urlaub.id, "kroatien-urlaub-region");
    assert_eq!(fliegen.id, "kroatien-fliegen-region");
}

#[test]
fn define_rejects_areas_that_cannot_be_built() {
    let (builder, dir) = builder("define-bad");
    let backwards = builder.define("x", [11.5, 46.5, 10.5, 47.5], &|_| false);
    assert!(
        matches!(backwards, Err(BuildError::InvalidArea(_))),
        "{backwards:?}"
    );
    let nowhere = builder.define("x", [30.0, 46.0, 31.0, 47.0], &|_| false);
    assert!(matches!(nowhere, Err(BuildError::NoExtract)), "{nowhere:?}");
    assert!(!dir.join("app/region-defs/x.json").exists());
}

#[test]
fn plan_tells_size_and_extracts() {
    let (builder, _) = builder("plan");
    let plan = builder.plan([11.5, 46.5, 12.5, 47.5]).unwrap();
    assert!((75.0..77.0).contains(&plan.width_km), "{}", plan.width_km);
    assert!(
        (110.0..112.0).contains(&plan.height_km),
        "{}",
        plan.height_km
    );
    assert!(
        (300e6..360e6).contains(&(plan.estimated_bytes as f64)),
        "{}",
        plan.estimated_bytes
    );
    assert_eq!(plan.extracts.len(), 2);
}

#[test]
fn cleanup_removes_everything_of_a_region() {
    let (builder, dir) = builder("cleanup");
    defined(&dir, "mini");
    build(&builder, "mini").0.unwrap();
    assert!(dir.join("repo/data/build/mini").is_dir());

    builder.cleanup("mini", false);
    assert!(!dir.join("app/region-packages/mini").exists());
    assert!(!dir.join("app/region-build/mini").exists());
    assert!(!dir.join("repo/data/build/mini").exists());
    assert!(dir.join("app/region-defs/mini.json").is_file());

    builder.cleanup("mini", true);
    assert!(!dir.join("app/region-defs/mini.json").exists());
    // Nothing there any more, and ids that are no plain names are ignored.
    builder.cleanup("mini", true);
    builder.cleanup("..", true);
    assert!(dir.join("app").is_dir());
}

#[test]
fn cleanup_keeps_a_build_directory_it_did_not_create() {
    let (builder, dir) = builder("foreign-cache");
    defined(&dir, "mini");
    let cache = dir.join("repo/data/build/mini");
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(cache.join("region.osm.pbf"), "expensive").unwrap();

    // Neither a build through the app nor deleting the region touches it.
    build(&builder, "mini").0.unwrap();
    builder.cleanup("mini", false);
    assert!(cache.join("region.osm.pbf").is_file());
    builder.cleanup("mini", true);
    assert!(cache.join("region.osm.pbf").is_file());
    assert!(!dir.join("app/region-packages/mini").exists());
    assert!(!dir.join("app/region-defs/mini.json").exists());
}

#[test]
fn cleanup_removes_what_a_stopped_build_left_behind() {
    let (builder, dir) = builder("own-cache");
    defined(&dir, "fail");

    assert!(build(&builder, "fail").0.is_err());
    let cache = dir.join("repo/data/build/fail");
    assert!(cache.is_dir());

    // Building again after a failure still counts as the app's own directory.
    assert!(build(&builder, "fail").0.is_err());
    builder.cleanup("fail", true);
    assert!(!cache.exists());
}

#[test]
fn a_new_region_avoids_the_id_of_an_existing_build_directory() {
    let (builder, dir) = builder("taken-cache");
    std::fs::create_dir_all(dir.join("repo/data/build/mini")).unwrap();

    let definition = builder
        .define("Mini", [10.5, 46.5, 11.5, 47.5], &|_| false)
        .unwrap();

    assert_ne!(definition.id, "mini");
}

#[test]
fn check_reports_a_missing_script() {
    let dir = temp_dir("check");
    let missing = Builder::with_paths(&dir, &dir, &dir.join("no-such-script.sh")).check();
    assert!(missing.contains(&"repo".to_string()), "{missing:?}");
    let (builder, _) = builder("check-ok");
    assert!(!builder.check().contains(&"repo".to_string()));
}

#[test]
fn a_build_that_cannot_start_is_reported() {
    let dir = temp_dir("nostart");
    let builder = Builder::with_paths(&dir.join("app"), &dir, &dir.join("no-such-script.sh"));
    defined(&dir, "mini");
    let (result, _) = build(&builder, "mini");
    assert!(
        matches!(result, Err(BuildError::Prerequisites(_))),
        "{result:?}"
    );
}
