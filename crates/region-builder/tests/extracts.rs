#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use region_builder::{BuildError, GeofabrikIndex};

fn index() -> GeofabrikIndex {
    GeofabrikIndex::parse(include_str!("fixtures/index.json")).unwrap()
}

fn ids(bbox: [f64; 4]) -> Vec<String> {
    let mut ids: Vec<String> = index()
        .select(bbox)
        .unwrap()
        .into_iter()
        .map(|extract| extract.id)
        .collect();
    ids.sort();
    ids
}

#[test]
fn area_inside_one_extract_takes_the_smallest() {
    assert_eq!(ids([10.5, 46.5, 11.5, 47.5]), ["a"]);
}

#[test]
fn area_across_a_border_takes_both_small_extracts() {
    assert_eq!(ids([11.5, 46.5, 12.5, 47.5]), ["a", "b"]);
}

#[test]
fn large_extract_fills_what_the_small_ones_leave() {
    assert_eq!(ids([9.5, 46.5, 10.5, 47.5]), ["a", "big"]);
}

#[test]
fn area_outside_all_extracts_is_an_error() {
    let result = index().select([30.0, 46.0, 31.0, 47.0]);
    assert!(matches!(result, Err(BuildError::NoExtract)), "{result:?}");
}

#[test]
fn parts_outside_all_extracts_do_not_matter() {
    assert_eq!(ids([19.5, 44.0, 20.5, 45.0]), ["big"]);
}

#[test]
fn holes_do_not_cover() {
    let result = index().select([24.6, 44.6, 25.4, 45.4]);
    assert!(matches!(result, Err(BuildError::NoExtract)), "{result:?}");
    assert_eq!(ids([24.1, 44.1, 24.4, 44.4]), ["ring"]);
}

#[test]
fn extracts_carry_their_download_address() {
    let extracts = index().select([10.5, 46.5, 11.5, 47.5]).unwrap();
    assert_eq!(extracts[0].url, "https://example.org/a-latest.osm.pbf");
    // 2° × 2° at 47° north.
    assert!(
        (30_000.0..36_000.0).contains(&extracts[0].area_km2),
        "{}",
        extracts[0].area_km2
    );
}

#[test]
fn broken_index_is_reported() {
    assert!(matches!(
        GeofabrikIndex::parse("{"),
        Err(BuildError::Index(_))
    ));
    assert!(matches!(
        GeofabrikIndex::parse("{}"),
        Err(BuildError::Index(_))
    ));
}
