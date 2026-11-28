//! A tour as a package to hand on: `tour.json` for machines (the later database
//! on Base reads it), `notizen.md` for people, and the files of its photos and
//! voice notes. Format version 1.

use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, BufWriter, Write as _};
use std::path::{Path, PathBuf};

use core_storage::{MediaKind, MediaRecord, MediaSource};
use core_track::{Activity, SegmentReason, TrackStats};
use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, UtcOffset};
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::app::now_ms;
use crate::{App, AppError, SectionDetail, TrackDetail};

/// `format` of every `tour.json`.
pub const PACKAGE_FORMAT: &str = "zerogrid-tour";
/// Version of the package layout and of `tour.json`.
pub const PACKAGE_VERSION: u32 = 1;

/// Content of `tour.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TourManifest {
    pub format: String,
    pub version: u32,
    pub exported_at_ms: i64,
    /// Offset of local time at the start of the tour, for reading the times.
    pub utc_offset_min: i32,
    pub tour: TourInfo,
    pub sections: Vec<SectionInfo>,
    pub media: Vec<MediaInfo>,
}

/// The tour as a whole.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TourInfo {
    /// The track's ID; the same on every export.
    pub id: Uuid,
    pub name: Option<String>,
    /// Activity of the first section.
    pub activity: Activity,
    /// The activities of its sections without rests, in order of appearance.
    pub activities: Vec<Activity>,
    pub started_at_ms: Option<i64>,
    pub ended_at_ms: Option<i64>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub rev: i64,
    pub stats: Option<TrackStats>,
    /// Where the elevations of `stats` and of the sections come from.
    pub elevation: Elevation,
}

/// Source of the elevations in the statistics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Elevation {
    /// The terrain model of the installed regions.
    Terrain,
    /// The GPS receiver.
    Receiver,
}

/// A section: one `<trkseg>` of `track.gpx`, in the same order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionInfo {
    /// The segment's ID; the same on every export.
    pub id: Uuid,
    pub index: usize,
    /// The activity's identifier, or `rest`.
    pub activity: String,
    pub reason: SegmentReason,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
    pub stats: TrackStats,
}

/// A photo or note.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaInfo {
    pub id: Uuid,
    pub kind: MediaKind,
    pub source: MediaSource,
    /// `None` for the whole tour.
    pub section_id: Option<Uuid>,
    pub time_ms: Option<i64>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    /// Its file within the package; `None` for a text note or a missing file.
    pub file: Option<String>,
    pub text: Option<String>,
    pub transcribed: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub rev: i64,
}

/// Where the file of a medium goes in the package: `fotos/<id>.<ext>` for
/// photos (their own format kept), `notizen/<id>.m4a` for voice notes.
pub fn package_path(media: &MediaRecord) -> Option<String> {
    let file = media.file.as_deref()?;
    let ext = Path::new(file)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase);
    let folder = match media.kind {
        MediaKind::Photo => "fotos",
        MediaKind::Voice => "notizen",
        MediaKind::Text => return None,
    };
    Some(match ext {
        Some(ext) => format!("{folder}/{}.{ext}", media.id),
        None => format!("{folder}/{}", media.id),
    })
}

/// Describes a track for `tour.json`. `segment_ids` are its segments' IDs in
/// order; `present` says whether the file of a medium exists.
pub fn tour_manifest(
    detail: &TrackDetail,
    segment_ids: &[Uuid],
    present: &dyn Fn(&MediaRecord) -> bool,
    exported_at_ms: i64,
    utc_offset_min: i32,
) -> TourManifest {
    let record = &detail.record;
    let mut activities = Vec::new();
    for section in detail.sections.iter().filter(|s| !is_rest(s)) {
        if !activities.contains(&section.activity) {
            activities.push(section.activity);
        }
    }
    let sections = detail
        .sections
        .iter()
        .zip(segment_ids)
        .map(|(s, id)| SectionInfo {
            id: *id,
            index: s.index,
            activity: if is_rest(s) {
                "rest".to_owned()
            } else {
                s.activity.as_str().to_owned()
            },
            reason: s.reason,
            start_ms: known(s.start_ms),
            end_ms: known(s.end_ms),
            stats: s.stats.clone(),
        })
        .collect();
    let media = detail
        .media
        .iter()
        .map(|d| {
            let m = &d.record;
            MediaInfo {
                id: m.id,
                kind: m.kind,
                source: m.source,
                section_id: m.segment_id,
                time_ms: m.time_ms,
                lat: m.lat,
                lon: m.lon,
                file: package_path(m).filter(|_| present(m)),
                text: m.text.clone(),
                transcribed: m.transcribed,
                created_at_ms: m.created_at_ms,
                updated_at_ms: m.updated_at_ms,
                rev: m.rev,
            }
        })
        .collect();
    TourManifest {
        format: PACKAGE_FORMAT.to_owned(),
        version: PACKAGE_VERSION,
        exported_at_ms,
        utc_offset_min,
        tour: TourInfo {
            id: record.id,
            name: record.name.clone(),
            activity: record.activity,
            activities,
            started_at_ms: record.started_at_ms,
            ended_at_ms: record.ended_at_ms,
            created_at_ms: record.created_at_ms,
            updated_at_ms: record.updated_at_ms,
            rev: record.rev,
            stats: detail.dem_stats.clone().or_else(|| record.stats.clone()),
            elevation: if detail.dem_stats.is_some() {
                Elevation::Terrain
            } else {
                Elevation::Receiver
            },
        },
        sections,
        media,
    }
}

fn is_rest(section: &SectionDetail) -> bool {
    section.reason == SegmentReason::Rest
}

/// Section times are 0 without fixes.
fn known(ms: i64) -> Option<i64> {
    (ms != 0).then_some(ms)
}

/// `notizen.md`: the notes and photos by section, for people.
pub fn notes_markdown(manifest: &TourManifest) -> String {
    let clock = Clock::new(manifest.utc_offset_min);
    let tour = &manifest.tour;
    let mut out = String::new();
    let title = match (&tour.name, tour.started_at_ms) {
        (Some(name), _) => name.clone(),
        (None, Some(start)) => clock.date_time(start),
        (None, None) => "Tour".to_owned(),
    };
    let _ = writeln!(out, "# {title}\n");

    let mut summary = vec![match tour.started_at_ms {
        Some(start) => {
            let end = tour.ended_at_ms.map(|e| clock.time(e)).unwrap_or_default();
            format!("{}–{end}", clock.date_time(start))
        }
        None => "–".to_owned(),
    }];
    if let Some(stats) = &tour.stats {
        summary.push(distance_and_ascent(stats));
    }
    let labels: Vec<&str> = tour.activities.iter().map(|a| activity_label(*a)).collect();
    if !labels.is_empty() {
        summary.push(labels.join(", "));
    }
    let _ = writeln!(out, "{}", summary.join(" · "));

    let whole: Vec<&MediaInfo> = manifest
        .media
        .iter()
        .filter(|m| {
            m.section_id
                .is_none_or(|id| !manifest.sections.iter().any(|s| s.id == id))
        })
        .collect();
    if !whole.is_empty() {
        out.push_str("\n## Ganze Tour\n\n");
        for m in whole {
            write_media(&mut out, &clock, m);
        }
    }
    for section in &manifest.sections {
        let range = match (section.start_ms, section.end_ms) {
            (Some(s), Some(e)) => format!("{}–{}", clock.time(s), clock.time(e)),
            (Some(s), None) => format!("{}–", clock.time(s)),
            _ => "–".to_owned(),
        };
        if section.reason == SegmentReason::Rest {
            let _ = writeln!(out, "\n## Rast · {range}");
        } else {
            let label = section
                .activity
                .parse::<Activity>()
                .map(activity_label)
                .unwrap_or("Keine Angabe");
            let _ = writeln!(
                out,
                "\n## Abschnitt {} · {label} · {range} · {}",
                section.index + 1,
                distance_and_ascent(&section.stats)
            );
        }
        let items: Vec<&MediaInfo> = manifest
            .media
            .iter()
            .filter(|m| m.section_id == Some(section.id))
            .collect();
        if !items.is_empty() {
            out.push('\n');
            for m in items {
                write_media(&mut out, &clock, m);
            }
        }
    }
    out
}

fn write_media(out: &mut String, clock: &Clock, m: &MediaInfo) {
    let time = m.time_ms.map(|t| clock.time(t));
    let shown = time.as_deref().unwrap_or("–");
    let text = m.text.as_deref().filter(|t| !t.trim().is_empty());
    let body = match (m.kind, &m.file) {
        (MediaKind::Photo, Some(file)) => {
            let alt = time
                .as_ref()
                .map_or("Foto".to_owned(), |t| format!("Foto {t}"));
            let mut body = format!("Foto: ![{alt}]({file})");
            if let Some(text) = text {
                let _ = write!(body, " – {text}");
            }
            body
        }
        (MediaKind::Photo, None) => match text {
            Some(text) => format!("Foto (Datei fehlt): {text}"),
            None => "Foto (Datei fehlt)".to_owned(),
        },
        (MediaKind::Voice, Some(file)) => match text {
            Some(text) => format!("Sprachnotiz: „{text}“ ([Audio]({file}))"),
            None => format!("Sprachnotiz: [Audio]({file})"),
        },
        (MediaKind::Voice, None) => match text {
            Some(text) => format!("Sprachnotiz (Datei fehlt): „{text}“"),
            None => "Sprachnotiz (Datei fehlt)".to_owned(),
        },
        (MediaKind::Text, _) => format!("Notiz: {}", text.unwrap_or("")),
    };
    let _ = writeln!(out, "- {shown} · {}", body.replace('\n', "\n  "));
}

fn distance_and_ascent(stats: &TrackStats) -> String {
    let km = format!("{:.1}", stats.distance_m / 1000.0).replace('.', ",");
    format!("{km} km · ↑ {:.0} m", stats.ascent_m)
}

/// German names of the activities, as in Trail.
fn activity_label(activity: Activity) -> &'static str {
    match activity {
        Activity::Bike => "Rad",
        Activity::Mtb => "MTB",
        Activity::Hike => "Wandern",
        Activity::SkiTour => "Skitour",
        Activity::Other => "Keine Angabe",
    }
}

/// Local times at a fixed offset.
struct Clock(UtcOffset);

impl Clock {
    fn new(offset_min: i32) -> Self {
        Clock(
            UtcOffset::from_whole_seconds(offset_min.saturating_mul(60)).unwrap_or(UtcOffset::UTC),
        )
    }

    fn local(&self, ms: i64) -> Option<OffsetDateTime> {
        OffsetDateTime::from_unix_timestamp_nanos(i128::from(ms) * 1_000_000)
            .ok()
            .map(|t| t.to_offset(self.0))
    }

    /// "10:05"
    fn time(&self, ms: i64) -> String {
        self.local(ms)
            .map(|t| format!("{:02}:{:02}", t.hour(), t.minute()))
            .unwrap_or_else(|| "–".to_owned())
    }

    /// "24.11.2026, 10:05"
    fn date_time(&self, ms: i64) -> String {
        self.local(ms)
            .map(|t| {
                format!(
                    "{:02}.{:02}.{}, {:02}:{:02}",
                    t.day(),
                    u8::from(t.month()),
                    t.year(),
                    t.hour(),
                    t.minute()
                )
            })
            .unwrap_or_else(|| "–".to_owned())
    }
}

/// Entries of at least this size need ZIP64 headers of their own.
const LARGE_FILE: u64 = u32::MAX as u64;

impl App {
    /// Writes a tour as a ZIP package to `dest`, replacing a file there; see
    /// [`tour_manifest`] and [`notes_markdown`]. `utc_offset_min` is the offset
    /// of local time for the readable notes. Not for the track being recorded.
    pub fn export_tour(&self, id: Uuid, dest: &Path, utc_offset_min: i32) -> Result<(), AppError> {
        if self.shared.inner().recording_track() == Some(id) {
            return Err(AppError::RecordingActive);
        }
        // Each call holds the app's lock only while it reads; writing a package
        // of gigabytes must not hold up a recording.
        let detail = self.track_detail(id)?;
        let segment_ids = self.section_ids(id)?;
        let gpx = self.export_gpx(id)?;
        let data_dir = self.shared.data_dir.clone();

        // Only paths here: a tour has hundreds of photos, more than iOS lets an
        // app keep open at once. Each file is opened when it is written.
        let mut files: Vec<(String, PathBuf)> = Vec::new();
        let mut present = std::collections::HashSet::new();
        for media in detail.media.iter().map(|d| &d.record) {
            let (Some(target), Some(file)) = (package_path(media), media.file.as_deref()) else {
                continue;
            };
            let source = data_dir.join(file);
            if File::open(&source).is_ok() {
                present.insert(media.id);
                files.push((target, source));
            }
        }
        let manifest = tour_manifest(
            &detail,
            &segment_ids,
            &|m| present.contains(&m.id),
            now_ms(),
            utc_offset_min,
        );
        let part = part_path(dest);
        let written = write_package(&part, &gpx, &manifest, files)
            .and_then(|()| std::fs::rename(&part, dest).map_err(io_error));
        if written.is_err() {
            let _ = std::fs::remove_file(&part);
        }
        written
    }
}

fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    dest.with_file_name(name)
}

fn write_package(
    path: &Path,
    gpx: &str,
    manifest: &TourManifest,
    files: Vec<(String, PathBuf)>,
) -> Result<(), AppError> {
    let json = serde_json::to_vec_pretty(manifest).map_err(|e| AppError::Export(e.to_string()))?;
    let notes = notes_markdown(manifest);
    let mut zip = ZipWriter::new(BufWriter::new(File::create(path).map_err(io_error)?));
    let text = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, content) in [
        ("track.gpx", gpx.as_bytes()),
        ("tour.json", json.as_slice()),
        ("notizen.md", notes.as_bytes()),
    ] {
        zip.start_file(name, text).map_err(zip_error)?;
        zip.write_all(content).map_err(io_error)?;
    }
    // Photos and audio are compressed already; storing them saves battery.
    for (name, path) in files {
        let mut source = File::open(&path).map_err(io_error)?;
        let size = source.metadata().map_err(io_error)?.len();
        let stored = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .large_file(size >= LARGE_FILE);
        zip.start_file(name, stored).map_err(zip_error)?;
        io::copy(&mut source, &mut zip).map_err(io_error)?;
    }
    let mut out = zip.finish().map_err(zip_error)?;
    out.flush().map_err(io_error)?;
    out.get_ref().sync_all().map_err(io_error)
}

fn io_error(e: io::Error) -> AppError {
    if e.kind() == io::ErrorKind::StorageFull {
        AppError::ExportNoSpace
    } else {
        AppError::Export(e.to_string())
    }
}

fn zip_error(e: zip::result::ZipError) -> AppError {
    match e {
        zip::result::ZipError::Io(e) => io_error(e),
        e => AppError::Export(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_full_maps_to_no_space() {
        let full = io_error(io::Error::from(io::ErrorKind::StorageFull));
        assert!(matches!(full, AppError::ExportNoSpace));
        assert_eq!(full.code(), "export_no_space");
        let other = io_error(io::Error::from(io::ErrorKind::PermissionDenied));
        assert_eq!(other.code(), "export_failed");
        let zipped = zip_error(zip::result::ZipError::Io(io::Error::from(
            io::ErrorKind::StorageFull,
        )));
        assert!(matches!(zipped, AppError::ExportNoSpace));
    }
}
