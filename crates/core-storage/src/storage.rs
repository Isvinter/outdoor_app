use std::path::Path;
use std::str::FromStr;

use core_track::{Activity, Segment, Track, TrackPoint, TrackStats};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::StorageError;
use crate::migrations::{MIGRATIONS, migrate, schema_version};

/// Life cycle of a track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackState {
    Recording,
    Paused,
    Finished,
}

impl TrackState {
    fn as_str(self) -> &'static str {
        match self {
            TrackState::Recording => "recording",
            TrackState::Paused => "paused",
            TrackState::Finished => "finished",
        }
    }

    fn parse(s: &str) -> Result<Self, StorageError> {
        match s {
            "recording" => Ok(TrackState::Recording),
            "paused" => Ok(TrackState::Paused),
            "finished" => Ok(TrackState::Finished),
            other => Err(StorageError::Corrupt(format!(
                "unknown track state `{other}`"
            ))),
        }
    }
}

/// A track without its points.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackRecord {
    pub id: Uuid,
    pub name: Option<String>,
    pub activity: Activity,
    pub state: TrackState,
    /// Time of the first point.
    pub started_at_ms: Option<i64>,
    /// Time the recording was finished, or of the last point of an imported track.
    pub ended_at_ms: Option<i64>,
    /// Statistics cached when the track was finished or imported.
    pub stats: Option<TrackStats>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    /// Incremented on every change; basis for synchronisation.
    pub rev: i64,
}

/// Handle to the application database.
pub struct Storage {
    conn: Connection,
}

const TRACK_COLUMNS: &str = "
    t.id, t.name, t.activity, t.state, t.ended_at, t.stats_json, t.created_at, t.updated_at, t.rev,
    (SELECT MIN(p.time_ms) FROM track_points p
       JOIN track_segments s ON s.id = p.segment_id WHERE s.track_id = t.id)";

impl Storage {
    /// Schema version this build reads and writes.
    pub const CURRENT_SCHEMA_VERSION: u32 = MIGRATIONS.len() as u32;

    /// Opens (and if necessary creates or migrates) the database file.
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        Self::init(Connection::open(path)?)
    }

    /// Opens a private in-memory database, for tests.
    pub fn open_in_memory() -> Result<Self, StorageError> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Self, StorageError> {
        // WAL keeps the database consistent if the process dies mid-write; with
        // synchronous=NORMAL at most the last few points before a power loss are lost.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", true)?;
        migrate(&mut conn)?;
        Ok(Self { conn })
    }

    pub fn schema_version(&self) -> Result<u32, StorageError> {
        schema_version(&self.conn)
    }

    // --- tracks -----------------------------------------------------------

    pub fn create_track(
        &mut self,
        name: Option<&str>,
        activity: Activity,
        state: TrackState,
        now_ms: i64,
    ) -> Result<Uuid, StorageError> {
        let id = Uuid::now_v7();
        let ended_at = (state == TrackState::Finished).then_some(now_ms);
        self.conn.execute(
            "INSERT INTO tracks (id, name, activity, state, ended_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            params![
                id,
                name,
                activity.as_str(),
                state.as_str(),
                ended_at,
                now_ms
            ],
        )?;
        Ok(id)
    }

    /// Stores a complete track, e.g. from a GPX import, as finished.
    pub fn import_track(
        &mut self,
        track: &Track,
        activity: Activity,
        stats: &TrackStats,
        now_ms: i64,
    ) -> Result<Uuid, StorageError> {
        let id = Uuid::now_v7();
        let ended_at = track
            .segments
            .iter()
            .flat_map(|s| &s.points)
            .filter_map(|p| p.time_ms)
            .max();
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO tracks (id, name, activity, state, ended_at, stats_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
            params![
                id,
                track.name,
                activity.as_str(),
                TrackState::Finished.as_str(),
                ended_at,
                stats_to_json(stats)?,
                now_ms
            ],
        )?;
        for (seq, segment) in track.segments.iter().enumerate() {
            let segment_id = Uuid::now_v7();
            tx.execute(
                "INSERT INTO track_segments (id, track_id, seq, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![segment_id, id, seq as i64, now_ms],
            )?;
            insert_points(&tx, segment_id, 0, &segment.points)?;
        }
        tx.commit()?;
        Ok(id)
    }

    /// Starts a new segment of a track; points are appended to segments.
    pub fn begin_segment(&mut self, track: Uuid, now_ms: i64) -> Result<Uuid, StorageError> {
        self.require_track(track)?;
        let id = Uuid::now_v7();
        self.conn.execute(
            "INSERT INTO track_segments (id, track_id, seq, created_at)
             SELECT ?1, ?2, COALESCE(MAX(seq), -1) + 1, ?3 FROM track_segments WHERE track_id = ?2",
            params![id, track, now_ms],
        )?;
        Ok(id)
    }

    /// Appends one point. Durable as soon as the call returns.
    pub fn append_point(&mut self, segment: Uuid, point: &TrackPoint) -> Result<(), StorageError> {
        self.append_points(segment, std::slice::from_ref(point))
    }

    /// Appends several points atomically.
    pub fn append_points(
        &mut self,
        segment: Uuid,
        points: &[TrackPoint],
    ) -> Result<(), StorageError> {
        let tx = self.conn.transaction()?;
        let next_seq: Option<i64> = tx
            .query_row(
                "SELECT (SELECT COALESCE(MAX(seq), -1) + 1 FROM track_points WHERE segment_id = s.id)
                 FROM track_segments s WHERE s.id = ?1",
                [segment],
                |row| row.get(0),
            )
            .optional()?;
        let next_seq = next_seq.ok_or(StorageError::NotFound(segment))?;
        insert_points(&tx, segment, next_seq, points)?;
        tx.commit()?;
        Ok(())
    }

    pub fn set_track_state(
        &mut self,
        track: Uuid,
        state: TrackState,
        now_ms: i64,
    ) -> Result<(), StorageError> {
        let ended_at = (state == TrackState::Finished).then_some(now_ms);
        self.update_track(
            track,
            "UPDATE tracks SET state = ?2, ended_at = ?3, updated_at = ?4, rev = rev + 1
             WHERE id = ?1 AND deleted_at IS NULL",
            params![track, state.as_str(), ended_at, now_ms],
        )
    }

    pub fn set_track_stats(
        &mut self,
        track: Uuid,
        stats: &TrackStats,
        now_ms: i64,
    ) -> Result<(), StorageError> {
        self.update_track(
            track,
            "UPDATE tracks SET stats_json = ?2, updated_at = ?3, rev = rev + 1
             WHERE id = ?1 AND deleted_at IS NULL",
            params![track, stats_to_json(stats)?, now_ms],
        )
    }

    /// Sets the activity of a track and the statistics computed for it.
    pub fn set_track_activity(
        &mut self,
        track: Uuid,
        activity: Activity,
        stats: &TrackStats,
        now_ms: i64,
    ) -> Result<(), StorageError> {
        self.update_track(
            track,
            "UPDATE tracks SET activity = ?2, stats_json = ?3, updated_at = ?4, rev = rev + 1
             WHERE id = ?1 AND deleted_at IS NULL",
            params![track, activity.as_str(), stats_to_json(stats)?, now_ms],
        )
    }

    pub fn rename_track(
        &mut self,
        track: Uuid,
        name: &str,
        now_ms: i64,
    ) -> Result<(), StorageError> {
        self.update_track(
            track,
            "UPDATE tracks SET name = ?2, updated_at = ?3, rev = rev + 1
             WHERE id = ?1 AND deleted_at IS NULL",
            params![track, name, now_ms],
        )
    }

    /// Marks a track as deleted. The row stays as a tombstone for synchronisation.
    pub fn delete_track(&mut self, track: Uuid, now_ms: i64) -> Result<(), StorageError> {
        self.update_track(
            track,
            "UPDATE tracks SET deleted_at = ?2, updated_at = ?2, rev = rev + 1
             WHERE id = ?1 AND deleted_at IS NULL",
            params![track, now_ms],
        )
    }

    /// Ids and deletion times of deleted tracks.
    pub fn deleted_tracks(&self) -> Result<Vec<(Uuid, i64)>, StorageError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, deleted_at FROM tracks WHERE deleted_at IS NOT NULL ORDER BY deleted_at",
        )?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn track(&self, id: Uuid) -> Result<TrackRecord, StorageError> {
        let sql = format!(
            "SELECT {TRACK_COLUMNS} FROM tracks t WHERE t.id = ?1 AND t.deleted_at IS NULL"
        );
        let raw = self.conn.query_row(&sql, [id], RawTrack::read).optional()?;
        raw.ok_or(StorageError::NotFound(id))?.into_record()
    }

    /// All tracks that are not deleted, newest first.
    pub fn list_tracks(&self) -> Result<Vec<TrackRecord>, StorageError> {
        let sql = format!(
            "SELECT {TRACK_COLUMNS} FROM tracks t WHERE t.deleted_at IS NULL AND t.state = 'finished'
             ORDER BY t.created_at DESC, t.id DESC"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], RawTrack::read)?;
        rows.map(|raw| raw?.into_record()).collect()
    }

    /// The track that was being recorded when the app last stopped, if any.
    pub fn open_recording(&self) -> Result<Option<TrackRecord>, StorageError> {
        let sql = format!(
            "SELECT {TRACK_COLUMNS} FROM tracks t
             WHERE t.deleted_at IS NULL AND t.state <> 'finished'
             ORDER BY t.created_at DESC LIMIT 1"
        );
        let raw = self.conn.query_row(&sql, [], RawTrack::read).optional()?;
        raw.map(RawTrack::into_record).transpose()
    }

    /// Loads a track with all its points.
    pub fn load_track(&self, id: Uuid) -> Result<Track, StorageError> {
        let record = self.track(id)?;
        let mut segments = Vec::new();
        let mut segment_stmt = self
            .conn
            .prepare("SELECT id FROM track_segments WHERE track_id = ?1 ORDER BY seq")?;
        let mut point_stmt = self.conn.prepare(
            "SELECT lat, lon, ele, time_ms, h_accuracy_m, speed_mps
             FROM track_points WHERE segment_id = ?1 ORDER BY seq",
        )?;
        let segment_ids: Vec<Uuid> = segment_stmt
            .query_map([id], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        for segment_id in segment_ids {
            let points = point_stmt
                .query_map([segment_id], |row| {
                    Ok(TrackPoint {
                        lat: row.get(0)?,
                        lon: row.get(1)?,
                        ele: row.get(2)?,
                        time_ms: row.get(3)?,
                        h_accuracy_m: row.get::<_, Option<f64>>(4)?.map(|v| v as f32),
                        speed_mps: row.get::<_, Option<f64>>(5)?.map(|v| v as f32),
                    })
                })?
                .collect::<Result<_, _>>()?;
            segments.push(Segment { points });
        }
        Ok(Track {
            name: record.name,
            activity: Some(record.activity),
            segments,
        })
    }

    fn require_track(&self, id: Uuid) -> Result<(), StorageError> {
        let found: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM tracks WHERE id = ?1 AND deleted_at IS NULL",
                [id],
                |row| row.get(0),
            )
            .optional()?;
        found.map(|_| ()).ok_or(StorageError::NotFound(id))
    }

    fn update_track(
        &mut self,
        id: Uuid,
        sql: &str,
        params: impl rusqlite::Params,
    ) -> Result<(), StorageError> {
        match self.conn.execute(sql, params)? {
            0 => Err(StorageError::NotFound(id)),
            _ => Ok(()),
        }
    }

    // --- routes ----------------------------------------------------------

    /// Stores a planned route and returns its id.
    pub fn save_route(
        &mut self,
        name: &str,
        profile: &str,
        distance_m: f64,
        ascent_m: Option<f64>,
        plan_json: &str,
        now_ms: i64,
    ) -> Result<Uuid, StorageError> {
        let id = Uuid::now_v7();
        self.conn.execute(
            "INSERT INTO routes (id, name, profile, distance_m, ascent_m, plan_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
            params![id, name, profile, distance_m, ascent_m, plan_json, now_ms],
        )?;
        Ok(id)
    }

    /// Puts a new plan into a saved route, keeping its id.
    #[allow(clippy::too_many_arguments)]
    pub fn replace_route(
        &mut self,
        id: Uuid,
        name: &str,
        profile: &str,
        distance_m: f64,
        ascent_m: Option<f64>,
        plan_json: &str,
        now_ms: i64,
    ) -> Result<(), StorageError> {
        self.update_route(
            id,
            "UPDATE routes SET name = ?2, profile = ?3, distance_m = ?4, ascent_m = ?5, plan_json = ?6,
                 updated_at = ?7, rev = rev + 1
             WHERE id = ?1 AND deleted_at IS NULL",
            params![id, name, profile, distance_m, ascent_m, plan_json, now_ms],
        )
    }

    /// The saved routes, newest first.
    pub fn list_routes(&self) -> Result<Vec<RouteRecord>, StorageError> {
        let mut statement = self.conn.prepare(
            "SELECT id, name, profile, distance_m, ascent_m, created_at, updated_at, rev FROM routes
             WHERE deleted_at IS NULL ORDER BY created_at DESC, id DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(RouteRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                profile: row.get(2)?,
                distance_m: row.get(3)?,
                ascent_m: row.get(4)?,
                created_at_ms: row.get(5)?,
                updated_at_ms: row.get(6)?,
                rev: row.get(7)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The plan of a saved route as it was stored.
    pub fn route_plan(&self, id: Uuid) -> Result<String, StorageError> {
        self.conn
            .query_row(
                "SELECT plan_json FROM routes WHERE id = ?1 AND deleted_at IS NULL",
                [id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(StorageError::NotFound(id))
    }

    pub fn rename_route(&mut self, id: Uuid, name: &str, now_ms: i64) -> Result<(), StorageError> {
        self.update_route(
            id,
            "UPDATE routes SET name = ?2, updated_at = ?3, rev = rev + 1 WHERE id = ?1 AND deleted_at IS NULL",
            params![id, name, now_ms],
        )
    }

    /// Marks a route as deleted. The row stays as a tombstone for synchronisation.
    pub fn delete_route(&mut self, id: Uuid, now_ms: i64) -> Result<(), StorageError> {
        self.update_route(
            id,
            "UPDATE routes SET deleted_at = ?2, updated_at = ?2, rev = rev + 1 WHERE id = ?1 AND deleted_at IS NULL",
            params![id, now_ms],
        )
    }

    /// The synchronisation state of a route, deleted ones included.
    pub fn route_sync(&self, id: Uuid) -> Result<Option<RouteSync>, StorageError> {
        Ok(self
            .conn
            .query_row(
                "SELECT rev, synced_rev, source_rev, deleted_at IS NOT NULL FROM routes WHERE id = ?1",
                [id],
                |row| {
                    Ok(RouteSync {
                        rev: row.get(0)?,
                        synced_rev: row.get(1)?,
                        source_rev: row.get(2)?,
                        deleted: row.get(3)?,
                    })
                },
            )
            .optional()?)
    }

    /// Stores a route taken over from another device under its id: inserted
    /// at `rev` 1, or replaced (and restored if deleted) with `rev` bumped.
    /// Afterwards it counts as synchronised.
    pub fn put_synced_route(
        &mut self,
        id: Uuid,
        route: &SyncedRoute<'_>,
        now_ms: i64,
    ) -> Result<(), StorageError> {
        self.conn.execute(
            "INSERT INTO routes (id, name, profile, distance_m, ascent_m, plan_json, created_at,
                                 updated_at, rev, synced_rev, source_rev)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, 1, 1, ?8)
             ON CONFLICT (id) DO UPDATE SET
                 name = excluded.name, profile = excluded.profile,
                 distance_m = excluded.distance_m, ascent_m = excluded.ascent_m,
                 plan_json = excluded.plan_json, updated_at = excluded.updated_at,
                 deleted_at = NULL, rev = rev + 1, synced_rev = rev + 1,
                 source_rev = excluded.source_rev",
            params![
                id,
                route.name,
                route.profile,
                route.distance_m,
                route.ascent_m,
                route.plan_json,
                now_ms,
                route.source_rev
            ],
        )?;
        Ok(())
    }

    fn update_route(
        &mut self,
        id: Uuid,
        sql: &str,
        params: impl rusqlite::Params,
    ) -> Result<(), StorageError> {
        match self.conn.execute(sql, params)? {
            0 => Err(StorageError::NotFound(id)),
            _ => Ok(()),
        }
    }

    // --- settings ---------------------------------------------------------

    pub fn setting(&self, key: &str) -> Result<Option<String>, StorageError> {
        Ok(self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()?)
    }

    pub fn set_setting(&mut self, key: &str, value: &str, now_ms: i64) -> Result<(), StorageError> {
        self.conn.execute(
            "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![key, value, now_ms],
        )?;
        Ok(())
    }
}

fn insert_points(
    conn: &Connection,
    segment: Uuid,
    first_seq: i64,
    points: &[TrackPoint],
) -> Result<(), StorageError> {
    let mut stmt = conn.prepare_cached(
        "INSERT INTO track_points (segment_id, seq, time_ms, lat, lon, ele, h_accuracy_m, speed_mps)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    for (i, p) in points.iter().enumerate() {
        stmt.execute(params![
            segment,
            first_seq + i as i64,
            p.time_ms,
            p.lat,
            p.lon,
            p.ele,
            p.h_accuracy_m.map(f64::from),
            p.speed_mps.map(f64::from),
        ])?;
    }
    Ok(())
}

fn stats_to_json(stats: &TrackStats) -> Result<String, StorageError> {
    serde_json::to_string(stats).map_err(|e| StorageError::Corrupt(e.to_string()))
}

/// Row of the `tracks` table before validation.
struct RawTrack {
    id: Uuid,
    name: Option<String>,
    activity: String,
    state: String,
    ended_at: Option<i64>,
    stats_json: Option<String>,
    created_at: i64,
    updated_at: i64,
    rev: i64,
    started_at: Option<i64>,
}

impl RawTrack {
    fn read(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            name: row.get(1)?,
            activity: row.get(2)?,
            state: row.get(3)?,
            ended_at: row.get(4)?,
            stats_json: row.get(5)?,
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
            rev: row.get(8)?,
            started_at: row.get(9)?,
        })
    }

    fn into_record(self) -> Result<TrackRecord, StorageError> {
        let stats = self
            .stats_json
            .map(|json| serde_json::from_str(&json))
            .transpose()
            .map_err(|e| StorageError::Corrupt(format!("statistics of track {}: {e}", self.id)))?;
        Ok(TrackRecord {
            id: self.id,
            name: self.name,
            activity: Activity::from_str(&self.activity)
                .map_err(|e| StorageError::Corrupt(e.to_string()))?,
            state: TrackState::parse(&self.state)?,
            started_at_ms: self.started_at,
            ended_at_ms: self.ended_at,
            stats,
            created_at_ms: self.created_at,
            updated_at_ms: self.updated_at,
            rev: self.rev,
        })
    }
}

/// What decides whether a route changed since it was taken over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteSync {
    pub rev: i64,
    /// The `rev` at the last takeover; equal to `rev` while unchanged here.
    pub synced_rev: i64,
    /// The `rev` of the other device's version; `None` if made on this device.
    pub source_rev: Option<i64>,
    pub deleted: bool,
}

/// A route as another device hands it over.
pub struct SyncedRoute<'a> {
    pub name: &'a str,
    pub profile: &'a str,
    pub distance_m: f64,
    pub ascent_m: Option<f64>,
    pub plan_json: &'a str,
    pub source_rev: i64,
}

/// A saved route as shown in lists; the plan itself is loaded separately.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RouteRecord {
    pub id: Uuid,
    pub name: String,
    /// Routing profile, as named by `core-routing`.
    pub profile: String,
    pub distance_m: f64,
    pub ascent_m: Option<f64>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    /// Counts the changes of the route, starting at 1.
    pub rev: i64,
}
