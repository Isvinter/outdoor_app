//! Recording and tracks for the apps: the position fixes go in, the state of
//! the recording and the stored tracks come out.

use core_app::{LinePoint, RecordingPhase, RecordingStatus};
use core_location::Fix;
use core_storage::TrackRecord;
use core_track::{Activity, TrackStats};
use uuid::Uuid;

use crate::{CoreApp, CoreError, LatLon};

/// A position of the receiver.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct FixInput {
    pub lat: f64,
    pub lon: f64,
    pub ele: Option<f64>,
    /// UTC, milliseconds since the Unix epoch.
    pub time_ms: i64,
    pub h_accuracy_m: Option<f32>,
    pub speed_mps: Option<f32>,
    pub course_deg: Option<f32>,
}

/// A point of the line being recorded.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct LineVertex {
    pub segment: u32,
    pub lat: f64,
    pub lon: f64,
}

/// The state of the recording.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RecordingInfo {
    /// "idle", "recording" or "paused".
    pub phase: String,
    pub track_id: Option<String>,
    /// "bike", "mtb", "hike" or "ski_tour".
    pub activity: Option<String>,
    pub distance_m: f64,
    pub moving_s: f64,
    pub elapsed_s: f64,
    pub ascent_m: Option<f64>,
    /// Points recorded since the last status sent to the listener.
    pub appended: Vec<LineVertex>,
}

/// A stored track as shown in lists.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TrackSummary {
    pub id: String,
    pub name: Option<String>,
    pub activity: String,
    pub started_ms: i64,
    pub distance_m: f64,
    pub moving_s: f64,
    pub ascent_m: Option<f64>,
}

/// A stored track with its geometry.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TrackView {
    pub summary: TrackSummary,
    pub segments: Vec<Vec<LatLon>>,
    pub descent_m: Option<f64>,
    pub max_ele_m: Option<f64>,
    pub elapsed_s: f64,
}

impl From<FixInput> for Fix {
    fn from(fix: FixInput) -> Self {
        Fix {
            lat: fix.lat,
            lon: fix.lon,
            ele: fix.ele,
            time_ms: fix.time_ms,
            h_accuracy_m: fix.h_accuracy_m,
            speed_mps: fix.speed_mps,
            course_deg: fix.course_deg,
        }
    }
}

impl From<&LinePoint> for LineVertex {
    fn from(point: &LinePoint) -> Self {
        LineVertex {
            segment: u32::try_from(point.segment).unwrap_or(u32::MAX),
            lat: point.lat,
            lon: point.lon,
        }
    }
}

pub(crate) fn recording_info(status: &RecordingStatus) -> RecordingInfo {
    let stats = status.stats.as_ref();
    RecordingInfo {
        phase: match status.phase {
            RecordingPhase::Idle => "idle",
            RecordingPhase::Recording => "recording",
            RecordingPhase::Paused => "paused",
        }
        .into(),
        track_id: status.track_id.map(|id| id.to_string()),
        activity: status.activity.map(|a| a.as_str().into()),
        distance_m: stats.map_or(0.0, |s| s.distance_m),
        moving_s: stats.map_or(0.0, |s| s.moving_time_s),
        elapsed_s: stats.map_or(0.0, |s| s.total_time_s),
        ascent_m: stats.map(|s| s.ascent_m),
        appended: status.appended.iter().map(LineVertex::from).collect(),
    }
}

fn summary(record: &TrackRecord, stats: Option<&TrackStats>) -> TrackSummary {
    TrackSummary {
        id: record.id.to_string(),
        name: record.name.clone(),
        activity: record.activity.as_str().into(),
        started_ms: record.started_at_ms.unwrap_or(record.created_at_ms),
        distance_m: stats.map_or(0.0, |s| s.distance_m),
        moving_s: stats.map_or(0.0, |s| s.moving_time_s),
        ascent_m: stats.map(|s| s.ascent_m),
    }
}

fn parse_activity(name: &str) -> Result<Activity, CoreError> {
    name.parse().map_err(|_| CoreError::Failed {
        code: "unknown_activity".into(),
        message: format!("unknown activity: {name}"),
    })
}

fn track_id(id: &str) -> Result<Uuid, CoreError> {
    Uuid::parse_str(id).map_err(|_| CoreError::Failed {
        code: "track_id".into(),
        message: format!("not a track id: {id}"),
    })
}

fn finished(record: Option<TrackRecord>) -> Option<TrackSummary> {
    record.map(|r| summary(&r, r.stats.as_ref()))
}

#[uniffi::export]
impl CoreApp {
    /// A position of the receiver: recorded if a recording runs, and followed
    /// by the guidance if it runs.
    pub fn handle_fix(&self, fix: FixInput) {
        self.app.handle_fix(fix.into());
    }

    /// Starts recording; `activity` is "bike", "mtb", "hike" or "ski_tour".
    pub fn start_recording(&self, activity: String) -> Result<RecordingInfo, CoreError> {
        let status = self.app.start_recording(parse_activity(&activity)?)?;
        Ok(recording_info(&status))
    }

    pub fn pause_recording(&self) -> Result<RecordingInfo, CoreError> {
        Ok(recording_info(&self.app.pause_recording()?))
    }

    pub fn resume_recording(&self) -> Result<RecordingInfo, CoreError> {
        Ok(recording_info(&self.app.resume_recording()?))
    }

    /// Ends the recording; `None` if it recorded nothing worth keeping.
    pub fn stop_recording(&self) -> Result<Option<TrackSummary>, CoreError> {
        Ok(finished(self.app.stop_recording()?))
    }

    pub fn recording_status(&self) -> RecordingInfo {
        recording_info(&self.app.recording_status())
    }

    /// The line recorded so far, one list per segment.
    pub fn recording_line(&self) -> Vec<Vec<LatLon>> {
        self.app
            .recording_line()
            .into_iter()
            .map(|segment| {
                segment
                    .into_iter()
                    .map(|[lon, lat]| LatLon { lat, lon })
                    .collect()
            })
            .collect()
    }

    /// A recording that was in progress when the app last ended.
    pub fn interrupted_recording(&self) -> Result<Option<TrackSummary>, CoreError> {
        Ok(self
            .app
            .interrupted_recording()?
            .map(|r| summary(&r, r.stats.as_ref())))
    }

    pub fn continue_interrupted(&self) -> Result<RecordingInfo, CoreError> {
        Ok(recording_info(&self.app.continue_interrupted()?))
    }

    pub fn finish_interrupted(&self) -> Result<Option<TrackSummary>, CoreError> {
        Ok(finished(self.app.finish_interrupted()?))
    }

    pub fn list_tracks(&self) -> Result<Vec<TrackSummary>, CoreError> {
        Ok(self
            .app
            .list_tracks()?
            .iter()
            .map(|r| summary(r, r.stats.as_ref()))
            .collect())
    }

    /// A track with its geometry; statistics use the terrain model's
    /// elevations where it covers the track.
    pub fn track_detail(&self, id: String) -> Result<TrackView, CoreError> {
        let detail = self.app.track_detail(track_id(&id)?)?;
        let stats = detail.dem_stats.as_ref().or(detail.record.stats.as_ref());
        Ok(TrackView {
            summary: summary(&detail.record, stats),
            segments: detail
                .line
                .into_iter()
                .map(|segment| {
                    segment
                        .into_iter()
                        .map(|[lon, lat]| LatLon { lat, lon })
                        .collect()
                })
                .collect(),
            descent_m: stats.map(|s| s.descent_m),
            max_ele_m: stats.and_then(|s| s.max_ele_m),
            elapsed_s: stats.map_or(0.0, |s| s.total_time_s),
        })
    }

    /// Sets the activity of a finished track ("bike", "mtb", "hike",
    /// "ski_tour" or "other") and computes its statistics anew.
    pub fn set_track_activity(
        &self,
        id: String,
        activity: String,
    ) -> Result<TrackSummary, CoreError> {
        let record = self
            .app
            .set_track_activity(track_id(&id)?, parse_activity(&activity)?)?;
        Ok(summary(&record, record.stats.as_ref()))
    }

    pub fn rename_track(&self, id: String, name: String) -> Result<(), CoreError> {
        Ok(self.app.rename_track(track_id(&id)?, &name)?)
    }

    pub fn delete_track(&self, id: String) -> Result<(), CoreError> {
        Ok(self.app.delete_track(track_id(&id)?)?)
    }

    /// The track as GPX text.
    pub fn export_track_gpx(&self, id: String) -> Result<String, CoreError> {
        Ok(self.app.export_gpx(track_id(&id)?)?)
    }
}
