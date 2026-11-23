//! Writing the index at build time.

use std::path::Path;

use rusqlite::{Connection, params};

use crate::{Entry, SEARCH_FORMAT, SearchError, index_terms};

pub struct SearchWriter {
    conn: Connection,
    next_id: i64,
}

impl SearchWriter {
    /// Creates a new index file. Fails if the file already holds an index.
    pub fn create(path: &Path, osm_timestamp: Option<&str>) -> Result<Self, SearchError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = MEMORY;
             PRAGMA synchronous = OFF;
             CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE entry (
                 id INTEGER PRIMARY KEY,
                 kind INTEGER NOT NULL,
                 category INTEGER,
                 subtype TEXT,
                 name TEXT NOT NULL,
                 town TEXT,
                 lat REAL NOT NULL,
                 lon REAL NOT NULL
             );
             CREATE VIRTUAL TABLE name_terms USING fts5(
                 name, town, content='', tokenize='unicode61 remove_diacritics 0'
             );
             BEGIN;",
        )?;
        conn.execute(
            "INSERT INTO meta (key, value) VALUES ('format', ?1)",
            [SEARCH_FORMAT.to_string()],
        )?;
        if let Some(ts) = osm_timestamp {
            conn.execute(
                "INSERT INTO meta (key, value) VALUES ('osm_timestamp', ?1)",
                [ts],
            )?;
        }
        Ok(Self { conn, next_id: 1 })
    }

    pub fn add(&mut self, entry: &Entry) -> Result<(), SearchError> {
        let id = self.next_id;
        self.conn
            .prepare_cached(
                "INSERT INTO entry (id, kind, category, subtype, name, town, lat, lon)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?
            .execute(params![
                id,
                entry.kind.code(),
                entry.category.map(|c| c.code()),
                entry.subtype,
                entry.name,
                entry.town,
                entry.lat,
                entry.lon,
            ])?;

        let mut terms: Vec<String> = Vec::new();
        for name in std::iter::once(&entry.name).chain(&entry.alt_names) {
            for term in index_terms(name) {
                if !terms.contains(&term) {
                    terms.push(term);
                }
            }
        }
        // An entry without a name is found by category only; its town alone
        // never makes it a match.
        if !terms.is_empty() {
            let town_terms = entry
                .town
                .as_deref()
                .map(index_terms)
                .unwrap_or_default()
                .join(" ");
            self.conn
                .prepare_cached("INSERT INTO name_terms (rowid, name, town) VALUES (?1, ?2, ?3)")?
                .execute(params![id, terms.join(" "), town_terms])?;
        }
        self.next_id += 1;
        Ok(())
    }

    /// Commits, compacts the full-text index and the file.
    pub fn finish(self) -> Result<(), SearchError> {
        let conn = self.conn;
        conn.execute_batch(
            "COMMIT;
             CREATE INDEX entry_category_lat ON entry (category, lat);
             INSERT INTO name_terms (name_terms) VALUES ('optimize');
             VACUUM;",
        )?;
        Ok(())
    }
}
