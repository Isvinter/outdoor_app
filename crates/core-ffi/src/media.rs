//! Photos and notes of a track for the apps. The app writes the file into
//! [`CoreApp::media_dir`] first and names it relative to the data folder,
//! `media/<name>`; the core keeps the rest.

use core_app::MediaDetail;
use core_storage::{MediaKind, MediaSource};
use uuid::Uuid;

use crate::{CoreApp, CoreError};

/// A photo or note of a track.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MediaItem {
    pub id: String,
    /// Index of its section, as in `TrackSection.index`; `None` for the whole track.
    pub section: Option<u32>,
    /// "photo", "voice" or "text".
    pub kind: String,
    /// "camera", "library", "typed" or "spoken".
    pub source: String,
    pub time_ms: Option<i64>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    /// Path relative to the data folder, `media/<name>`.
    pub file: Option<String>,
    pub text: Option<String>,
    /// Whether `text` was transcribed from the voice.
    pub transcribed: bool,
    pub updated_ms: i64,
}

/// A photo or note to add to a track.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MediaInput {
    /// "photo", "voice" or "text".
    pub kind: String,
    /// "camera", "library", "typed" or "spoken".
    pub source: String,
    /// `media/<name>`; for photos and voice notes.
    pub file: Option<String>,
    /// For text notes.
    pub text: Option<String>,
    /// Without, the beginning of the section.
    pub time_ms: Option<i64>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
}

pub(crate) fn media_item(detail: &MediaDetail) -> MediaItem {
    let r = &detail.record;
    MediaItem {
        id: r.id.to_string(),
        section: detail.section.map(|i| u32::try_from(i).unwrap_or(u32::MAX)),
        kind: r.kind.as_str().into(),
        source: r.source.as_str().into(),
        time_ms: r.time_ms,
        lat: r.lat,
        lon: r.lon,
        file: r.file.clone(),
        text: r.text.clone(),
        transcribed: r.transcribed,
        updated_ms: r.updated_at_ms,
    }
}

fn failed(code: &str, message: String) -> CoreError {
    CoreError::Failed {
        code: code.into(),
        message,
    }
}

fn parse_kind(kind: &str) -> Result<MediaKind, CoreError> {
    kind.parse()
        .map_err(|_| failed("unknown_media_kind", format!("unknown media kind: {kind}")))
}

fn parse_source(source: &str) -> Result<MediaSource, CoreError> {
    source.parse().map_err(|_| {
        failed(
            "unknown_media_source",
            format!("unknown media source: {source}"),
        )
    })
}

fn media_id(id: &str) -> Result<Uuid, CoreError> {
    Uuid::parse_str(id).map_err(|_| failed("media_id", format!("not a media id: {id}")))
}

fn track_id(id: &str) -> Result<Uuid, CoreError> {
    Uuid::parse_str(id).map_err(|_| failed("track_id", format!("not a track id: {id}")))
}

#[uniffi::export]
impl CoreApp {
    /// The folder to write photos and voice notes into before adding them.
    pub fn media_dir(&self) -> String {
        self.app.media_dir().display().to_string()
    }

    /// Adds a photo or note to the running recording, taken now at the last
    /// position, in the running section (see `docs/ALGORITHMEN.md`, 6b).
    pub fn add_recording_media(
        &self,
        kind: String,
        source: String,
        file: Option<String>,
        text: Option<String>,
    ) -> Result<MediaItem, CoreError> {
        let detail = self.app.add_recording_media(
            parse_kind(&kind)?,
            parse_source(&source)?,
            file.as_deref(),
            text.as_deref(),
        )?;
        Ok(media_item(&detail))
    }

    /// Adds a photo or note to a section of a track (`None`: the whole track).
    pub fn add_track_media(
        &self,
        track_id: String,
        section: Option<u32>,
        input: MediaInput,
    ) -> Result<MediaItem, CoreError> {
        let input = core_app::MediaInput {
            kind: parse_kind(&input.kind)?,
            source: parse_source(&input.source)?,
            file: input.file,
            text: input.text,
            time_ms: input.time_ms,
            lat: input.lat,
            lon: input.lon,
        };
        let section = section.map(|i| usize::try_from(i).unwrap_or(usize::MAX));
        let detail = self
            .app
            .add_track_media(self::track_id(&track_id)?, section, input)?;
        Ok(media_item(&detail))
    }

    /// Sets the text of a photo or note; an empty text removes it (not from a
    /// text note: code "invalid_media").
    pub fn set_media_text(&self, id: String, text: String) -> Result<MediaItem, CoreError> {
        Ok(media_item(&self.app.set_media_text(media_id(&id)?, &text)?))
    }

    /// Sets the text transcribed from a voice note.
    pub fn set_media_transcript(&self, id: String, text: String) -> Result<MediaItem, CoreError> {
        Ok(media_item(
            &self.app.set_media_transcript(media_id(&id)?, &text)?,
        ))
    }

    /// Deletes a photo or note and the app's copy of its file.
    pub fn delete_media(&self, id: String) -> Result<(), CoreError> {
        Ok(self.app.delete_media(media_id(&id)?)?)
    }
}
