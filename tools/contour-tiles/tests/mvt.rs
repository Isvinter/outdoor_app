//! Vector tile encoding.
#![allow(clippy::unwrap_used)]

mod common;

use contour_tiles::encode_tile;

#[test]
fn roundtrip() {
    let first = vec![[0, 0], [100, 50], [4096, 4100]];
    let second = vec![[-20, 300], [10, 280]];
    let tile = encode_tile(&[(1200, 1, first.clone()), (1220, 0, second.clone())]);
    let layers = common::decode(&tile);
    assert_eq!(layers.len(), 1);
    let layer = &layers[0];
    assert_eq!(
        (layer.name.as_str(), layer.version, layer.extent),
        ("contours", 2, 4096)
    );
    assert_eq!(layer.features.len(), 2);
    let [a, b] = [&layer.features[0], &layer.features[1]];
    assert_eq!((a.geom_type, b.geom_type), (2, 2));
    assert_eq!(a.lines, vec![first]);
    assert_eq!(b.lines, vec![second]);
    assert_eq!((a.int("ele"), a.int("level")), (1200, 1));
    assert_eq!((b.int("ele"), b.int("level")), (1220, 0));
}

/// MVT 2.1 §4.3.3.2: a LineTo must not repeat the previous point. Repeated
/// points are dropped, and a line left with a single point is not written.
#[test]
fn repeated_points_are_dropped() {
    let line = vec![[10, 10], [10, 10], [20, 15], [20, 15], [10, 10]];
    let dot = vec![[30, 30], [30, 30]];
    let layers = common::decode(&encode_tile(&[(1200, 1, line), (1220, 0, dot)]));
    let features = &layers[0].features;
    assert_eq!(features.len(), 1);
    assert_eq!(features[0].lines, vec![vec![[10, 10], [20, 15], [10, 10]]]);
}
