//! Searching all installed regions.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::Path;
use std::sync::{Arc, Mutex};

use core_app::{App, KnownPosition, LegSpec, RouteFrom};
use core_routing::{
    LatLon, MatrixEntry, MatrixRequest, Profile, Route, RouteOptions, RouteRequest, Router,
    RoutingError,
};
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

fn place(name: &str, lat: f64, lon: f64) -> Entry {
    entry(Kind::Place, None, name, lat, lon)
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
    app.search(text, NEAR, 50_000.0, 30)
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
    assert!(
        app.search("Hostel", NEAR, 50_000.0, 30)
            .unsearchable
            .is_empty()
    );
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
    let results = common::open(&dir).search("Hostel", NEAR, 50_000.0, 30);
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
    let results = common::open(&dir).search("Hostel", NEAR, 50_000.0, 30);
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
        .map(|i| lodging("Hostel", -13.5, -72.0 + f64::from(i) * 0.001))
        .collect();
    region(&dir, "a", "A", Some(&entries));
    let app = common::open(&dir);
    assert_eq!(
        app.search("Hostel", NEAR, 50_000.0, usize::MAX).hits.len(),
        100
    );
    assert_eq!(app.search("Hostel", NEAR, 50_000.0, 0).hits.len(), 1);
    let by_category = app
        .search_category("lodging", NEAR, 50_000.0, usize::MAX)
        .unwrap();
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
        .search_category("lodging", NEAR, 50_000.0, 30)
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
        .search_category("nonsense", NEAR, 50_000.0, 30)
        .unwrap_err();
    assert_eq!(error.code(), "search_category");
}

#[test]
fn radius_keeps_near_hits_and_places_everywhere() {
    let dir = common::temp_dir("search-radius");
    region(
        &dir,
        "a",
        "A",
        Some(&[
            lodging("Hostel Nah", -13.51, -72.0),
            lodging("Hostel Fern", -13.9, -72.0),
            place("Hostelito", -13.95, -72.0),
        ]),
    );
    let results = common::open(&dir).search("Hostel", NEAR, 10_000.0, 30);
    let names: Vec<_> = results.hits.iter().map(|h| h.name.as_str()).collect();
    assert!(names.contains(&"Hostel Nah") && names.contains(&"Hostelito"));
    assert!(!names.contains(&"Hostel Fern"));
    assert!(!results.widened);
}

#[test]
fn nothing_in_radius_shows_the_nearest_with_a_note() {
    let dir = common::temp_dir("search-widened");
    region(
        &dir,
        "a",
        "A",
        Some(&[
            lodging("Hostel Fern", -13.9, -72.0),
            place("Hostelito", -13.95, -72.0),
        ]),
    );
    let results = common::open(&dir).search("Hostel", NEAR, 10_000.0, 30);
    assert_eq!(results.hits.len(), 2);
    assert!(results.widened);
}

#[test]
fn a_far_place_with_the_whole_word_is_no_widening() {
    let dir = common::temp_dir("search-far-place");
    region(
        &dir,
        "a",
        "A",
        Some(&[
            place("Cusco", -13.9, -72.0),
            lodging("Cusco Hostel", -13.91, -72.0),
        ]),
    );
    let results = common::open(&dir).search("Cusco", NEAR, 10_000.0, 30);
    assert_eq!(results.hits[0].name, "Cusco");
    assert_eq!(results.hits.len(), 2);
    assert!(!results.widened);
}

#[test]
fn category_stays_inside_the_radius() {
    let dir = common::temp_dir("search-category-radius");
    region(&dir, "a", "A", Some(&[lodging("Fern", -13.9, -72.0)]));
    let results = common::open(&dir)
        .search_category("lodging", NEAR, 10_000.0, 30)
        .unwrap();
    assert!(results.hits.is_empty());
    assert!(!results.widened);
}

#[test]
fn radius_applies_across_regions() {
    let dir = common::temp_dir("search-radius-regions");
    region(
        &dir,
        "a",
        "A",
        Some(&[lodging("Hostel Nah", -13.51, -72.0)]),
    );
    region(
        &dir,
        "b",
        "B",
        Some(&[lodging("Hostel Fern", -13.9, -72.0)]),
    );
    let results = common::open(&dir).search("Hostel", NEAR, 10_000.0, 30);
    assert_eq!(results.hits.len(), 1);
    assert_eq!(results.hits[0].name, "Hostel Nah");
}

#[test]
fn radius_is_held_between_2_and_50_km() {
    let dir = common::temp_dir("search-radius-limits");
    region(&dir, "a", "A", Some(&[lodging("Hostel", -13.51, -72.0)]));
    let app = common::open(&dir);
    assert_eq!(app.search("Hostel", NEAR, 1.0, 30).radius_m, 2_000.0);
    assert_eq!(app.search("Hostel", NEAR, 1e9, 30).radius_m, 50_000.0);
    assert_eq!(app.search("Hostel", NEAR, -5.0, 30).radius_m, 2_000.0);
    for invalid in [f64::NAN, f64::INFINITY] {
        assert_eq!(app.search("Hostel", NEAR, invalid, 30).radius_m, 50_000.0);
    }
}

fn far_places(name: &str, count: u32) -> Vec<Entry> {
    (0..count)
        .map(|i| place(name, -13.9 - f64::from(i) * 0.01, -72.0))
        .collect()
}

#[test]
fn many_far_places_do_not_push_out_a_near_hit() {
    let dir = common::temp_dir("search-many-places");
    let mut entries = far_places("Neustadt", 35);
    entries.push(lodging("Neustadt Hostel", -13.51, -72.0));
    region(&dir, "a", "A", Some(&entries));
    let results = common::open(&dir).search("Neustadt", NEAR, 10_000.0, 30);
    assert!(results.hits.iter().any(|h| h.name == "Neustadt Hostel"));
    assert_eq!(results.hits.len(), 30);
}

#[test]
fn far_places_of_two_regions_do_not_push_out_a_near_hit() {
    let dir = common::temp_dir("search-many-places-regions");
    let mut first = far_places("Neustadt", 20);
    first.push(lodging("Neustadt Hostel", -13.51, -72.0));
    let second: Vec<Entry> = (0..20)
        .map(|i| place("Neustadt", -13.2 + f64::from(i) * 0.01, -72.0))
        .collect();
    region(&dir, "a", "A", Some(&first));
    region(&dir, "b", "B", Some(&second));
    let results = common::open(&dir).search("Neustadt", NEAR, 10_000.0, 30);
    assert!(results.hits.iter().any(|h| h.name == "Neustadt Hostel"));
}

#[test]
fn many_far_places_do_not_push_out_a_category_hit() {
    let dir = common::temp_dir("search-many-camping");
    let mut entries = far_places("Camping", 35);
    entries.push(entry(
        Kind::Poi,
        Some(Category::Camping),
        "Zeltplatz",
        -13.51,
        -72.0,
    ));
    region(&dir, "a", "A", Some(&entries));
    let results = common::open(&dir).search("camping", NEAR, 10_000.0, 30);
    assert!(results.hits.iter().any(|h| h.name == "Zeltplatz"));
}

#[test]
fn partly_typed_place_survives_many_streets() {
    let dir = common::temp_dir("search-partial-place");
    let mut entries = vec![entry(Kind::Street, None, "Berliner Strasse", -13.51, -72.0)];
    entries.extend((0..35).map(|i| {
        entry(
            Kind::Street,
            None,
            "Berliner Strasse",
            -13.7 - f64::from(i) * 0.001,
            -72.0,
        )
    }));
    entries.push(place("Berlin", -13.95, -72.0));
    region(&dir, "a", "A", Some(&entries));
    let results = common::open(&dir).search("Berl", NEAR, 10_000.0, 30);
    assert!(results.hits.iter().any(|h| h.name == "Berlin"));
}

#[test]
fn a_far_place_keeps_a_share_among_many_near_hits() {
    let dir = common::temp_dir("search-place-share");
    let mut entries: Vec<Entry> = (0..40)
        .map(|i| lodging("Neustadt Hostel", -13.5, -72.0 + f64::from(i) * 0.001))
        .collect();
    entries.push(place("Neustadt", -13.95, -72.0));
    region(&dir, "a", "A", Some(&entries));
    let results = common::open(&dir).search("Neustadt", NEAR, 10_000.0, 30);
    assert_eq!(results.hits.len(), 30);
    assert_eq!(results.hits[0].name, "Neustadt");
}

// Routing from the own position.

const WIDE: [f64; 4] = [-74.0, -15.0, -70.0, -12.0];
/// The own position, 27 km east of the map centre.
const POS: LatLon = LatLon {
    lat: -13.5,
    lon: -71.5,
};
const CENTRE: LatLon = LatLon {
    lat: -13.5,
    lon: -71.75,
};

/// A region with a search file, and a (dummy) routing file if `routing`.
fn routed_region(dir: &Path, id: &str, bbox: [f64; 4], routing: bool, entries: &[Entry]) {
    let region = dir.join("regions").join(id);
    std::fs::create_dir_all(&region).unwrap();
    let mut writer =
        SearchWriter::create(&region.join("search.sqlite"), Some("2026-11-01T00:00:00Z")).unwrap();
    for e in entries {
        writer.add(e).unwrap();
    }
    writer.finish().unwrap();
    let mut files = vec![(
        "search.sqlite",
        std::fs::metadata(region.join("search.sqlite"))
            .unwrap()
            .len(),
    )];
    if routing {
        std::fs::write(region.join("routing.tar"), b"dummy").unwrap();
        files.push(("routing.tar", 5));
    }
    let size: u64 = files.iter().map(|f| f.1).sum();
    let files: Vec<_> = files
        .iter()
        .map(|(name, size)| serde_json::json!({ "name": name, "size": size, "sha256": "0".repeat(64) }))
        .collect();
    let manifest = serde_json::json!({
        "format": 1, "id": id, "name": id, "bbox": bbox,
        "version": "1", "attribution": [], "files": files, "size": size,
    });
    std::fs::write(region.join("manifest.json"), manifest.to_string()).unwrap();
}

/// Answers the matrix from a table by target position, and keeps the calls.
struct FakeMatrix {
    table: Vec<(LatLon, Option<MatrixEntry>)>,
    fail: bool,
    calls: Mutex<Vec<MatrixRequest>>,
}

impl Router for FakeMatrix {
    fn route(&self, _: &RouteRequest) -> Result<Route, RoutingError> {
        Err(RoutingError::Engine("no route".into()))
    }

    fn matrix(&self, request: &MatrixRequest) -> Result<Vec<Option<MatrixEntry>>, RoutingError> {
        self.calls.lock().unwrap().push(request.clone());
        if self.fail {
            return Err(RoutingError::Engine("broken".into()));
        }
        Ok(request
            .to
            .iter()
            .map(|t| {
                self.table
                    .iter()
                    .find(|(at, _)| (at.lat - t.lat).abs() < 1e-9 && (at.lon - t.lon).abs() < 1e-9)
                    .and_then(|(_, entry)| *entry)
            })
            .collect())
    }

    fn profiles(&self) -> Vec<Profile> {
        Vec::new()
    }
}

fn way(distance_m: f64) -> Option<MatrixEntry> {
    Some(MatrixEntry {
        distance_m,
        duration_s: distance_m / 1.25,
    })
}

fn at(lat: f64, lon: f64) -> LatLon {
    LatLon { lat, lon }
}

fn fake(table: Vec<(LatLon, Option<MatrixEntry>)>) -> Arc<FakeMatrix> {
    Arc::new(FakeMatrix {
        table,
        fail: false,
        calls: Mutex::new(Vec::new()),
    })
}

fn from(profile: Profile, age_s: f64) -> RouteFrom {
    RouteFrom {
        position: KnownPosition {
            lat: POS.lat,
            lon: POS.lon,
            age_s,
        },
        leg: LegSpec {
            profile,
            options: RouteOptions::default(),
        },
    }
}

fn lodgings(dir: &Path, entries: &[Entry]) -> App {
    routed_region(dir, "a", WIDE, true, entries);
    common::open(dir)
}

fn hostel_pair() -> [Entry; 2] {
    [
        lodging("Hostel A", -13.509, -71.5),
        lodging("Hostel B", -13.518, -71.5),
    ]
}

#[test]
fn without_position_nothing_changes() {
    let dir = common::temp_dir("route-none");
    let app = lodgings(&dir, &hostel_pair());
    let router = fake(vec![(at(-13.509, -71.5), way(6_000.0))]);
    app.set_router_for_tests("a", router.clone());
    let plain = app.search("Hostel", CENTRE, 50_000.0, 30);
    assert_eq!(app.search_from("Hostel", CENTRE, 50_000.0, 30, None), plain);
    let old = from(Profile::Trekking, 121.0);
    assert_eq!(
        app.search_from("Hostel", CENTRE, 50_000.0, 30, Some(&old)),
        plain
    );
    assert!(
        plain
            .hits
            .iter()
            .all(|h| h.route_m.is_none() && !h.no_route)
    );
    assert_eq!(plain.hits[0].distance_m, plain.hits[0].centre_distance_m);
    assert!(router.calls.lock().unwrap().is_empty());
}

#[test]
fn routed_hits_sort_by_route_distance() {
    let dir = common::temp_dir("route-sort");
    let app = lodgings(&dir, &hostel_pair());
    app.set_router_for_tests(
        "a",
        fake(vec![
            (at(-13.509, -71.5), way(6_000.0)),
            (at(-13.518, -71.5), way(3_000.0)),
        ]),
    );
    let found = app
        .search_category_from(
            "lodging",
            CENTRE,
            50_000.0,
            30,
            Some(&from(Profile::Trekking, 5.0)),
        )
        .unwrap();
    let names: Vec<_> = found.hits.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(names, ["Hostel B", "Hostel A"]);
    let b = &found.hits[0];
    assert_eq!(b.route_m, Some(3_000.0));
    assert_eq!(b.route_s, Some(2_400.0));
    assert!((b.distance_m - 2_000.0).abs() < 20.0, "{}", b.distance_m);
    assert!(
        (b.centre_distance_m - 27_000.0).abs() < 600.0,
        "{}",
        b.centre_distance_m
    );
    assert!(!found.ascent_ignored);
}

#[test]
fn match_quality_goes_before_route() {
    let dir = common::temp_dir("route-quality");
    let app = lodgings(
        &dir,
        &[
            place("Seeblick", -13.527, -71.5),
            lodging("Seeblickhof", -13.509, -71.5),
        ],
    );
    app.set_router_for_tests(
        "a",
        fake(vec![
            (at(-13.527, -71.5), way(3_000.0)),
            (at(-13.509, -71.5), way(1_000.0)),
        ]),
    );
    let found = app.search_from(
        "Seeblick",
        CENTRE,
        50_000.0,
        30,
        Some(&from(Profile::Trekking, 5.0)),
    );
    let names: Vec<_> = found.hits.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(names, ["Seeblick", "Seeblickhof"]);
}

#[test]
fn hit_without_route_goes_after_routed_ones() {
    let dir = common::temp_dir("route-noway");
    let app = lodgings(
        &dir,
        &[
            lodging("Hostel A", -13.509, -71.5),
            lodging("Hostel B", -13.518, -71.5),
            lodging("Hostel C", -13.527, -71.5),
        ],
    );
    app.set_router_for_tests(
        "a",
        fake(vec![
            (at(-13.509, -71.5), None),
            (at(-13.518, -71.5), way(9_000.0)),
            (at(-13.527, -71.5), way(8_000.0)),
        ]),
    );
    let found = app
        .search_category_from(
            "lodging",
            CENTRE,
            50_000.0,
            30,
            Some(&from(Profile::Trekking, 5.0)),
        )
        .unwrap();
    let got: Vec<_> = found
        .hits
        .iter()
        .map(|h| (h.name.as_str(), h.no_route))
        .collect();
    assert_eq!(
        got,
        [("Hostel C", false), ("Hostel B", false), ("Hostel A", true)]
    );
}

#[test]
fn far_hits_are_not_routed() {
    let dir = common::temp_dir("route-far");
    let app = lodgings(
        &dir,
        &[
            lodging("Hostel A", -13.509, -71.5),
            lodging("Hostel B", -13.518, -71.5),
            lodging("Hostel Fern", -13.5, -72.05),
        ],
    );
    let router = fake(vec![
        (at(-13.509, -71.5), way(6_000.0)),
        (at(-13.518, -71.5), None),
    ]);
    app.set_router_for_tests("a", router.clone());
    let found = app
        .search_category_from(
            "lodging",
            CENTRE,
            50_000.0,
            30,
            Some(&from(Profile::Trekking, 5.0)),
        )
        .unwrap();
    let got: Vec<_> = found
        .hits
        .iter()
        .map(|h| (h.name.as_str(), h.no_route))
        .collect();
    assert_eq!(
        got,
        [
            ("Hostel A", false),
            ("Hostel B", true),
            ("Hostel Fern", false)
        ]
    );
    let calls = router.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].to.len(), 2);
}

#[test]
fn target_outside_the_region_has_no_route() {
    let dir = common::temp_dir("route-outside");
    routed_region(
        &dir,
        "a",
        [-71.9, -15.0, -70.0, -12.0],
        true,
        &[lodging("Hostel A", -13.509, -71.5)],
    );
    routed_region(
        &dir,
        "b",
        [-75.0, -15.0, -71.9, -12.0],
        false,
        &[lodging("Hostel West", -13.5, -71.95)],
    );
    let app = common::open(&dir);
    let router = fake(vec![(at(-13.509, -71.5), way(6_000.0))]);
    app.set_router_for_tests("a", router.clone());
    let found = app
        .search_category_from(
            "lodging",
            CENTRE,
            50_000.0,
            30,
            Some(&from(Profile::Trekking, 5.0)),
        )
        .unwrap();
    let got: Vec<_> = found
        .hits
        .iter()
        .map(|h| (h.name.as_str(), h.no_route))
        .collect();
    assert_eq!(got, [("Hostel A", false), ("Hostel West", true)]);
    assert_eq!(router.calls.lock().unwrap()[0].to.len(), 1);
}

#[test]
fn failed_matrix_falls_back_to_air_distance() {
    let dir = common::temp_dir("route-fail");
    let app = lodgings(&dir, &hostel_pair());
    let broken = Arc::new(FakeMatrix {
        table: Vec::new(),
        fail: true,
        calls: Mutex::new(Vec::new()),
    });
    app.set_router_for_tests("a", broken);
    for profile in [Profile::Trekking, Profile::Manual] {
        let found = app
            .search_category_from("lodging", CENTRE, 50_000.0, 30, Some(&from(profile, 5.0)))
            .unwrap();
        let names: Vec<_> = found.hits.iter().map(|h| h.name.as_str()).collect();
        assert_eq!(names, ["Hostel A", "Hostel B"]);
        assert!(
            found
                .hits
                .iter()
                .all(|h| h.route_m.is_none() && !h.no_route)
        );
        assert!((found.hits[0].distance_m - 1_000.0).abs() < 20.0);
        assert!(!found.ascent_ignored);
    }
    // A position outside every routing region.
    let lonely = RouteFrom {
        position: KnownPosition {
            lat: 10.0,
            lon: 10.0,
            age_s: 5.0,
        },
        leg: from(Profile::Trekking, 5.0).leg,
    };
    let found = app
        .search_category_from("lodging", CENTRE, 50_000.0, 30, Some(&lonely))
        .unwrap();
    assert!(
        found
            .hits
            .iter()
            .all(|h| h.route_m.is_none() && !h.no_route)
    );
}

#[test]
fn far_places_keep_their_slots() {
    let dir = common::temp_dir("route-places");
    let app = lodgings(
        &dir,
        &[
            lodging("Seeblick Hostel", -13.509, -71.5),
            place("Seeblickort", -13.5, -73.5),
        ],
    );
    let router = fake(vec![(at(-13.509, -71.5), way(6_000.0))]);
    app.set_router_for_tests("a", router.clone());
    let found = app.search_from(
        "Seeblick",
        CENTRE,
        2_000.0,
        30,
        Some(&from(Profile::Trekking, 5.0)),
    );
    let names: Vec<_> = found.hits.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(names, ["Seeblick Hostel", "Seeblickort"]);
    let far = &found.hits[1];
    assert!(far.route_m.is_none() && !far.no_route);
    assert!(
        (far.distance_m - 216_000.0).abs() < 5_000.0,
        "{}",
        far.distance_m
    );
    assert_eq!(router.calls.lock().unwrap()[0].to.len(), 1);
}

#[test]
fn ascent_note_only_for_hiking() {
    let dir = common::temp_dir("route-ascent");
    let app = lodgings(&dir, &hostel_pair());
    app.set_router_for_tests("a", fake(vec![(at(-13.509, -71.5), way(1_500.0))]));
    for (profile, expected) in [(Profile::Hike, true), (Profile::Trekking, false)] {
        let found = app
            .search_category_from("lodging", CENTRE, 50_000.0, 30, Some(&from(profile, 5.0)))
            .unwrap();
        assert_eq!(found.ascent_ignored, expected, "{profile:?}");
    }
}

#[test]
fn only_shown_hits_are_routed() {
    let dir = common::temp_dir("route-limit");
    let entries: Vec<Entry> = (0..5)
        .map(|i| {
            lodging(
                &format!("Hostel {i}"),
                -13.5 - 0.009 * f64::from(i + 1),
                -71.5,
            )
        })
        .collect();
    let app = lodgings(&dir, &entries);
    let router = fake(Vec::new());
    app.set_router_for_tests("a", router.clone());
    let found = app
        .search_category_from(
            "lodging",
            CENTRE,
            50_000.0,
            3,
            Some(&from(Profile::Trekking, 5.0)),
        )
        .unwrap();
    assert_eq!(found.hits.len(), 3);
    let calls = router.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].to.len(), 3);
}

#[test]
fn exact_far_town_ranks_above_partial_hits_with_position() {
    let dir = common::temp_dir("route-far-town");
    let app = lodgings(
        &dir,
        &[
            lodging("Seeblickhof", -13.509, -71.5),
            place("Seeblick", -13.5, -73.5),
        ],
    );
    app.set_router_for_tests("a", fake(vec![(at(-13.509, -71.5), way(6_000.0))]));
    let with = app.search_from(
        "Seeblick",
        CENTRE,
        2_000.0,
        30,
        Some(&from(Profile::Trekking, 5.0)),
    );
    let without = app.search("Seeblick", CENTRE, 2_000.0, 30);
    let order = |r: &core_app::SearchResults| -> Vec<String> {
        r.hits.iter().map(|h| h.name.clone()).collect()
    };
    assert_eq!(order(&with), ["Seeblick", "Seeblickhof"]);
    assert_eq!(order(&without), order(&with));
}
