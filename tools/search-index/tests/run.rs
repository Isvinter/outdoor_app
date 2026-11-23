//! Building a search file from a small GeoJSON-sequence export.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use core_search::{Category, Kind, SearchIndex};
use search_index::{BuildError, build};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mini.geojsonseq")
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("search-index-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Builds the fixture and opens the result.
fn built(name: &str) -> (SearchIndex, core_maps::SearchInfo, PathBuf) {
    let dir = temp(name);
    let out = dir.join("search.sqlite");
    let info = build(&fixture(), &out, Some("2026-11-01T00:00:00Z")).unwrap();
    (SearchIndex::open(&out).unwrap(), info, dir)
}

const KUFSTEIN: (f64, f64) = (47.583, 12.167);

#[test]
fn categories_follow_the_design() {
    let (index, _, _) = built("categories");
    let cases = [
        ("Tankstelle Eins", Category::Fuel, "amenity=fuel"),
        ("Billa", Category::Groceries, "shop=supermarket"),
        ("Baeckerei Mayer", Category::Retail, "shop=bakery"),
        ("Gasthof Post", Category::Food, "amenity=restaurant"),
        ("Apotheke Zentrum", Category::Pharmacy, "amenity=pharmacy"),
        ("Hostel Sonne", Category::Lodging, "tourism=hostel"),
        ("Camping Kufstein", Category::Camping, "tourism=camp_site"),
        ("Festungsmuseum", Category::Sights, "tourism=museum"),
        ("Burg", Category::Sights, "historic=castle"),
    ];
    for (name, category, subtype) in cases {
        let hits = index.search(name, Some(KUFSTEIN), 5).unwrap();
        let hit = hits
            .iter()
            .find(|h| h.name == name)
            .unwrap_or_else(|| panic!("{name} not found"));
        assert_eq!(hit.kind, Kind::Poi, "{name}");
        assert_eq!(hit.category, Some(category), "{name}");
        assert_eq!(hit.subtype.as_deref(), Some(subtype), "{name}");
    }
}

#[test]
fn poi_wins_over_place() {
    let (index, _, _) = built("poi-over-place");
    let hits = index.search("Cafe Vorort", None, 5).unwrap();
    assert_eq!(hits[0].kind, Kind::Poi);
    assert_eq!(hits[0].category, Some(Category::Food));
}

#[test]
fn private_parking_is_left_out() {
    let (index, _, _) = built("private");
    assert!(index.search("Privatparkplatz", None, 5).unwrap().is_empty());
}

#[test]
fn unnamed_poi_stays() {
    let (index, _, _) = built("unnamed");
    let hits = index.by_category(Category::Parking, KUFSTEIN, 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].name, "");
}

#[test]
fn same_feature_counts_once() {
    let (index, _, _) = built("once");
    let hits = index.search("Doppel Apotheke", None, 10).unwrap();
    assert_eq!(hits.len(), 1);
}

#[test]
fn street_segments_merge() {
    let (index, _, _) = built("merge");
    let hits: Vec<_> = index
        .search("Hauptstraße", None, 10)
        .unwrap()
        .into_iter()
        .filter(|h| h.kind == Kind::Street)
        .collect();
    assert_eq!(hits.len(), 2);
    assert!(hits.iter().any(|h| h.lat > 47.8));
}

#[test]
fn unnamed_streets_are_left_out() {
    let (index, info, _) = built("unnamed-streets");
    // Hauptstraße twice, Bahnhofstraße; the unnamed way and the footway do not count.
    assert_eq!(info.streets, 3);
    assert!(index.search("Fussweg", None, 5).unwrap().is_empty());
}

#[test]
fn towns_are_assigned() {
    let (index, _, _) = built("towns");
    let hits: Vec<_> = index
        .search("Hauptstraße", None, 10)
        .unwrap()
        .into_iter()
        .filter(|h| h.kind == Kind::Street)
        .collect();
    let near = hits.iter().find(|h| h.lat < 47.7).unwrap();
    let far = hits.iter().find(|h| h.lat > 47.8).unwrap();
    assert_eq!(near.town.as_deref(), Some("Kufstein"));
    assert_eq!(far.town, None);
    let place = index.search("Kufstein", None, 10).unwrap();
    let place = place.iter().find(|h| h.kind == Kind::Place).unwrap();
    assert_eq!(place.town, None);
}

#[test]
fn alternative_names_are_kept() {
    let (index, _, _) = built("alt");
    for text in [
        "Main Street",
        "Hauptstrasse",
        "Hauptstr",
        "Obere Gasse",
        "Hauptweg",
        "Hpt",
    ] {
        let hits = index.search(text, None, 10).unwrap();
        assert!(
            hits.iter()
                .any(|h| h.kind == Kind::Street && h.name == "Hauptstraße"),
            "{text} finds the street"
        );
    }
}

#[test]
fn info_counts_are_right() {
    let (_, info, dir) = built("info");
    assert_eq!(info.format, core_search::SEARCH_FORMAT);
    assert_eq!(info.places, 1);
    assert_eq!(info.streets, 3);
    // n10-n17, n19-n21, w30, r50 (the private car park is left out)
    assert_eq!(info.pois, 13);
    assert_eq!(info.named_pois, 12);
    assert!(!dir.join("search.sqlite.tmp").exists());
}

#[test]
fn broken_line_is_an_error() {
    let dir = temp("broken");
    let input = dir.join("bad.geojsonseq");
    std::fs::write(
        &input,
        "\u{1e}{\"type\":\"Feature\",\"id\":\"n1\",\"properties\":{},\"geometry\":{\"type\":\"Point\",\"coordinates\":[1,2]}}\n\u{1e}{not json\n",
    )
    .unwrap();
    let out = dir.join("search.sqlite");
    let err = build(&input, &out, None).unwrap_err();
    assert!(matches!(err, BuildError::Line { line: 2, .. }), "{err}");
    assert!(err.to_string().contains("line 2"), "{err}");
    assert!(!out.exists());
    assert!(!dir.join("search.sqlite.tmp").exists());
}

#[test]
fn stale_temporary_file_is_replaced() {
    let dir = temp("stale");
    let out = dir.join("search.sqlite");
    let first = build(&fixture(), &out, None).unwrap();
    let stale = dir.join("search.sqlite.tmp");
    // A crashed earlier run left a finished index behind.
    std::fs::copy(&out, &stale).unwrap();
    std::fs::remove_file(&out).unwrap();
    let again = build(&fixture(), &out, None).unwrap();
    assert_eq!(first, again);
    assert!(out.exists());
    assert!(!stale.exists());
}

fn parking_input(dir: &Path, second_id: &str) -> PathBuf {
    let input = dir.join("closed.geojsonseq");
    let line = |id: &str, geometry: &str| {
        format!(
            "\u{1e}{{\"type\":\"Feature\",\"id\":\"{id}\",\"properties\":{{\"amenity\":\"parking\"}},\"geometry\":{geometry}}}\n"
        )
    };
    let ring = "[[1,2],[1,3],[2,3],[1,2]]";
    std::fs::write(
        &input,
        line(
            "w11",
            &format!("{{\"type\":\"LineString\",\"coordinates\":{ring}}}"),
        ) + &line(
            second_id,
            &format!("{{\"type\":\"Polygon\",\"coordinates\":[{ring}]}}"),
        ),
    )
    .unwrap();
    input
}

#[test]
fn closed_way_exported_twice_counts_once() {
    let dir = temp("closed-way");
    let info = build(&parking_input(&dir, "a22"), &dir.join("s.sqlite"), None).unwrap();
    assert_eq!(info.pois, 1);
}

#[test]
fn area_of_a_relation_is_not_a_way() {
    let dir = temp("relation-area");
    // a23 is relation 11, a different object than way 11.
    let info = build(&parking_input(&dir, "a23"), &dir.join("s.sqlite"), None).unwrap();
    assert_eq!(info.pois, 2);
}

#[test]
fn broken_area_id_is_an_error() {
    let dir = temp("bad-area");
    let out = dir.join("s.sqlite");
    let err = build(&parking_input(&dir, "ax"), &out, None).unwrap_err();
    assert!(matches!(err, BuildError::Line { line: 2, .. }), "{err}");
    assert!(!out.exists());
}
