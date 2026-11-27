//! Photos and notes of a track. The app writes their files into the folder
//! [`MEDIA_DIR`] next to the database and names them by their path relative
//! to the database's folder; the core keeps the metadata, assigns them to a
//! section and removes the files with them.

use std::path::{Component, Path};

use core_storage::{MediaKind, MediaRecord, MediaSource, NewMedia, Storage};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::App;
use crate::AppError;
use crate::app::now_ms;

/// Folder of the media files, inside the folder of the database.
pub(crate) const MEDIA_DIR: &str = "media";

/// A photo or note to be added to a track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaInput {
    pub kind: MediaKind,
    pub source: MediaSource,
    /// Path relative to the folder of the database, `media/<name>`; required
    /// for photos and voice notes.
    pub file: Option<String>,
    /// Required for text notes.
    pub text: Option<String>,
    /// When it was taken; without, the beginning of its section.
    pub time_ms: Option<i64>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
}

/// A stored photo or note with the section it belongs to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaDetail {
    pub record: MediaRecord,
    /// Index of its section as in [`crate::SectionDetail::index`]; `None` for
    /// the whole track.
    pub section: Option<usize>,
}

/// The media of a track with the indexes of their sections.
pub(crate) fn media_details(storage: &Storage, track: Uuid) -> Result<Vec<MediaDetail>, AppError> {
    let segments = storage.segment_ids(track)?;
    Ok(storage
        .list_media(track)?
        .into_iter()
        .map(|record| detail(&segments, record))
        .collect())
}

fn detail(segments: &[Uuid], record: MediaRecord) -> MediaDetail {
    let section = record
        .segment_id
        .and_then(|id| segments.iter().position(|&s| s == id));
    MediaDetail { record, section }
}

fn detail_of(storage: &Storage, id: Uuid) -> Result<MediaDetail, AppError> {
    let record = storage.media(id)?;
    let segments = storage.segment_ids(record.track_id)?;
    Ok(detail(&segments, record))
}

/// Removes the files of media; one already gone is no error, nor one that
/// cannot be removed (the records are deleted already).
pub(crate) fn remove_media_files(data_dir: &Path, media: &[MediaRecord]) {
    for file in media.iter().filter_map(|m| m.file.as_deref()) {
        if is_media_path(file) {
            let _ = std::fs::remove_file(data_dir.join(file));
        }
    }
}

/// Whether `file` is `media/<name>`: nothing outside the media folder, no subfolders.
fn is_media_path(file: &str) -> bool {
    let mut parts = Path::new(file).components();
    matches!(
        (parts.next(), parts.next(), parts.next()),
        (Some(Component::Normal(dir)), Some(Component::Normal(_)), None) if dir == MEDIA_DIR
    )
}

fn check_file(data_dir: &Path, file: &str) -> Result<(), AppError> {
    if !is_media_path(file) {
        return Err(AppError::MediaFile(format!(
            "`{file}` is not a file in `{MEDIA_DIR}/`"
        )));
    }
    if !data_dir.join(file).is_file() {
        return Err(AppError::MediaFile(format!("`{file}` does not exist")));
    }
    Ok(())
}

fn is_blank(text: Option<&str>) -> bool {
    text.is_none_or(|t| t.trim().is_empty())
}

/// Checks that kind, source, file and text fit together, and the file.
fn check(
    data_dir: &Path,
    kind: MediaKind,
    source: MediaSource,
    file: Option<&str>,
    text: Option<&str>,
) -> Result<(), AppError> {
    let fits = match kind {
        MediaKind::Photo => {
            matches!(source, MediaSource::Camera | MediaSource::Library) && file.is_some()
        }
        MediaKind::Voice => source == MediaSource::Spoken && file.is_some(),
        MediaKind::Text => source == MediaSource::Typed && file.is_none() && !is_blank(text),
    };
    if !fits {
        return Err(AppError::InvalidMedia);
    }
    file.map_or(Ok(()), |f| check_file(data_dir, f))
}

impl App {
    /// The folder the app writes photos and voice notes into before adding them.
    pub fn media_dir(&self) -> std::path::PathBuf {
        self.shared.data_dir.join(MEDIA_DIR)
    }

    /// Adds a photo or note to the running recording, taken now at the last
    /// position. It belongs to the last section that has a point: the running
    /// one, the rest, during a pause the one before; before the first point
    /// to the whole track.
    pub fn add_recording_media(
        &self,
        kind: MediaKind,
        source: MediaSource,
        file: Option<&str>,
        text: Option<&str>,
    ) -> Result<MediaDetail, AppError> {
        check(&self.shared.data_dir, kind, source, file, text)?;
        let mut inner = self.shared.inner();
        let (track_id, segment_id) = inner
            .recording_media_target()
            .ok_or(AppError::NoRecording)?;
        let place = inner.last_fix.map(|f| (f.lat, f.lon));
        let id = inner.storage.add_media(
            &NewMedia {
                track_id,
                segment_id,
                kind,
                source,
                time_ms: Some(now_ms()),
                lat: place.map(|p| p.0),
                lon: place.map(|p| p.1),
                file,
                text,
            },
            now_ms(),
        )?;
        detail_of(&inner.storage, id)
    }

    /// Adds a photo or note to section `section` of a track (`None`: the whole
    /// track), finished or being recorded. Without a time it takes the
    /// beginning of the section, or of the track.
    pub fn add_track_media(
        &self,
        track: Uuid,
        section: Option<usize>,
        input: MediaInput,
    ) -> Result<MediaDetail, AppError> {
        let (file, text) = (input.file.as_deref(), input.text.as_deref());
        let mut inner = self.shared.inner();
        let loaded = inner.storage.load_track(track)?;
        let segments = inner.storage.segment_ids(track)?;
        let segment_id = match section {
            None => None,
            Some(i) => Some(*segments.get(i).ok_or(AppError::InvalidSection)?),
        };
        check(&self.shared.data_dir, input.kind, input.source, file, text)?;
        let first_time = |points: &[core_track::TrackPoint]| points.iter().find_map(|p| p.time_ms);
        let start = match section {
            Some(i) => first_time(&loaded.segments[i].points),
            None => loaded.segments.iter().find_map(|s| first_time(&s.points)),
        };
        let id = inner.storage.add_media(
            &NewMedia {
                track_id: track,
                segment_id,
                kind: input.kind,
                source: input.source,
                time_ms: input.time_ms.or(start),
                lat: input.lat,
                lon: input.lon,
                file,
                text,
            },
            now_ms(),
        )?;
        detail_of(&inner.storage, id)
    }

    /// Sets the text of a photo or note; it keeps whether it was transcribed.
    /// An empty text removes it, except from a text note: [`AppError::InvalidMedia`].
    pub fn set_media_text(&self, id: Uuid, text: &str) -> Result<MediaDetail, AppError> {
        let mut inner = self.shared.inner();
        let record = inner.storage.media(id)?;
        let text = (!text.trim().is_empty()).then_some(text);
        if record.kind == MediaKind::Text && text.is_none() {
            return Err(AppError::InvalidMedia);
        }
        inner
            .storage
            .set_media_text(id, text, record.transcribed, now_ms())?;
        detail_of(&inner.storage, id)
    }

    /// Sets the text transcribed from a voice note; other media have no voice:
    /// [`AppError::InvalidMedia`].
    pub fn set_media_transcript(&self, id: Uuid, text: &str) -> Result<MediaDetail, AppError> {
        let mut inner = self.shared.inner();
        if inner.storage.media(id)?.kind != MediaKind::Voice {
            return Err(AppError::InvalidMedia);
        }
        inner
            .storage
            .set_media_text(id, Some(text), true, now_ms())?;
        detail_of(&inner.storage, id)
    }

    /// Deletes a photo or note and its file (only the app's copy; one already
    /// gone is no error).
    pub fn delete_media(&self, id: Uuid) -> Result<(), AppError> {
        let mut inner = self.shared.inner();
        let record = inner.storage.media(id)?;
        inner.storage.delete_media(id, now_ms())?;
        remove_media_files(&self.shared.data_dir, std::slice::from_ref(&record));
        Ok(())
    }
}
