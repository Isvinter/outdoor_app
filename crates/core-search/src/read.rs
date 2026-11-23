//! Reading the index, read-only.

use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::path::Path;

use rusqlite::{Connection, OpenFlags, Row, params};

use crate::{Category, Hit, Kind, SEARCH_FORMAT, SearchError, normalize};

/// How many full-text candidates are ranked by distance.
const CANDIDATES: i64 = 2000;
/// Metres per degree of latitude, rounded down so bands stay on the safe side.
const METRES_PER_LAT_DEGREE: f64 = 110_000.0;
const EARTH_RADIUS_M: f64 = 6_371_000.0;

pub struct SearchIndex {
    conn: Connection,
}

impl SearchIndex {
    pub fn open(path: &Path) -> Result<Self, SearchError> {
        let conn = Connection::open_with_flags(
            uri(path),
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
        )?;
        let found: String =
            conn.query_row("SELECT value FROM meta WHERE key = 'format'", [], |row| {
                row.get(0)
            })?;
        if found != SEARCH_FORMAT.to_string() {
            return Err(SearchError::Format { found });
        }
        Ok(Self { conn })
    }

    /// Finds entries whose name starts each word of `text`. Nearest first when
    /// `near` (`(lat, lon)`) is given, best match first otherwise.
    pub fn search(
        &self,
        text: &str,
        near: Option<(f64, f64)>,
        limit: usize,
    ) -> Result<Vec<Hit>, SearchError> {
        let query = normalize(text)
            .split_whitespace()
            .map(|token| format!("\"{token}\"*"))
            .collect::<Vec<_>>()
            .join(" ");
        if query.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let Some(near) = near else {
            let mut stmt = self.conn.prepare_cached(
                "SELECT e.kind, e.category, e.subtype, e.name, e.town, e.lat, e.lon
                 FROM (SELECT rowid FROM name_terms WHERE name_terms MATCH ?1
                       ORDER BY rank LIMIT ?2) f
                 JOIN entry e ON e.id = f.rowid",
            )?;
            // Candidates arrive in rank order.
            let rows = stmt.query_map(params![query, CANDIDATES], |row| hit(row, None))?;
            let mut hits = rows.collect::<Result<Vec<_>, _>>()?;
            hits.truncate(limit);
            return hits.into_iter().map(finish_hit).collect();
        };
        // With a position every match is a candidate: no rank ordering and no
        // cap, only the nearest `limit` are kept while the rows stream by.
        let mut stmt = self.conn.prepare_cached(
            "SELECT e.kind, e.category, e.subtype, e.name, e.town, e.lat, e.lon
             FROM name_terms f JOIN entry e ON e.id = f.rowid
             WHERE name_terms MATCH ?1",
        )?;
        let mut rows = stmt.query(params![query])?;
        let mut nearest: BinaryHeap<ByDistance> = BinaryHeap::with_capacity(limit + 1);
        while let Some(row) = rows.next()? {
            nearest.push(ByDistance(hit(row, Some(near))?));
            if nearest.len() > limit {
                nearest.pop();
            }
        }
        nearest
            .into_sorted_vec()
            .into_iter()
            .map(|h| finish_hit(h.0))
            .collect()
    }

    /// The `limit` nearest entries of a category, named or not.
    pub fn by_category(
        &self,
        category: Category,
        near: (f64, f64),
        limit: usize,
    ) -> Result<Vec<Hit>, SearchError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let mut stmt = self.conn.prepare_cached(
            "SELECT kind, category, subtype, name, town, lat, lon
             FROM entry WHERE category = ?1 AND lat BETWEEN ?2 AND ?3",
        )?;
        let mut band = 0.05_f64;
        loop {
            let rows = stmt.query_map(
                params![category.code(), near.0 - band, near.0 + band],
                |row| hit(row, Some(near)),
            )?;
            let mut hits = rows.collect::<Result<Vec<_>, _>>()?;
            hits.sort_by(|a, b| a.distance_m.total_cmp(&b.distance_m));
            hits.truncate(limit);
            // Everything within band * 110 km lies inside the latitude band, so
            // the nearest `limit` hits are final once the last of them is that close.
            let complete = hits.len() == limit
                && hits
                    .last()
                    .is_some_and(|h| h.distance_m <= band * METRES_PER_LAT_DEGREE);
            if complete || band >= 180.0 {
                return hits.into_iter().map(finish_hit).collect();
            }
            band *= 2.0;
        }
    }
}

/// A hit before its codes are checked; `distance_m` is NaN-free but optional.
struct RawHit {
    kind: i64,
    category: Option<i64>,
    subtype: Option<String>,
    name: String,
    town: Option<String>,
    lat: f64,
    lon: f64,
    distance_m: f64,
    has_distance: bool,
}

/// Orders hits by distance so a heap can drop the farthest.
struct ByDistance(RawHit);

impl PartialEq for ByDistance {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for ByDistance {}
impl PartialOrd for ByDistance {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for ByDistance {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.distance_m.total_cmp(&other.0.distance_m)
    }
}

fn hit(row: &Row, near: Option<(f64, f64)>) -> rusqlite::Result<RawHit> {
    let lat: f64 = row.get(5)?;
    let lon: f64 = row.get(6)?;
    Ok(RawHit {
        kind: row.get(0)?,
        category: row.get(1)?,
        subtype: row.get(2)?,
        name: row.get(3)?,
        town: row.get(4)?,
        lat,
        lon,
        distance_m: near.map_or(0.0, |n| haversine_m(n, (lat, lon))),
        has_distance: near.is_some(),
    })
}

fn finish_hit(raw: RawHit) -> Result<Hit, SearchError> {
    let kind = Kind::from_code(raw.kind)
        .ok_or_else(|| SearchError::Corrupt(format!("unknown kind {}", raw.kind)))?;
    let category = match raw.category {
        None => None,
        Some(code) => Some(
            Category::from_code(code)
                .ok_or_else(|| SearchError::Corrupt(format!("unknown category {code}")))?,
        ),
    };
    Ok(Hit {
        kind,
        category,
        subtype: raw.subtype,
        name: raw.name,
        town: raw.town,
        lat: raw.lat,
        lon: raw.lon,
        distance_m: raw.has_distance.then_some(raw.distance_m),
    })
}

fn haversine_m(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (lat1, lat2) = (a.0.to_radians(), b.0.to_radians());
    let dlat = lat2 - lat1;
    let dlon = (b.1 - a.1).to_radians();
    let h = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_M * h.sqrt().min(1.0).asin()
}

/// `file:` URI with the characters that are special in a URI escaped.
fn uri(path: &Path) -> String {
    let mut out = String::from("file:");
    for c in path.to_string_lossy().chars() {
        match c {
            '%' => out.push_str("%25"),
            '?' => out.push_str("%3F"),
            '#' => out.push_str("%23"),
            _ => out.push(c),
        }
    }
    out.push_str("?immutable=1");
    out
}
