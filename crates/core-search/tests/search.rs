#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use core_search::{Category, Entry, Hit, Kind, SearchIndex, SearchWriter, index_terms, normalize};

fn entry(kind: Kind, name: &str, lat: f64, lon: f64) -> Entry {
    Entry {
        kind,
        category: None,
        subtype: None,
        name: name.to_string(),
        alt_names: Vec::new(),
        town: None,
        town_names: Vec::new(),
        lat,
        lon,
    }
}

fn build(dir: &Path, entries: &[Entry]) -> PathBuf {
    let path = dir.join("search.sqlite");
    let mut writer = SearchWriter::create(&path, Some("2026-11-01T00:00:00Z")).unwrap();
    for e in entries {
        writer.add(e).unwrap();
    }
    writer.finish().unwrap();
    path
}

fn names(hits: &[Hit]) -> Vec<&str> {
    hits.iter().map(|h| h.name.as_str()).collect()
}

fn sample() -> Vec<Entry> {
    let mut street = entry(Kind::Street, "Hauptstraße", 47.58, 12.17);
    street.town = Some("Kufstein".into());
    let mut abbreviated = entry(Kind::Street, "Hauptstr.", 47.60, 12.20);
    abbreviated.town = Some("Ebbs".into());
    let mut hostel = entry(Kind::Poi, "Hostel Alpina", 46.50, 11.35);
    hostel.category = Some(Category::Lodging);
    let mut bozen = entry(Kind::Place, "Bozen", 46.50, 11.35);
    bozen.alt_names = vec!["Bolzano".into()];
    let mut parking = entry(Kind::Poi, "", 47.00, 11.00);
    parking.category = Some(Category::Parking);
    let mut far_parking = entry(Kind::Poi, "", 10.00, 11.00);
    far_parking.category = Some(Category::Parking);
    let mut near_parking = entry(Kind::Poi, "", 47.01, 11.00);
    near_parking.category = Some(Category::Parking);
    vec![
        street,
        abbreviated,
        hostel,
        bozen,
        entry(Kind::Place, "Peña Blanca", 40.0, -4.0),
        entry(Kind::Poi, "Bar Grill", 48.0, 11.0),
        far_parking,
        parking,
        near_parking,
    ]
}

#[test]
fn fts5_is_available() {
    let dir = tempfile_dir("fts5");
    build(&dir, &[]);
}

#[test]
fn umlauts_and_sharp_s() {
    assert_eq!(normalize("Straße"), "strasse");
    assert_eq!(normalize("München"), "munchen");
    assert!(index_terms("München").contains(&"muenchen".to_string()));
    assert!(index_terms("München").contains(&"munchen".to_string()));
    assert!(index_terms("Hauptstr.").contains(&"hauptstrasse".to_string()));
}

#[test]
fn every_town_name_is_searchable_but_only_the_town_is_shown() {
    let dir = tempfile_dir("town-names");
    let mut street = entry(Kind::Street, "Testgasse", 48.0, 11.0);
    street.town = Some("Lehel".into());
    street.town_names = vec!["Lehel".into(), "München".into(), "Munich".into()];
    let index = SearchIndex::open(&build(&dir, &[street])).unwrap();
    for text in ["Testgasse Lehel", "Testgasse München", "Testgasse Munich"] {
        let hits = index.search(text, None, 5).unwrap();
        assert_eq!(names(&hits), ["Testgasse"], "{text}");
        assert_eq!(hits[0].town.as_deref(), Some("Lehel"), "{text}");
    }
    assert!(
        index
            .search("Testgasse Berlin", None, 5)
            .unwrap()
            .is_empty()
    );
    assert!(index.search("München", None, 5).unwrap().is_empty());
}

#[test]
fn best_match_comes_first_whatever_the_order_of_the_rows() {
    let dir = tempfile_dir("rank-order");
    // The short name is the better match but is stored last.
    let long = entry(
        Kind::Poi,
        "Alpen Haus Hof Hütte Stube Keller Garten",
        48.0,
        11.0,
    );
    let short = entry(Kind::Poi, "Alpen", 47.0, 11.0);
    let index = SearchIndex::open(&build(&dir, &[long, short])).unwrap();
    let hits = index.search("Alpen", None, 5).unwrap();
    assert_eq!(hits[0].name, "Alpen");
}

#[test]
fn a_place_with_many_names_comes_before_other_matches() {
    let dir = tempfile_dir("place-first");
    let mut place = entry(Kind::Place, "Cuzco", -13.5, -72.0);
    place.alt_names = [
        "Cusco", "Qusqu", "Kusko", "Cuscum", "Kuzco", "Koskoo", "Cozco", "Qosqo",
    ]
    .map(String::from)
    .to_vec();
    let poi = entry(Kind::Poi, "Cusco", -13.5, -72.0);
    let street = entry(Kind::Street, "Avenida Cusco", -13.6, -72.0);
    let index = SearchIndex::open(&build(&dir, &[poi, street, place])).unwrap();
    let hits = index.search("Cusco", None, 5).unwrap();
    assert_eq!(hits[0].kind, Kind::Place, "{:?}", names(&hits));
    assert_eq!(hits[0].name, "Cuzco");
}

#[test]
fn name_prefix_is_found() {
    let dir = tempfile_dir("prefix");
    let index = SearchIndex::open(&build(&dir, &sample())).unwrap();
    let hits = index.search("hos", None, 10).unwrap();
    assert_eq!(names(&hits), ["Hostel Alpina"]);
    assert_eq!(hits[0].category, Some(Category::Lodging));
}

#[test]
fn street_shows_its_town() {
    let dir = tempfile_dir("town");
    let index = SearchIndex::open(&build(&dir, &sample())).unwrap();
    let hits = index.search("Hauptstraße", None, 10).unwrap();
    assert!(hits.iter().any(|h| h.town.as_deref() == Some("Kufstein")));
}

#[test]
fn street_abbreviation_is_found() {
    let dir = tempfile_dir("abbr");
    let index = SearchIndex::open(&build(&dir, &sample())).unwrap();
    for query in ["Hauptstr", "Hauptstraße"] {
        let hits = index.search(query, None, 10).unwrap();
        let mut found = names(&hits);
        found.sort_unstable();
        assert_eq!(found, ["Hauptstr.", "Hauptstraße"], "query {query}");
    }
}

#[test]
fn alternative_name_is_found() {
    let dir = tempfile_dir("alt");
    let index = SearchIndex::open(&build(&dir, &sample())).unwrap();
    for query in ["Bolzano", "Bozen"] {
        let hits = index.search(query, None, 10).unwrap();
        assert_eq!(names(&hits), ["Bozen"], "query {query}");
    }
}

#[test]
fn spanish_names_are_found_without_accents() {
    let dir = tempfile_dir("es");
    let index = SearchIndex::open(&build(&dir, &sample())).unwrap();
    assert_eq!(
        names(&index.search("pena", None, 10).unwrap()),
        ["Peña Blanca"]
    );
    assert_eq!(
        names(&index.search("Peña", None, 10).unwrap()),
        ["Peña Blanca"]
    );
}

#[test]
fn nearer_hits_come_first() {
    let dir = tempfile_dir("near");
    let entries = vec![
        entry(Kind::Street, "Hauptstraße", 47.0, 11.0),
        entry(Kind::Street, "Hauptstraße", 48.0, 12.0),
    ];
    let index = SearchIndex::open(&build(&dir, &entries)).unwrap();
    let hits = index.search("Hauptstraße", Some((48.0, 12.0)), 10).unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].lat, 48.0);
    assert!(hits[0].distance_m.unwrap() < 1.0);
    assert!(hits[1].distance_m.unwrap() > 100_000.0);
    assert!(
        index.search("Hauptstraße", None, 10).unwrap()[0]
            .distance_m
            .is_none()
    );
}

#[test]
fn category_finds_unnamed_places() {
    let dir = tempfile_dir("category");
    let index = SearchIndex::open(&build(&dir, &sample())).unwrap();
    let hits = index
        .by_category(Category::Parking, (47.0, 11.0), 2)
        .unwrap();
    assert_eq!(hits.len(), 2);
    assert!(hits.iter().all(|h| h.name.is_empty()));
    assert_eq!((hits[0].lat, hits[1].lat), (47.0, 47.01));
    // Widening finds the far one when asked for more than are close by.
    let all = index
        .by_category(Category::Parking, (47.0, 11.0), 10)
        .unwrap();
    assert_eq!(all.len(), 3);
    assert_eq!(all[2].lat, 10.0);
}

#[test]
fn query_survives_search_syntax() {
    let dir = tempfile_dir("syntax");
    let index = SearchIndex::open(&build(&dir, &sample())).unwrap();
    for q in ["\"", "*", "a AND", "NEAR(", "Bar & Grill", "...", ""] {
        assert!(index.search(q, None, 10).is_ok(), "query {q:?}");
    }
    assert_eq!(
        names(&index.search("Bar & Grill", None, 10).unwrap()),
        ["Bar Grill"]
    );
}

#[test]
fn reading_leaves_the_file_unchanged() {
    let dir = tempfile_dir("readonly");
    let path = build(&dir, &sample());
    let before = std::fs::read(&path).unwrap();
    let index = SearchIndex::open(&path).unwrap();
    index.search("Hauptstr", Some((47.0, 11.0)), 5).unwrap();
    index
        .by_category(Category::Parking, (47.0, 11.0), 5)
        .unwrap();
    drop(index);
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let mut files: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    files.sort();
    assert_eq!(files, ["search.sqlite"]);
}

#[test]
fn missing_or_foreign_format_is_an_error() {
    let dir = tempfile_dir("errors");
    assert!(SearchIndex::open(&dir.join("none.sqlite")).is_err());
    let foreign = dir.join("foreign.sqlite");
    let conn = rusqlite::Connection::open(&foreign).unwrap();
    conn.execute_batch(
        "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
         INSERT INTO meta VALUES ('format', '2');",
    )
    .unwrap();
    drop(conn);
    assert!(matches!(
        SearchIndex::open(&foreign),
        Err(core_search::SearchError::Format { .. })
    ));
}

fn tempfile_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("core-search-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn nearest_hit_survives_many_matches() {
    let dir = tempfile_dir("many");
    let mut entries: Vec<Entry> = (0..2500)
        .map(|i| {
            entry(
                Kind::Street,
                "Hauptstraße",
                10.0 + f64::from(i) * 1e-4,
                20.0,
            )
        })
        .collect();
    entries.push(entry(Kind::Street, "Hauptstraße", 47.0, 11.0));
    let index = SearchIndex::open(&build(&dir, &entries)).unwrap();
    let hits = index.search("Hauptstr", Some((47.0, 11.0)), 1).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].lat, 47.0);
    assert!(hits[0].distance_m.unwrap() < 1.0);
}

fn with_town() -> Vec<Entry> {
    let mut innsbruck = entry(Kind::Street, "Erlerstraße", 47.26, 11.39);
    innsbruck.town = Some("Innsbruck".into());
    let mut hall = entry(Kind::Street, "Erlerstraße", 47.28, 11.50);
    hall.town = Some("Hall in Tirol".into());
    let mut place = entry(Kind::Place, "Innsbruck", 47.2654, 11.3933);
    place.town = None;
    vec![innsbruck, hall, place]
}

fn towns(hits: &[Hit]) -> Vec<Option<&str>> {
    hits.iter().map(|h| h.town.as_deref()).collect()
}

#[test]
fn street_is_found_with_its_town() {
    let dir = tempfile_dir("town-found");
    let index = SearchIndex::open(&build(&dir, &with_town())).unwrap();
    let hits = index.search("Erlerstraße Innsbruck", None, 10).unwrap();
    assert_eq!(towns(&hits), vec![Some("Innsbruck")]);
}

#[test]
fn town_alone_does_not_match_its_streets() {
    let dir = tempfile_dir("town-alone");
    let index = SearchIndex::open(&build(&dir, &with_town())).unwrap();
    let hits = index.search("Innsbruck", None, 10).unwrap();
    assert_eq!(names(&hits), vec!["Innsbruck"]);
    assert_eq!(hits[0].kind, Kind::Place);
}

#[test]
fn town_prefix_works() {
    let dir = tempfile_dir("town-prefix");
    let index = SearchIndex::open(&build(&dir, &with_town())).unwrap();
    let hits = index.search("Erlerstr Innsb", None, 10).unwrap();
    assert_eq!(towns(&hits), vec![Some("Innsbruck")]);
}

#[test]
fn words_in_any_order() {
    let dir = tempfile_dir("town-order");
    let index = SearchIndex::open(&build(&dir, &with_town())).unwrap();
    let hits = index.search("Innsbruck Erlerstraße", None, 10).unwrap();
    assert_eq!(towns(&hits), vec![Some("Innsbruck")]);
}

#[test]
fn other_town_does_not_match() {
    let dir = tempfile_dir("town-other");
    let index = SearchIndex::open(&build(&dir, &with_town())).unwrap();
    assert!(
        index
            .search("Erlerstraße Kufstein", Some((47.0, 11.0)), 10)
            .unwrap()
            .is_empty()
    );
}
