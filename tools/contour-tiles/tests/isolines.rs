//! Contour lines on elevation grids.
#![allow(clippy::unwrap_used)]

use contour_tiles::{Grid, intervals, isolines, simplify};

/// A cone of `side × side` samples, `peak` metres in the centre, falling
/// linearly to 0 at a distance of `radius` samples.
fn cone(side: usize, peak: f32, radius: f32) -> Grid {
    let centre = (side - 1) as f32 / 2.0;
    let values = (0..side * side)
        .map(|i| {
            let (x, y) = ((i % side) as f32 - centre, (i / side) as f32 - centre);
            (peak * (1.0 - (x * x + y * y).sqrt() / radius)).max(0.0)
        })
        .collect();
    Grid { side, values }
}

#[test]
fn intervals_match_desktop() {
    assert_eq!(intervals(11), Some((100, 500)));
    assert_eq!(intervals(12), Some((50, 250)));
    assert_eq!(intervals(13), Some((20, 100)));
    assert_eq!(intervals(14), Some((20, 100)));
    assert_eq!(intervals(15), Some((10, 50)));
    assert_eq!(intervals(10), None);
    assert_eq!(intervals(16), None);
}

#[test]
fn cone_gives_concentric_rings() {
    // The peak stays below 1000 m so that no line sits exactly on the summit.
    let (peak, radius) = (950.0, 16.0);
    let lines = isolines(&cone(33, peak, radius), 100);
    let mut levels: Vec<i32> = lines.iter().map(|line| line.ele).collect();
    levels.sort_unstable();
    assert_eq!(levels, (1..=9).map(|k| k * 100).collect::<Vec<_>>());
    for line in &lines {
        assert_eq!(
            line.points.first(),
            line.points.last(),
            "ring {} not closed",
            line.ele
        );
        let expected = radius * (1.0 - line.ele as f32 / peak);
        for [x, y] in &line.points {
            let r = ((x - 16.0).powi(2) + (y - 16.0).powi(2)).sqrt();
            assert!(
                (r - expected).abs() <= 0.6,
                "ring {}: r {r} vs {expected}",
                line.ele
            );
        }
    }
}

#[test]
fn flat_grid_has_no_lines() {
    let grid = Grid {
        side: 10,
        values: vec![500.0; 100],
    };
    assert!(isolines(&grid, 100).is_empty());
}

#[test]
fn saddle_is_resolved_consistently() {
    // Top-left 0, top-right 10, bottom-left 10, bottom-right 0.
    let grid = Grid {
        side: 2,
        values: vec![0.0, 10.0, 10.0, 0.0],
    };
    let lines = isolines(&grid, 5);
    assert_eq!(lines.len(), 2, "{lines:?}");
    let [a, b] = [&lines[0].points, &lines[1].points];
    assert_eq!((a.len(), b.len()), (2, 2));
    assert!(
        !segments_cross(a[0], a[1], b[0], b[1]),
        "{a:?} crosses {b:?}"
    );
}

fn segments_cross(p: [f32; 2], q: [f32; 2], r: [f32; 2], s: [f32; 2]) -> bool {
    let side = |a: [f32; 2], b: [f32; 2], c: [f32; 2]| {
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    };
    side(p, q, r) * side(p, q, s) < 0.0 && side(r, s, p) * side(r, s, q) < 0.0
}

#[test]
fn nan_cells_give_nothing() {
    let mut grid = cone(33, 950.0, 16.0);
    // A hole at the sample (20, 16), on the right of the summit.
    grid.values[16 * 33 + 20] = f32::NAN;
    let lines = isolines(&grid, 100);
    for line in &lines {
        for [x, y] in &line.points {
            let inside_hole_cells = *x > 19.0 && *x < 21.0 && *y > 15.0 && *y < 17.0;
            assert!(
                !inside_hole_cells,
                "point ({x}, {y}) of {} in a cell with no data",
                line.ele
            );
        }
    }
    // Rings crossing the hole are open now; the others stay closed.
    assert!(
        lines
            .iter()
            .any(|line| line.points.first() != line.points.last())
    );
    assert!(
        lines
            .iter()
            .any(|line| line.points.first() == line.points.last())
    );
}

#[test]
fn simplify_keeps_ends_and_drops_collinear() {
    assert_eq!(
        simplify(&[[0.0, 0.0], [1.0, 0.001], [2.0, 0.0]], 0.1),
        vec![[0.0, 0.0], [2.0, 0.0]]
    );
    let corner = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]];
    assert_eq!(simplify(&corner, 0.1), corner.to_vec());
}
