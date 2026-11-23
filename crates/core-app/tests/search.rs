//! Searching all installed regions.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::Path;

use core_app::App;
use core_routing::LatLon;
use core_search::{Category, Entry, Kind, SearchWriter};

const NEAR: LatLon = LatLon {
    lat: -13.5,
    lon: -72.0,
};

fn entry(kind: Kind, category: Option<Category>, name: &str, lat: f64, lon: f64) -> Entry {
    Entry {
        kind,
        category,
        subtype: None,
        name: name.to_string(),
        alt_names: Vec::new(),
        town: None,
        town_names: Vec::new(),
        lat,
        lon,
    }
}

fn lodging(name: &str, lat: f64, lon: f64) -> Entry {
    entry(Kind::Poi, Some(Category::Lodging), name, lat, lon)
}

/// A region directory; `entries` of `None` means it has no search file.
fn region(dir: &Path, id: &str, name: &str, entries: Option<&[Entry]>) {
    let region = dir.join("regions").join(id);
    std::fs::create_dir_all(&region).unwrap();
    let file = if let Some(entries) = entries {
        let mut writer =
            SearchWriter::create(&region.join("search.sqlite"), Some("2026-11-01T00:00:00Z"))
                .unwrap();
        for e in entries {
            writer.add(e).unwrap();
        }
        writer.finish().unwrap();
        "search.sqlite"
    } else {
        std::fs::write(region.join("routing.tar"), b"dummy").unwrap();
        "routing.tar"
    };
    let size = std::fs::metadata(region.join(file)).unwrap().len();
    let manifest = serde_json::json!({
        "format": 1, "id": id, "name": name, "bbox": [-73.0, -14.0, -71.0, -13.0],
        "version": "1", "attribution": [],
        "files": [{ "name": file, "size": size, "sha256": "0".repeat(64) }], "size": size,
    });
    std::fs::write(region.join("manifest.json"), manifest.to_string()).unwrap();
}

fn names(app: &App, text: &str) -> Vec<String> {
    app.search(text, NEAR, 30)
        .hits
        .into_iter()
        .map(|h| h.name)
        .collect()
}

#[test]
fn finds_in_every_enabled_region() {
    let dir = common::temp_dir("search-every");
    region(
        &dir,
        "a",
        "A",
        Some(&[lodging("Hostel Eins", -13.5, -72.0)]),
    );
    region(
        &dir,
        "b",
        "B",
        Some(&[lodging("Hostel Zwei", -13.51, -72.0)]),
    );
    let app = common::open(&dir);
    assert_eq!(names(&app, "Hostel E"), ["Hostel Eins"]);
    assert_eq!(names(&app, "Hostel Z"), ["Hostel Zwei"]);
    assert_eq!(names(&app, "Hostel"), ["Hostel Eins", "Hostel Zwei"]);
}

#[test]
fn disabled_region_is_not_searched() {
    let dir = common::temp_dir("search-disabled");
    region(
        &dir,
        "a",
        "A",
        Some(&[lodging("Hostel Eins", -13.5, -72.0)]),
    );
    let app = common::open(&dir);
    app.set_region_enabled("a", false).unwrap();
    assert!(names(&app, "Hostel").is_empty());
    assert!(app.search("Hostel", NEAR, 30).unsearchable.is_empty());
}

#[test]
fn duplicate_within_30_m_appears_once() {
    let dir = common::temp_dir("search-duplicate");
    region(
        &dir,
        "a",
        "A",
        Some(&[lodging("Hostel Eins", -13.5, -72.0)]),
    );
    region(
        &dir,
        "b",
        "B",
        Some(&[lodging("Hostel Eins", -13.5001, -72.0)]),
    );
    assert_eq!(names(&common::open(&dir), "Eins"), ["Hostel Eins"]);
}

#[test]
fn same_name_50_m_apart_stays_twice() {
    let dir = common::temp_dir("search-apart");
    region(
        &dir,
        "a",
        "A",
        Some(&[lodging("Hostel Eins", -13.5, -72.0)]),
    );
    region(
        &dir,
        "b",
        "B",
        Some(&[lodging("Hostel Eins", -13.50045, -72.0)]),
    );
    assert_eq!(names(&common::open(&dir), "Eins").len(), 2);
}

#[test]
fn region_without_index_is_named() {
    let dir = common::temp_dir("search-no-index");
    region(
        &dir,
        "a",
        "A",
        Some(&[lodging("Hostel Eins", -13.5, -72.0)]),
    );
    region(&dir, "alt", "Alt", None);
    let results = common::open(&dir).search("Hostel", NEAR, 30);
    assert_eq!(results.unsearchable, ["Alt"]);
    assert_eq!(results.hits.len(), 1);
}

#[test]
fn damaged_index_is_named_and_others_still_hit() {
    let dir = common::temp_dir("search-damaged");
    region(
        &dir,
        "a",
        "A",
        Some(&[lodging("Hostel Eins", -13.5, -72.0)]),
    );
    region(
        &dir,
        "kaputt",
        "Kaputt",
        Some(&[lodging("Hostel Zwei", -13.5, -72.0)]),
    );
    let file = dir.join("regions/kaputt/search.sqlite");
    let size = std::fs::metadata(&file).unwrap().len();
    std::fs::write(&file, vec![b'x'; usize::try_from(size).unwrap()]).unwrap();
    let results = common::open(&dir).search("Hostel", NEAR, 30);
    assert_eq!(results.unsearchable, ["Kaputt"]);
    assert_eq!(results.hits.len(), 1);
}

#[test]
fn category_word_adds_unnamed_kind() {
    let dir = common::temp_dir("search-category-word");
    region(&dir, "a", "A", Some(&[lodging("Pariwana", -13.5, -72.0)]));
    assert_eq!(names(&common::open(&dir), "Hostel"), ["Pariwana"]);
}

#[test]
fn category_word_does_not_duplicate_name_hit() {
    let dir = common::temp_dir("search-category-once");
    region(
        &dir,
        "a",
        "A",
        Some(&[lodging("Hostel Alpina", -13.5, -72.0)]),
    );
    assert_eq!(names(&common::open(&dir), "Hostel"), ["Hostel Alpina"]);
}

#[test]
fn two_words_stay_name_search() {
    let dir = common::temp_dir("search-two-words");
    region(&dir, "a", "A", Some(&[lodging("Pariwana", -13.5, -72.0)]));
    assert!(names(&common::open(&dir), "Hostel Cusco").is_empty());
}

#[test]
fn too_short_input_is_empty() {
    let dir = common::temp_dir("search-short");
    region(&dir, "a", "A", Some(&[lodging("A", -13.5, -72.0)]));
    let app = common::open(&dir);
    assert!(names(&app, "a").is_empty());
    assert!(names(&app, " - ").is_empty());
}

#[test]
fn limit_is_clamped() {
    let dir = common::temp_dir("search-limit");
    let entries: Vec<Entry> = (0..150)
        .map(|i| lodging("Hostel", -13.5, -72.0 + f64::from(i) * 0.01))
        .collect();
    region(&dir, "a", "A", Some(&entries));
    let app = common::open(&dir);
    assert_eq!(app.search("Hostel", NEAR, usize::MAX).hits.len(), 100);
    assert_eq!(app.search("Hostel", NEAR, 0).hits.len(), 1);
    let by_category = app.search_category("lodging", NEAR, usize::MAX).unwrap();
    assert_eq!(by_category.hits.len(), 100);
}

#[test]
fn category_search_returns_nearest_with_its_key() {
    let dir = common::temp_dir("search-category");
    region(
        &dir,
        "a",
        "A",
        Some(&[lodging("Fern", -13.9, -72.0), lodging("Nah", -13.5, -72.0)]),
    );
    let results = common::open(&dir)
        .search_category("lodging", NEAR, 30)
        .unwrap();
    assert_eq!(results.hits[0].name, "Nah");
    assert_eq!(results.hits[0].kind, "poi");
    assert_eq!(results.hits[0].category.as_deref(), Some("lodging"));
}

#[test]
fn unknown_category_is_an_error() {
    let dir = common::temp_dir("search-unknown");
    region(&dir, "a", "A", Some(&[lodging("Nah", -13.5, -72.0)]));
    let error = common::open(&dir)
        .search_category("nonsense", NEAR, 30)
        .unwrap_err();
    assert_eq!(error.code(), "search_category");
}
