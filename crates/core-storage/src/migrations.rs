//! Schema migrations, tracked in SQLite's `user_version`.
//!
//! Each entry migrates from version `index` to `index + 1` and runs in its own
//! transaction. Released entries must never be changed, only appended to.

use rusqlite::Connection;

use crate::StorageError;

pub(crate) const MIGRATIONS: &[&str] = &[
    // 1: tracks, segments, points, settings
    "
    CREATE TABLE tracks (
        id          BLOB PRIMARY KEY NOT NULL,
        name        TEXT,
        activity    TEXT NOT NULL,
        state       TEXT NOT NULL,
        ended_at    INTEGER,
        stats_json  TEXT,
        created_at  INTEGER NOT NULL,
        updated_at  INTEGER NOT NULL,
        deleted_at  INTEGER,
        rev         INTEGER NOT NULL DEFAULT 1,
        synced_rev  INTEGER NOT NULL DEFAULT 0
    ) STRICT;

    CREATE TABLE track_segments (
        id          BLOB PRIMARY KEY NOT NULL,
        track_id    BLOB NOT NULL REFERENCES tracks (id),
        seq         INTEGER NOT NULL,
        created_at  INTEGER NOT NULL,
        UNIQUE (track_id, seq)
    ) STRICT;

    -- Points are immutable and belong to exactly one segment; they carry no
    -- sync metadata of their own (a track is synchronised as a whole).
    CREATE TABLE track_points (
        segment_id    BLOB NOT NULL REFERENCES track_segments (id),
        seq           INTEGER NOT NULL,
        time_ms       INTEGER,
        lat           REAL NOT NULL,
        lon           REAL NOT NULL,
        ele           REAL,
        h_accuracy_m  REAL,
        speed_mps     REAL,
        PRIMARY KEY (segment_id, seq)
    ) STRICT, WITHOUT ROWID;

    CREATE TABLE settings (
        key         TEXT PRIMARY KEY NOT NULL,
        value       TEXT NOT NULL,
        updated_at  INTEGER NOT NULL
    ) STRICT, WITHOUT ROWID;
    ",
    // 2: planned routes
    "
    CREATE TABLE routes (
        id          BLOB PRIMARY KEY NOT NULL,
        name        TEXT NOT NULL,
        profile     TEXT NOT NULL,
        distance_m  REAL NOT NULL,
        ascent_m    REAL,
        -- The complete plan (waypoints, line, elevation profile, times) as JSON.
        plan_json   TEXT NOT NULL,
        created_at  INTEGER NOT NULL,
        updated_at  INTEGER NOT NULL,
        deleted_at  INTEGER,
        rev         INTEGER NOT NULL DEFAULT 1,
        synced_rev  INTEGER NOT NULL DEFAULT 0
    ) STRICT;
    ",
];

pub(crate) fn schema_version(conn: &Connection) -> Result<u32, StorageError> {
    Ok(conn.query_row("PRAGMA user_version", [], |row| row.get(0))?)
}

pub(crate) fn migrate(conn: &mut Connection) -> Result<(), StorageError> {
    let supported = MIGRATIONS.len() as u32;
    let found = schema_version(conn)?;
    if found > supported {
        return Err(StorageError::SchemaTooNew { found, supported });
    }
    for (index, sql) in MIGRATIONS.iter().enumerate().skip(found as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", index as u32 + 1)?;
        tx.commit()?;
    }
    Ok(())
}
