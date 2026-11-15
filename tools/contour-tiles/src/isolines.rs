//! Marching squares on an elevation grid, with the segments joined into lines.

use std::collections::HashMap;

/// Elevation samples on a square grid, row-major, `side × side`; NaN marks
/// samples without data.
#[derive(Debug, Clone, PartialEq)]
pub struct Grid {
    pub side: usize,
    pub values: Vec<f32>,
}

/// One contour line in grid coordinates (x = column, y = row). A closed ring
/// ends on its first point.
#[derive(Debug, Clone, PartialEq)]
pub struct Isoline {
    pub ele: i32,
    pub points: Vec<[f32; 2]>,
}

/// A cell edge: horizontal edges from sample (i, j) to (i + 1, j), vertical
/// ones from (i, j) to (i, j + 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Edge {
    H(usize, usize),
    V(usize, usize),
}

/// All contour lines at multiples of `interval` metres. A sample on a level
/// counts as above it.
pub fn isolines(grid: &Grid, interval: u32) -> Vec<Isoline> {
    let side = grid.side;
    if side < 2 || interval == 0 {
        return Vec::new();
    }
    let known = grid.values.iter().copied().filter(|v| !v.is_nan());
    let (min, max) = known.fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), v| {
        (lo.min(v), hi.max(v))
    });
    if min > max {
        return Vec::new();
    }
    let step = interval as f32;
    let first = (min / step).floor() as i64 + 1;
    let last = (max / step).floor() as i64;
    (first..=last)
        .flat_map(|k| {
            let ele = (k * i64::from(interval)) as i32;
            level(grid, ele)
        })
        .collect()
}

fn level(grid: &Grid, ele: i32) -> Vec<Isoline> {
    let side = grid.side;
    let at = |i: usize, j: usize| grid.values[j * side + i];
    let threshold = ele as f32;
    let point = |edge: Edge| -> [f32; 2] {
        let ((i0, j0), (i1, j1)) = match edge {
            Edge::H(i, j) => ((i, j), (i + 1, j)),
            Edge::V(i, j) => ((i, j), (i, j + 1)),
        };
        let (v0, v1) = (at(i0, j0), at(i1, j1));
        let t = ((threshold - v0) / (v1 - v0)).clamp(0.0, 1.0);
        [
            i0 as f32 + t * (i1 - i0) as f32,
            j0 as f32 + t * (j1 - j0) as f32,
        ]
    };

    let mut segments: Vec<[Edge; 2]> = Vec::new();
    for j in 0..side - 1 {
        for i in 0..side - 1 {
            let corners = [at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)];
            if corners.iter().any(|v| v.is_nan()) {
                continue;
            }
            let [a, b, c, d] = corners.map(|v| v >= threshold);
            let (top, right, bottom, left) = (
                Edge::H(i, j),
                Edge::V(i + 1, j),
                Edge::H(i, j + 1),
                Edge::V(i, j),
            );
            let centre = corners.iter().sum::<f32>() / 4.0 >= threshold;
            let case = (a as u8) << 3 | (b as u8) << 2 | (c as u8) << 1 | d as u8;
            match case {
                0 | 15 => {}
                // A single corner differs from the other three.
                0b1000 | 0b0111 => segments.push([top, left]),
                0b0100 | 0b1011 => segments.push([top, right]),
                0b0010 | 0b1101 => segments.push([right, bottom]),
                0b0001 | 0b1110 => segments.push([bottom, left]),
                // Two neighbouring corners differ from the other two.
                0b1100 | 0b0011 => segments.push([left, right]),
                0b1001 | 0b0110 => segments.push([top, bottom]),
                // Saddles: the centre decides which corners are connected.
                0b1010 if centre => segments.extend([[top, right], [bottom, left]]),
                0b1010 => segments.extend([[top, left], [right, bottom]]),
                0b0101 if centre => segments.extend([[top, left], [right, bottom]]),
                _ => segments.extend([[top, right], [bottom, left]]),
            }
        }
    }
    join(&segments)
        .into_iter()
        .map(|edges| Isoline {
            ele,
            points: edges.into_iter().map(point).collect(),
        })
        // Samples exactly on the level (a summit, a plateau) give lines of no length.
        .filter(|line| line.points.iter().any(|p| *p != line.points[0]))
        .collect()
}

/// Chains the segments at their shared edges. A closed chain repeats its
/// first edge at the end.
fn join(segments: &[[Edge; 2]]) -> Vec<Vec<Edge>> {
    let mut at: HashMap<Edge, Vec<usize>> = HashMap::new();
    for (index, segment) in segments.iter().enumerate() {
        for edge in segment {
            at.entry(*edge).or_default().push(index);
        }
    }
    let mut used = vec![false; segments.len()];
    // The other segment at `edge`, if it is not used yet.
    let next = |edge: Edge, used: &[bool]| at[&edge].iter().copied().find(|&s| !used[s]);
    let mut lines = Vec::new();
    // Open lines first, from their loose ends, then the rings that remain.
    let starts: Vec<usize> = (0..segments.len())
        .filter(|&s| segments[s].iter().any(|edge| at[edge].len() == 1))
        .chain(0..segments.len())
        .collect();
    for start in starts {
        if used[start] {
            continue;
        }
        let [first, second] = segments[start];
        // Begin at the loose end of an open line.
        let (head, mut tail) = if at[&first].len() == 1 {
            (first, second)
        } else {
            (second, first)
        };
        used[start] = true;
        let mut line = vec![head, tail];
        while let Some(segment) = next(tail, &used) {
            used[segment] = true;
            let [a, b] = segments[segment];
            tail = if a == tail { b } else { a };
            line.push(tail);
        }
        lines.push(line);
    }
    lines
}

/// Douglas–Peucker: drops points closer than `tolerance` to the line through
/// their neighbours that remain. The ends are kept.
pub fn simplify(points: &[[f32; 2]], tolerance: f32) -> Vec<[f32; 2]> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut stack = vec![(0, points.len() - 1)];
    while let Some((from, to)) = stack.pop() {
        let (a, b) = (points[from], points[to]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let length = (dx * dx + dy * dy).sqrt();
        let distance = |p: [f32; 2]| {
            if length == 0.0 {
                ((p[0] - a[0]).powi(2) + (p[1] - a[1]).powi(2)).sqrt()
            } else {
                ((p[0] - a[0]) * dy - (p[1] - a[1]) * dx).abs() / length
            }
        };
        let farthest = (from + 1..to)
            .map(|i| (i, distance(points[i])))
            .max_by(|x, y| x.1.total_cmp(&y.1));
        if let Some((index, d)) = farthest
            && d > tolerance
        {
            keep[index] = true;
            stack.extend([(from, index), (index, to)]);
        }
    }
    points
        .iter()
        .zip(keep)
        .filter(|(_, k)| *k)
        .map(|(p, _)| *p)
        .collect()
}
