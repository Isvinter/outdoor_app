//! Which ways become one street, where it lies, and under which towns it is found.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use core_search::{Category, Hit, Kind, SearchIndex};
use search_index::build;
use serde_json::{Value, json};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("search-streets-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn node(id: u32, tags: Value, lat: f64, lon: f64) -> Value {
    json!({
        "type": "Feature",
        "id": format!("n{id}"),
        "properties": tags,
        "geometry": { "type": "Point", "coordinates": [lon, lat] },
    })
}

fn place(id: u32, rank: &str, name: &str, lat: f64, lon: f64) -> Value {
    node(id, json!({ "place": rank, "name": name }), lat, lon)
}

/// A residential way through `(lat, lon)` points.
fn way(id: u32, name: &str, points: &[(f64, f64)]) -> Value {
    let coordinates: Vec<[f64; 2]> = points.iter().map(|&(lat, lon)| [lon, lat]).collect();
    json!({
        "type": "Feature",
        "id": format!("w{id}"),
        "properties": { "highway": "residential", "name": name },
        "geometry": { "type": "LineString", "coordinates": coordinates },
    })
}

fn index(name: &str, features: &[Value]) -> SearchIndex {
    let dir = temp(name);
    let input = dir.join("input.geojsonseq");
    let text: String = features.iter().map(|f| format!("\u{1e}{f}\n")).collect();
    std::fs::write(&input, text).unwrap();
    let output = dir.join("search.sqlite");
    build(&input, &output, None).unwrap();
    SearchIndex::open(&output).unwrap()
}

fn streets(index: &SearchIndex, text: &str) -> Vec<Hit> {
    index
        .search(text, None, 20)
        .unwrap()
        .into_iter()
        .filter(|h| h.kind == Kind::Street)
        .collect()
}

/// A suburb inside a city, and a town far away.
fn city_with_suburb() -> Vec<Value> {
    vec![
        node(
            1,
            json!({ "place": "city", "name": "Grossstadt", "name:en": "Bigcity" }),
            48.0,
            11.0,
        ),
        place(2, "suburb", "Vorviertel", 48.02, 11.0),
        place(3, "town", "Fernstadt", 47.0, 11.0),
        way(10, "Testgasse", &[(48.021, 11.0), (48.021, 11.001)]),
    ]
}

#[test]
fn street_in_a_suburb_is_found_with_the_suburb_and_the_city() {
    let index = index("suburb-city", &city_with_suburb());
    for text in ["Testgasse Vorviertel", "Testgasse Grossstadt"] {
        let hits = streets(&index, text);
        assert_eq!(hits.len(), 1, "{text}");
        assert_eq!(hits[0].town.as_deref(), Some("Vorviertel"), "{text}");
    }
}

#[test]
fn street_is_found_with_the_alternative_name_of_the_city() {
    let index = index("suburb-alt", &city_with_suburb());
    assert_eq!(streets(&index, "Testgasse Bigcity").len(), 1);
}

#[test]
fn street_is_not_found_with_an_unrelated_town() {
    let index = index("suburb-unrelated", &city_with_suburb());
    assert!(streets(&index, "Testgasse Fernstadt").is_empty());
}

#[test]
fn city_alone_does_not_list_its_streets() {
    let index = index("suburb-alone", &city_with_suburb());
    assert!(streets(&index, "Grossstadt").is_empty());
}

#[test]
fn a_village_beyond_its_radius_is_not_searchable() {
    let index = index(
        "village-radius",
        &[
            place(1, "village", "Nahdorf", 48.0, 11.0),
            // About 5.5 km away: outside the 4 km of a village.
            place(2, "village", "Weitdorf", 48.05, 11.0),
            way(10, "Testgasse", &[(48.0, 11.001), (48.0, 11.002)]),
        ],
    );
    assert_eq!(streets(&index, "Testgasse Nahdorf").len(), 1);
    assert!(streets(&index, "Testgasse Weitdorf").is_empty());
}

#[test]
fn same_name_in_two_villages_stays_two_streets() {
    // 1.5 km apart, each with its own Dorfstraße.
    let index = index(
        "two-villages",
        &[
            place(1, "village", "Oberdorf", 48.0, 11.0),
            place(2, "village", "Unterdorf", 47.9865, 11.0),
            way(10, "Dorfstraße", &[(48.0, 11.0), (48.0, 11.002)]),
            way(11, "Dorfstraße", &[(47.9865, 11.0), (47.9865, 11.002)]),
        ],
    );
    assert_eq!(streets(&index, "Dorfstraße").len(), 2);
    let upper = streets(&index, "Dorfstraße Oberdorf");
    assert_eq!(upper.len(), 1);
    assert_eq!(upper[0].town.as_deref(), Some("Oberdorf"));
    assert!((upper[0].lat - 48.0).abs() < 1e-9);
    let lower = streets(&index, "Dorfstraße Unterdorf");
    assert_eq!(lower.len(), 1);
    assert_eq!(lower[0].town.as_deref(), Some("Unterdorf"));
    assert!((lower[0].lat - 47.9865).abs() < 1e-9);
}

#[test]
fn three_connected_ways_are_one_street() {
    let index = index(
        "three-ways",
        &[
            // The middle way comes last: joining must not depend on the order.
            way(10, "Langgasse", &[(48.0, 11.0), (48.0, 11.002)]),
            way(11, "Langgasse", &[(48.0, 11.004), (48.0, 11.006)]),
            way(12, "Langgasse", &[(48.0, 11.002), (48.0, 11.004)]),
        ],
    );
    assert_eq!(streets(&index, "Langgasse").len(), 1);
}

#[test]
fn nearly_touching_ways_join_but_a_gap_does_not() {
    let index = index(
        "gap",
        &[
            // About 75 m between the ends.
            way(10, "Nahgasse", &[(48.0, 11.0), (48.0, 11.002)]),
            way(11, "Nahgasse", &[(48.0, 11.003), (48.0, 11.005)]),
            // About 300 m between the ends.
            way(12, "Ferngasse", &[(47.0, 11.0), (47.0, 11.002)]),
            way(13, "Ferngasse", &[(47.0, 11.006), (47.0, 11.008)]),
        ],
    );
    assert_eq!(streets(&index, "Nahgasse").len(), 1);
    assert_eq!(streets(&index, "Ferngasse").len(), 2);
}

#[test]
fn a_long_connected_street_is_one_entry() {
    // Ten ways of about 750 m each, 7.5 km in all.
    let features: Vec<Value> = (0..10_u32)
        .map(|i| {
            let start = 11.0 + f64::from(i) * 0.01;
            way(10 + i, "Fernstraße", &[(48.0, start), (48.0, start + 0.01)])
        })
        .collect();
    let index = index("long", &features);
    assert_eq!(streets(&index, "Fernstraße").len(), 1);
}

#[test]
fn the_position_is_a_vertex_of_the_street() {
    // A bend: the mean of the points lies off the street.
    let bend = [(48.0, 11.0), (48.0, 11.01), (48.01, 11.01)];
    let index = index("vertex", &[way(10, "Kurvenweg", &bend)]);
    let hits = streets(&index, "Kurvenweg");
    assert_eq!(hits.len(), 1);
    assert!(
        bend.iter()
            .any(|&(lat, lon)| hits[0].lat == lat && hits[0].lon == lon),
        "{}, {} is not on the street",
        hits[0].lat,
        hits[0].lon
    );
}

#[test]
fn the_town_comes_from_the_position_on_the_street() {
    // The mean of the points (48.004, 11.008) lies beside Falschdorf; the vertex
    // closest to it is the bend at (48.0, 11.012), beside Richtigdorf.
    let index = index(
        "town-from-vertex",
        &[
            place(1, "hamlet", "Richtigdorf", 48.0, 11.013),
            place(2, "hamlet", "Falschdorf", 48.004, 11.008),
            way(
                10,
                "Kurvenweg",
                &[(48.0, 11.0), (48.0, 11.012), (48.012, 11.012)],
            ),
        ],
    );
    let hits = streets(&index, "Kurvenweg");
    assert_eq!(hits.len(), 1);
    assert_eq!((hits[0].lat, hits[0].lon), (48.0, 11.012));
    assert_eq!(hits[0].town.as_deref(), Some("Richtigdorf"));
}

#[test]
fn entry_without_name_shows_its_first_alternative_name() {
    let index = index(
        "alt-display",
        &[node(
            1,
            json!({ "tourism": "hostel", "name:en": "Mountain Hostel", "alt_name": "Berghaus" }),
            48.0,
            11.0,
        )],
    );
    let hits = index
        .by_category(Category::Lodging, (48.0, 11.0), 5)
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].name, "Mountain Hostel");
    assert_eq!(index.search("Berghaus", None, 5).unwrap().len(), 1);
}

#[test]
fn shops_that_are_not_open_are_left_out() {
    let index = index(
        "shops",
        &[
            node(1, json!({ "shop": "vacant", "name": "Leer" }), 48.0, 11.0),
            node(2, json!({ "shop": "no", "name": "Kein" }), 48.0, 11.0),
            node(3, json!({ "shop": "disused", "name": "Alt" }), 48.0, 11.0),
            node(4, json!({ "shop": "bakery", "name": "Offen" }), 48.0, 11.0),
        ],
    );
    let hits = index
        .by_category(Category::Retail, (48.0, 11.0), 10)
        .unwrap();
    let names: Vec<&str> = hits.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(names, ["Offen"]);
}

/// The node of Cusco as OSM has it, and a street beside it.
fn cusco() -> Vec<Value> {
    vec![
        node(
            1,
            json!({
                "place": "city",
                "name": "Cuzco",
                "official_name": "Cusco",
                "name:it": "Cusco",
                "name:qu": "Qusqu",
                "name:ru": "Куско",
                "name:left": "Linksname",
            }),
            -13.517,
            -71.978,
        ),
        way(10, "Plaza Mayor", &[(-13.517, -71.977), (-13.517, -71.976)]),
    ]
}

fn places(index: &SearchIndex, text: &str) -> Vec<String> {
    index
        .search(text, None, 20)
        .unwrap()
        .into_iter()
        .filter(|h| h.kind == Kind::Place)
        .map(|h| h.name)
        .collect()
}

#[test]
fn a_place_is_found_under_all_its_names() {
    let index = index("place-names", &cusco());
    for text in ["Cusco", "Cuzco", "Qusqu", "Куско"] {
        assert_eq!(places(&index, text), ["Cuzco"], "{text}");
    }
}

#[test]
fn only_language_tags_count_as_names_of_a_place() {
    let index = index("place-language-tags", &cusco());
    assert!(places(&index, "Linksname").is_empty());
}

#[test]
fn a_place_is_found_under_its_local_and_old_name() {
    let index = index(
        "place-old-names",
        &[node(
            1,
            json!({ "place": "town", "name": "Neustadt", "loc_name": "Noistadt", "old_name": "Altburg;Vorburg" }),
            48.0,
            11.0,
        )],
    );
    for text in ["Noistadt", "Altburg", "Vorburg"] {
        assert_eq!(places(&index, text), ["Neustadt"], "{text}");
    }
}

#[test]
fn a_street_is_found_with_the_official_name_of_its_town() {
    let index = index("street-official-town", &cusco());
    let hits = streets(&index, "Plaza Cusco");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].town.as_deref(), Some("Cuzco"));
}

#[test]
fn the_languages_of_a_city_stay_out_of_the_town_column() {
    let index = index("street-town-languages", &cusco());
    assert!(streets(&index, "Plaza Куско").is_empty());
    assert!(streets(&index, "Plaza Qusqu").is_empty());
}

#[test]
fn streets_and_pois_add_only_the_official_name() {
    let tags = json!({
        "tourism": "hostel",
        "name": "Casa",
        "official_name": "Hospedaje Oficial",
        "name:fr": "Maison",
        "old_name": "Vieja",
    });
    let mut street = way(11, "Calle Corta", &[(48.0, 11.0), (48.0, 11.001)]);
    street["properties"]["official_name"] = json!("Calle Larga Oficial");
    street["properties"]["name:fr"] = json!("Rue Courte");
    let index = index("poi-official", &[node(1, tags, 47.0, 11.0), street]);
    assert_eq!(index.search("Hospedaje", None, 5).unwrap().len(), 1);
    assert!(index.search("Maison", None, 5).unwrap().is_empty());
    assert!(index.search("Vieja", None, 5).unwrap().is_empty());
    assert_eq!(streets(&index, "Larga").len(), 1);
    assert!(streets(&index, "Rue").is_empty());
}

#[test]
fn names_that_differ_only_in_spelling_count_once() {
    let features = [node(
        1,
        json!({ "tourism": "hostel", "name": "Café Süd", "official_name": "CAFE SUD", "name:en": "Cafe Sud" }),
        48.0,
        11.0,
    )];
    let index = index("normalised-names", &features);
    let hits = index.search("Cafe", None, 5).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].name, "Café Süd");
}
