//! Reading the index, read-only.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashSet};
use std::path::Path;

use rusqlite::{Connection, OpenFlags, Row, params};

use crate::{Category, Hit, Kind, SEARCH_FORMAT, SearchError, query_words};

/// How many full-text candidates are ranked by distance.
const CANDIDATES: i64 = 2000;
/// Metres per degree of latitude, rounded down so bands stay on the safe side.
const METRES_PER_LAT_DEGREE: f64 = 110_000.0;
const EARTH_RADIUS_M: f64 = 6_371_000.0;
/// [`Hit::match_rank`] of everything but a place matching in whole words.
const OTHER_RANK: u8 = 5;

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
        let words = query_words(text);
        // Every word must match the name or the town, and at least one the
        // name, so a town alone does not list all its streets.
        let query = if words.is_empty() {
            String::new()
        } else {
            let each = words
                .iter()
                .map(|w| format!("{{name town}} : ({})", prefix_terms(w)))
                .collect::<Vec<_>>()
                .join(" AND ");
            let in_name = words
                .iter()
                .map(|w| format!("name : ({})", prefix_terms(w)))
                .collect::<Vec<_>>()
                .join(" OR ");
            format!("({each}) AND ({in_name})")
        };
        if query.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let whole_word_places = self.whole_word_places(&words)?;
        let ranked = |row: &Row, near| -> rusqlite::Result<RawHit> {
            let mut raw = hit(row, near)?;
            let id: i64 = row.get(7)?;
            if whole_word_places.contains(&id) {
                raw.match_rank = place_rank(raw.subtype.as_deref());
            }
            Ok(raw)
        };
        let Some(near) = near else {
            // Places first: their many names in other languages would otherwise
            // rank them below anything that merely has the word in its name.
            let mut stmt = self.conn.prepare_cached(
                "SELECT e.kind, e.category, e.subtype, e.name, e.town, e.lat, e.lon, e.id
                 FROM (SELECT rowid, rank AS score FROM name_terms WHERE name_terms MATCH ?1
                       ORDER BY rank LIMIT ?2) f
                 JOIN entry e ON e.id = f.rowid
                 ORDER BY e.kind = 0 DESC, f.score, e.id",
            )?;
            let rows = stmt.query_map(params![query, CANDIDATES], |row| ranked(row, None))?;
            let mut hits = rows.collect::<Result<Vec<_>, _>>()?;
            hits.sort_by_key(|h| h.match_rank);
            hits.truncate(limit);
            return hits.into_iter().map(finish_hit).collect();
        };
        // With a position every match is a candidate: no rank ordering and no
        // cap, only the best `limit` are kept while the rows stream by.
        let mut stmt = self.conn.prepare_cached(
            "SELECT e.kind, e.category, e.subtype, e.name, e.town, e.lat, e.lon, e.id
             FROM name_terms f JOIN entry e ON e.id = f.rowid
             WHERE name_terms MATCH ?1",
        )?;
        let mut rows = stmt.query(params![query])?;
        let mut nearest: BinaryHeap<ByDistance> = BinaryHeap::with_capacity(limit + 1);
        while let Some(row) = rows.next()? {
            nearest.push(ByDistance(ranked(row, Some(near))?));
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

    /// The places that have every query word as a whole word.
    fn whole_word_places(&self, words: &[Vec<String>]) -> Result<HashSet<i64>, SearchError> {
        if words.is_empty() {
            return Ok(HashSet::new());
        }
        let query = words
            .iter()
            .map(|w| {
                let any = w
                    .iter()
                    .map(|v| format!("\"{v}\""))
                    .collect::<Vec<_>>()
                    .join(" OR ");
                format!("name : ({any})")
            })
            .collect::<Vec<_>>()
            .join(" AND ");
        let mut stmt = self.conn.prepare_cached(
            "SELECT e.id FROM name_terms f JOIN entry e ON e.id = f.rowid
             WHERE name_terms MATCH ?1 AND e.kind = 0",
        )?;
        let rows = stmt.query_map(params![query], |row| row.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The `limit` nearest entries of a category, named or not.
    pub fn by_category(
        &self,
        category: Category,
        near: (f64, f64),
        limit: usize,
    ) -> Result<Vec<Hit>, SearchError> {
        self.nearest_in_category(category, near, None, limit)
    }

    /// The `limit` nearest entries of a category no more than `max_m` away.
    pub fn by_category_within(
        &self,
        category: Category,
        near: (f64, f64),
        max_m: f64,
        limit: usize,
    ) -> Result<Vec<Hit>, SearchError> {
        self.nearest_in_category(category, near, Some(max_m), limit)
    }

    fn nearest_in_category(
        &self,
        category: Category,
        near: (f64, f64),
        max_m: Option<f64>,
        limit: usize,
    ) -> Result<Vec<Hit>, SearchError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let mut stmt = self.conn.prepare_cached(
            "SELECT kind, category, subtype, name, town, lat, lon
             FROM entry WHERE category = ?1 AND lat BETWEEN ?2 AND ?3",
        )?;
        let widest_band = max_m.map_or(180.0, |m| m / METRES_PER_LAT_DEGREE);
        let mut band = 0.05_f64.min(widest_band);
        loop {
            let rows = stmt.query_map(
                params![category.code(), near.0 - band, near.0 + band],
                |row| hit(row, Some(near)),
            )?;
            let mut hits = rows.collect::<Result<Vec<_>, _>>()?;
            if let Some(max_m) = max_m {
                hits.retain(|h| h.distance_m <= max_m);
            }
            hits.sort_by(|a, b| a.distance_m.total_cmp(&b.distance_m));
            hits.truncate(limit);
            // Everything within band * 110 km lies inside the latitude band, so
            // the nearest `limit` hits are final once the last of them is that close.
            let complete = hits.len() == limit
                && hits
                    .last()
                    .is_some_and(|h| h.distance_m <= band * METRES_PER_LAT_DEGREE);
            if complete || band >= widest_band {
                return hits.into_iter().map(finish_hit).collect();
            }
            band = (band * 2.0).min(widest_band);
        }
    }
}

impl Hit {
    /// A place that has every query word as a whole word.
    #[must_use]
    pub fn is_whole_word_place(&self) -> bool {
        self.match_rank < OTHER_RANK
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
    match_rank: u8,
}

/// Orders hits by rank, then distance, so a heap can drop the last one.
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
        self.0
            .match_rank
            .cmp(&other.0.match_rank)
            .then(self.0.distance_m.total_cmp(&other.0.distance_m))
    }
}

/// All spellings of a word as a prefix. The short form `str` of `strasse` must
/// match exactly, or it would also find every word starting with "str".
fn prefix_terms(variants: &[String]) -> String {
    variants
        .iter()
        .enumerate()
        .map(|(i, v)| {
            if i > 0 && v == "str" {
                format!("\"{v}\"")
            } else {
                format!("\"{v}\"*")
            }
        })
        .collect::<Vec<_>>()
        .join(" OR ")
}

fn place_rank(subtype: Option<&str>) -> u8 {
    match subtype {
        Some("city") => 0,
        Some("town") => 1,
        Some("village") => 2,
        Some("part") => 3,
        _ => 4,
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
        match_rank: OTHER_RANK,
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
        match_rank: raw.match_rank,
    })
}

/// Metres between two `(lat, lon)` points on the earth.
#[must_use]
pub fn haversine_m(a: (f64, f64), b: (f64, f64)) -> f64 {
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
