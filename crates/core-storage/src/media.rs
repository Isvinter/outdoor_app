//! Photos and notes of a track: metadata only, the files live in the app's
//! data folder and are named by a path relative to it.

use std::fmt;
use std::str::FromStr;

use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Storage, StorageError};

/// What a medium is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Photo,
    Voice,
    Text,
}

/// Where a medium came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaSource {
    /// Taken with the app's camera.
    Camera,
    /// Picked from the device's photo library.
    Library,
    Typed,
    Spoken,
}

/// A value that names no kind or source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownMediaValue(pub String);

impl fmt::Display for UnknownMediaValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown media value `{}`", self.0)
    }
}

impl std::error::Error for UnknownMediaValue {}

impl MediaKind {
    pub const ALL: [MediaKind; 3] = [MediaKind::Photo, MediaKind::Voice, MediaKind::Text];

    pub fn as_str(self) -> &'static str {
        match self {
            MediaKind::Photo => "photo",
            MediaKind::Voice => "voice",
            MediaKind::Text => "text",
        }
    }
}

impl FromStr for MediaKind {
    type Err = UnknownMediaValue;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        MediaKind::ALL
            .into_iter()
            .find(|k| k.as_str() == s)
            .ok_or_else(|| UnknownMediaValue(s.into()))
    }
}

impl MediaSource {
    pub const ALL: [MediaSource; 4] = [
        MediaSource::Camera,
        MediaSource::Library,
        MediaSource::Typed,
        MediaSource::Spoken,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            MediaSource::Camera => "camera",
            MediaSource::Library => "library",
            MediaSource::Typed => "typed",
            MediaSource::Spoken => "spoken",
        }
    }
}

impl FromStr for MediaSource {
    type Err = UnknownMediaValue;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        MediaSource::ALL
            .into_iter()
            .find(|k| k.as_str() == s)
            .ok_or_else(|| UnknownMediaValue(s.into()))
    }
}

/// A medium to be stored.
#[derive(Debug, Clone, PartialEq)]
pub struct NewMedia<'a> {
    pub track_id: Uuid,
    /// The segment it belongs to; `None` for the whole track.
    pub segment_id: Option<Uuid>,
    pub kind: MediaKind,
    pub source: MediaSource,
    pub time_ms: Option<i64>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    /// Path of its file relative to the app's data folder.
    pub file: Option<&'a str>,
    pub text: Option<&'a str>,
}

/// A stored medium.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaRecord {
    pub id: Uuid,
    pub track_id: Uuid,
    /// `None` for the whole track.
    pub segment_id: Option<Uuid>,
    pub kind: MediaKind,
    pub source: MediaSource,
    pub time_ms: Option<i64>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    /// Path of its file relative to the app's data folder.
    pub file: Option<String>,
    pub text: Option<String>,
    /// Whether `text` was transcribed from the recorded voice.
    pub transcribed: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    /// Incremented on every change; basis for synchronisation.
    pub rev: i64,
}

const MEDIA_COLUMNS: &str = "id, track_id, segment_id, kind, source, time_ms, lat, lon, file, text,
    transcribed, created_at, updated_at, rev";

impl Storage {
    /// Stores a medium. The track must exist, and the segment, if given, belong to it.
    pub fn add_media(&mut self, media: &NewMedia, now_ms: i64) -> Result<Uuid, StorageError> {
        self.require_track(media.track_id)?;
        if let Some(segment) = media.segment_id {
            let found: Option<i64> = self
                .conn
                .query_row(
                    "SELECT 1 FROM track_segments WHERE id = ?1 AND track_id = ?2",
                    params![segment, media.track_id],
                    |row| row.get(0),
                )
                .optional()?;
            found.ok_or(StorageError::NotFound(segment))?;
        }
        let id = Uuid::now_v7();
        self.conn.execute(
            "INSERT INTO track_media (id, track_id, segment_id, kind, source, time_ms, lat, lon,
                 file, text, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)",
            params![
                id,
                media.track_id,
                media.segment_id,
                media.kind.as_str(),
                media.source.as_str(),
                media.time_ms,
                media.lat,
                media.lon,
                media.file,
                media.text,
                now_ms
            ],
        )?;
        Ok(id)
    }

    /// A medium that is not deleted.
    pub fn media(&self, id: Uuid) -> Result<MediaRecord, StorageError> {
        let sql =
            format!("SELECT {MEDIA_COLUMNS} FROM track_media WHERE id = ?1 AND deleted_at IS NULL");
        let raw = self.conn.query_row(&sql, [id], RawMedia::read).optional()?;
        raw.ok_or(StorageError::NotFound(id))?.into_record()
    }

    /// The media of a track that are not deleted, by time; those without a time last.
    pub fn list_media(&self, track: Uuid) -> Result<Vec<MediaRecord>, StorageError> {
        let sql = format!(
            "SELECT {MEDIA_COLUMNS} FROM track_media WHERE track_id = ?1 AND deleted_at IS NULL
             ORDER BY time_ms IS NULL, time_ms, created_at, id"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([track], RawMedia::read)?;
        rows.map(|raw| raw?.into_record()).collect()
    }

    /// Sets the text of a medium and whether it was transcribed.
    pub fn set_media_text(
        &mut self,
        id: Uuid,
        text: Option<&str>,
        transcribed: bool,
        now_ms: i64,
    ) -> Result<(), StorageError> {
        self.update_track(
            id,
            "UPDATE track_media SET text = ?2, transcribed = ?3, updated_at = ?4, rev = rev + 1
             WHERE id = ?1 AND deleted_at IS NULL",
            params![id, text, transcribed, now_ms],
        )
    }

    /// Marks a medium as deleted; the row stays as a tombstone for synchronisation.
    pub fn delete_media(&mut self, id: Uuid, now_ms: i64) -> Result<(), StorageError> {
        self.update_track(
            id,
            "UPDATE track_media SET deleted_at = ?2, updated_at = ?2, rev = rev + 1
             WHERE id = ?1 AND deleted_at IS NULL",
            params![id, now_ms],
        )
    }

    /// The ids of the segments of a track, in order; the position is the
    /// index of the section, as in [`Storage::load_track`].
    pub fn segment_ids(&self, track: Uuid) -> Result<Vec<Uuid>, StorageError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM track_segments WHERE track_id = ?1 ORDER BY seq")?;
        let rows = stmt.query_map([track], |row| row.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

/// Row of the `track_media` table before validation.
struct RawMedia {
    id: Uuid,
    track_id: Uuid,
    segment_id: Option<Uuid>,
    kind: String,
    source: String,
    time_ms: Option<i64>,
    lat: Option<f64>,
    lon: Option<f64>,
    file: Option<String>,
    text: Option<String>,
    transcribed: bool,
    created_at: i64,
    updated_at: i64,
    rev: i64,
}

impl RawMedia {
    fn read(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            track_id: row.get(1)?,
            segment_id: row.get(2)?,
            kind: row.get(3)?,
            source: row.get(4)?,
            time_ms: row.get(5)?,
            lat: row.get(6)?,
            lon: row.get(7)?,
            file: row.get(8)?,
            text: row.get(9)?,
            transcribed: row.get(10)?,
            created_at: row.get(11)?,
            updated_at: row.get(12)?,
            rev: row.get(13)?,
        })
    }

    fn into_record(self) -> Result<MediaRecord, StorageError> {
        let corrupt = |e: UnknownMediaValue| StorageError::Corrupt(e.to_string());
        Ok(MediaRecord {
            id: self.id,
            track_id: self.track_id,
            segment_id: self.segment_id,
            kind: self.kind.parse().map_err(corrupt)?,
            source: self.source.parse().map_err(corrupt)?,
            time_ms: self.time_ms,
            lat: self.lat,
            lon: self.lon,
            file: self.file,
            text: self.text,
            transcribed: self.transcribed,
            created_at_ms: self.created_at,
            updated_at_ms: self.updated_at,
            rev: self.rev,
        })
    }
}
