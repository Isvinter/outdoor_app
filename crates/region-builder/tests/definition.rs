#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use region_builder::{
    BuildError, RegionDefinition, Template, estimated_bytes, size_km, slug, unique_id,
    validate_bbox,
};

#[test]
fn names_become_plain_ids() {
    assert_eq!(slug("Dolomiten West"), "dolomiten-west");
    assert_eq!(slug("Zillertal – Ötztal (groß)"), "zillertal-oetztal-gross");
    assert_eq!(slug("ÄÖÜ"), "aeoeue");
    assert_eq!(slug("Val d'Isère 2"), "val-d-is-re-2");
}

#[test]
fn names_without_usable_characters_get_a_default_id() {
    assert_eq!(slug("🏔️"), "region");
    assert_eq!(slug("  "), "region");
    assert_eq!(slug("---"), "region");
}

#[test]
fn long_names_are_cut() {
    assert_eq!(slug(&"a".repeat(100)).len(), 40);
    // Cutting must not leave a dash at the end.
    let cut = slug(&format!("{} b", "a".repeat(39)));
    assert_eq!(cut, "a".repeat(39));
}

#[test]
fn taken_ids_get_a_counter() {
    assert_eq!(unique_id("x", &|_| false), "x");
    assert_eq!(unique_id("x", &|id| id == "x" || id == "x-2"), "x-3");
}

#[test]
fn bounding_boxes_are_validated() {
    assert!(validate_bbox([11.0, 46.0, 12.0, 47.0]).is_ok());
    for bad in [
        [12.0, 46.0, 11.0, 47.0],
        [11.0, 47.0, 12.0, 46.0],
        [11.0, 46.0, 11.001, 47.0],
        [11.0, 46.0, 12.0, 46.001],
        [11.0, 86.0, 12.0, 87.0],
        [179.0, 46.0, 181.0, 47.0],
        [f64::NAN, 46.0, 12.0, 47.0],
    ] {
        let result = validate_bbox(bad);
        assert!(matches!(result, Err(BuildError::InvalidArea(_))), "{bad:?}");
    }
}

#[test]
fn size_is_measured_in_kilometres() {
    let (width, height) = size_km([11.35, 47.35, 11.95, 47.80]);
    assert!((44.0..46.5).contains(&width), "{width}");
    assert!((49.5..50.5).contains(&height), "{height}");
}

#[test]
fn estimate_matches_the_measured_test_region() {
    // isarwinkel-achensee: 90 MB.
    let bytes = estimated_bytes([11.35, 47.35, 11.95, 47.80]) as f64;
    assert!((80e6..100e6).contains(&bytes), "{bytes}");
}

#[test]
fn definition_is_written_in_the_format_of_the_build_script() {
    let definition = RegionDefinition::new(
        "dolomiten",
        "Dolomiten",
        [11.0, 46.0, 12.0, 47.0],
        vec!["https://example.org/a.osm.pbf".to_string()],
    );
    let json: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&definition).unwrap()).unwrap();
    assert_eq!(json["id"], "dolomiten");
    assert_eq!(json["name"], "Dolomiten");
    assert_eq!(json["bbox"], serde_json::json!([11.0, 46.0, 12.0, 47.0]));
    assert_eq!(json["osm"][0], "https://example.org/a.osm.pbf");
    assert_eq!(json["basemap_maxzoom"], 14);
    assert_eq!(json["dem_maxzoom"], 13);
    assert_eq!(json["attribution"].as_array().unwrap().len(), 3);
    let back: RegionDefinition = serde_json::from_value(json).unwrap();
    assert_eq!(back, definition);
}

fn sample_json(extra: &str) -> String {
    format!(
        r#"{{"id":"x","name":"X","bbox":[1.0,2.0,3.0,4.0],"osm":[],"basemap_maxzoom":14,
        "dem_maxzoom":13,"attribution":[]{extra}}}"#
    )
}

#[test]
fn template_defaults_to_outdoor_and_is_not_written() {
    let definition: RegionDefinition = serde_json::from_str(&sample_json("")).unwrap();
    assert_eq!(definition.template, Template::Outdoor);
    let written = serde_json::to_string(&definition).unwrap();
    assert!(!written.contains("template"), "{written}");
}

#[test]
fn urlaub_round_trips() {
    let definition: RegionDefinition =
        serde_json::from_str(&sample_json(r#","template":"urlaub""#)).unwrap();
    assert_eq!(definition.template, Template::Urlaub);
    let written = serde_json::to_string(&definition).unwrap();
    assert!(written.contains(r#""template":"urlaub""#), "{written}");
    assert_eq!(
        serde_json::from_str::<RegionDefinition>(&written).unwrap(),
        definition
    );
}

#[test]
fn unknown_template_is_rejected() {
    let parsed = serde_json::from_str::<RegionDefinition>(&sample_json(r#","template":"strand""#));
    assert!(parsed.is_err());
}
