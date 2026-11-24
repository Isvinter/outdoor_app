use std::path::Path;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use core_gpx::{GpxDocument, parse_gpx, write_gpx};
use core_location::{Fix, FixSink, LocationConfig, LocationSource, SourceStatus};
use core_storage::{Storage, TrackRecord, TrackState};
use core_track::{
    Activity, Recorder, RecorderState, StatsParams, Track, TrackStats, compute_stats,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppError;
use crate::guide::{ActiveGuide, GuideStatus};

/// Recording updates are sent to the listener at most this often.
const UPDATE_INTERVAL: Duration = Duration::from_millis(250);
/// A fix is recorded if it follows the previous one after at least this share of
/// the activity's interval; receivers do not tick with millisecond precision.
const INTERVAL_TOLERANCE: f64 = 0.8;
/// After this long without a fix kept, the recording goes on in a new
/// segment: the gap counts neither as distance nor as time moving.
const GAP_MS: i64 = 120_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordingPhase {
    Idle,
    Recording,
    Paused,
}

/// A point of the track being recorded, for drawing the live line.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LinePoint {
    pub segment: usize,
    pub lon: f64,
    pub lat: f64,
}

/// State of the current recording.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordingStatus {
    pub phase: RecordingPhase,
    pub track_id: Option<Uuid>,
    pub activity: Option<Activity>,
    /// Statistics of the track so far.
    pub stats: Option<TrackStats>,
    /// Points recorded since the previous update sent to the listener.
    pub appended: Vec<LinePoint>,
}

/// A stored track with its geometry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackDetail {
    pub record: TrackRecord,
    /// One list of `[lon, lat]` positions per segment.
    pub line: Vec<Vec<[f64; 2]>>,
    /// Statistics with elevation from the terrain model of the installed
    /// regions instead of the receiver; `None` unless the model covers the track.
    pub dem_stats: Option<TrackStats>,
}

/// Terrain elevation replaces receiver elevation only if it is known for this
/// share of the points; a mix of both sources would invent ascent.
const DEM_MIN_COVERAGE: f64 = 0.99;
/// Terrain elevation read along a track is rough in its own way: on a road
/// across a slope a few metres sideways are a few metres up or down. Smoothing
/// over twice the distance used for receiver elevation evens that out
/// (calibrated on the real tracks, see docs/ALGORITHMEN.md).
const DEM_SMOOTH_FACTOR: f64 = 2.0;

/// Receives what happens inside the app. Called from arbitrary threads; must not block.
pub trait AppListener: Send + Sync {
    fn on_position(&self, fix: &Fix);
    fn on_recording(&self, status: &RecordingStatus);
    fn on_source_status(&self, status: &SourceStatus);
    /// A background operation failed, e.g. a fix could not be stored.
    fn on_error(&self, message: &str);
    /// Download progress of a map region.
    fn on_region_progress(&self, _progress: &core_maps::Progress) {}
    /// Guidance along the planned route: its status on start and whenever a
    /// fix changes it; `None` when it ends. Told in the order of the changes;
    /// must not start or stop the guidance or take in a fix (see `guide/app.rs`).
    fn on_guidance(&self, _status: Option<&GuideStatus>) {}
}

/// The application. One instance per database.
pub struct App {
    pub(crate) shared: Arc<Shared>,
}

pub(crate) struct Shared {
    inner: Mutex<Inner>,
    pub(crate) maps: crate::regions::Maps,
    pub(crate) routing: crate::routes::Routing,
    /// Separate from `inner`: stopping a source joins its thread, which may be
    /// waiting for `inner` to deliver a fix.
    source: Mutex<Option<Box<dyn LocationSource>>>,
    pub(crate) listener: Arc<dyn AppListener>,
    /// Guidance along the planned route; see `guide/app.rs` for the locking.
    pub(crate) guide: Mutex<Option<ActiveGuide>>,
    /// Held while a fix is taken in by the guide, so that fixes are taken in
    /// one after the other; `guide` stays free meanwhile.
    pub(crate) guide_turn: Mutex<()>,
    /// Taken while `guide` is still held and kept until the listener was
    /// told, so that it hears of the guides in the order `guide` saw them.
    pub(crate) guide_told: Mutex<()>,
    /// Numbers the guides started; a fix taken in by an older one is dropped.
    pub(crate) guide_generation: AtomicU64,
}

pub(crate) struct Inner {
    pub(crate) storage: Storage,
    pub(crate) last_fix: Option<Fix>,
    recording: Option<ActiveRecording>,
}

struct ActiveRecording {
    track_id: Uuid,
    activity: Activity,
    recorder: Recorder,
    params: StatsParams,
    /// Database segment that receives points, and the recorder segment it mirrors.
    segment: Option<(usize, Uuid)>,
    last_recorded_ms: Option<i64>,
    /// Points not yet announced to the listener.
    pending: Vec<LinePoint>,
    last_update: Option<Instant>,
}

impl ActiveRecording {
    fn new(track_id: Uuid, activity: Activity, recorder: Recorder) -> Self {
        Self {
            track_id,
            activity,
            recorder,
            params: StatsParams::for_activity(activity),
            segment: None,
            last_recorded_ms: None,
            pending: Vec::new(),
            last_update: None,
        }
    }

    /// Current status; hands over the points collected since the last call.
    fn status(&mut self) -> RecordingStatus {
        self.last_update = Some(Instant::now());
        RecordingStatus {
            phase: match self.recorder.state() {
                RecorderState::Recording => RecordingPhase::Recording,
                RecorderState::Paused => RecordingPhase::Paused,
                RecorderState::Stopped => RecordingPhase::Idle,
            },
            track_id: Some(self.track_id),
            activity: Some(self.activity),
            stats: Some(self.recorder.stats(&self.params)),
            appended: std::mem::take(&mut self.pending),
        }
    }
}

const IDLE: RecordingStatus = RecordingStatus {
    phase: RecordingPhase::Idle,
    track_id: None,
    activity: None,
    stats: None,
    appended: Vec::new(),
};

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn line_of(track: &Track) -> Vec<Vec<[f64; 2]>> {
    track
        .segments
        .iter()
        .map(|s| s.points.iter().map(|p| [p.lon, p.lat]).collect())
        .collect()
}

impl Shared {
    pub(crate) fn inner(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn handle_fix(&self, fix: Fix) {
        let (update, error) = {
            let mut inner = self.inner();
            inner.last_fix = Some(fix);
            match inner.record(fix) {
                Ok(update) => (update, None),
                Err(e) => (None, Some(format!("could not store position: {e}"))),
            }
        };
        self.listener.on_position(&fix);
        if let Some(status) = update {
            self.listener.on_recording(&status);
        }
        if let Some(message) = error {
            self.listener.on_error(&message);
        }
        self.guide_fix(&fix);
    }
}

impl Inner {
    /// The track being recorded, if any.
    pub(crate) fn recording_track(&self) -> Option<Uuid> {
        self.recording.as_ref().map(|rec| rec.track_id)
    }

    /// Adds a fix to the active recording. Returns a status if one is due.
    fn record(&mut self, fix: Fix) -> Result<Option<RecordingStatus>, AppError> {
        let Some(rec) = &mut self.recording else {
            return Ok(None);
        };
        let min_gap = f64::from(rec.activity.location_interval_ms()) * INTERVAL_TOLERANCE;
        if rec
            .last_recorded_ms
            .is_some_and(|last| ((fix.time_ms - last) as f64) < min_gap)
        {
            return Ok(None);
        }
        if rec
            .last_recorded_ms
            .is_some_and(|last| fix.time_ms - last > GAP_MS)
        {
            rec.recorder.break_segment();
        }
        let point = fix.to_track_point();
        if !rec.recorder.add_point(point) {
            return Ok(None);
        }
        rec.last_recorded_ms = Some(fix.time_ms);
        let index = rec.recorder.current_segment();
        let segment_id = match rec.segment {
            Some((i, id)) if i == index => id,
            _ => {
                let id = self.storage.begin_segment(rec.track_id, now_ms())?;
                rec.segment = Some((index, id));
                id
            }
        };
        self.storage.append_point(segment_id, &point)?;
        rec.pending.push(LinePoint {
            segment: index,
            lon: fix.lon,
            lat: fix.lat,
        });
        let due = rec
            .last_update
            .is_none_or(|t| t.elapsed() >= UPDATE_INTERVAL);
        Ok(due.then(|| rec.status()))
    }

    /// Finishes a track: caches its statistics, or deletes it if it has no points.
    fn finish(
        &mut self,
        id: Uuid,
        track: &Track,
        activity: Activity,
    ) -> Result<Option<TrackRecord>, AppError> {
        let now = now_ms();
        if track.segments.iter().all(|s| s.points.is_empty()) {
            self.storage.delete_track(id, now)?;
            return Ok(None);
        }
        let stats = compute_stats(track, &StatsParams::for_activity(activity));
        self.storage.set_track_stats(id, &stats, now)?;
        self.storage
            .set_track_state(id, TrackState::Finished, now)?;
        Ok(Some(self.storage.track(id)?))
    }
}

struct SinkAdapter(Arc<Shared>);

impl FixSink for SinkAdapter {
    fn on_fix(&self, fix: Fix) {
        self.0.handle_fix(fix);
    }

    fn on_status(&self, status: SourceStatus) {
        self.0.listener.on_source_status(&status);
    }
}

impl App {
    /// Opens the database at `db_path`, creating or migrating it as needed.
    /// Map regions are kept in the directory `regions` next to the database,
    /// definitions of regions built on this device in `region-defs`.
    pub fn open(db_path: &Path, listener: Arc<dyn AppListener>) -> Result<Self, AppError> {
        let data_dir = db_path.parent().unwrap_or(Path::new("."));
        let regions_dir = data_dir.join("regions");
        let definitions_dir = data_dir.join("region-defs");
        let inner = Inner {
            storage: Storage::open(db_path)?,
            last_fix: None,
            recording: None,
        };
        Ok(Self {
            shared: Arc::new(Shared {
                inner: Mutex::new(inner),
                maps: crate::regions::Maps::open(&regions_dir, &definitions_dir)?,
                routing: crate::routes::Routing::default(),
                source: Mutex::new(None),
                listener,
                guide: Mutex::new(None),
                guide_turn: Mutex::new(()),
                guide_told: Mutex::new(()),
                guide_generation: AtomicU64::new(0),
            }),
        })
    }

    // --- location ---------------------------------------------------------

    /// Replaces the position source and starts it.
    pub fn set_location_source(&self, mut source: Box<dyn LocationSource>) -> Result<(), AppError> {
        self.stop_location_source();
        source.start(
            LocationConfig::default(),
            Arc::new(SinkAdapter(Arc::clone(&self.shared))),
        )?;
        *self.source() = Some(source);
        Ok(())
    }

    pub fn stop_location_source(&self) {
        let source = self.source().take();
        if let Some(mut source) = source {
            source.stop();
        }
    }

    fn source(&self) -> MutexGuard<'_, Option<Box<dyn LocationSource>>> {
        self.shared
            .source
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Entry point for fixes that do not come from a [`LocationSource`], such as
    /// a native location service on a mobile platform.
    pub fn handle_fix(&self, fix: Fix) {
        self.shared.handle_fix(fix);
    }

    pub fn last_position(&self) -> Option<Fix> {
        self.shared.inner().last_fix
    }

    // --- recording --------------------------------------------------------

    pub fn start_recording(&self, activity: Activity) -> Result<RecordingStatus, AppError> {
        let status = {
            let mut inner = self.shared.inner();
            if inner.recording.is_some() {
                return Err(AppError::RecordingActive);
            }
            // A recording left open by a crash would stay open forever once a
            // new one exists; keep it as a finished track.
            if let Some(open) = inner.storage.open_recording()? {
                let track = inner.storage.load_track(open.id)?;
                inner.finish(open.id, &track, open.activity)?;
            }
            let id = inner
                .storage
                .create_track(None, activity, TrackState::Recording, now_ms())?;
            let rec = inner.recording.insert(ActiveRecording::new(
                id,
                activity,
                Recorder::start(activity),
            ));
            rec.status()
        };
        self.shared.listener.on_recording(&status);
        Ok(status)
    }

    pub fn pause_recording(&self) -> Result<RecordingStatus, AppError> {
        self.change_state(TrackState::Paused, Recorder::pause)
    }

    pub fn resume_recording(&self) -> Result<RecordingStatus, AppError> {
        self.change_state(TrackState::Recording, Recorder::resume)
    }

    fn change_state(
        &self,
        state: TrackState,
        transition: fn(&mut Recorder) -> Result<(), core_track::TrackError>,
    ) -> Result<RecordingStatus, AppError> {
        let status = {
            let mut inner = self.shared.inner();
            let inner = &mut *inner;
            let rec = inner.recording.as_mut().ok_or(AppError::NoRecording)?;
            transition(&mut rec.recorder)?;
            inner
                .storage
                .set_track_state(rec.track_id, state, now_ms())?;
            rec.status()
        };
        self.shared.listener.on_recording(&status);
        Ok(status)
    }

    /// Ends the recording. Returns the finished track, or `None` if nothing was
    /// recorded (the empty track is discarded).
    pub fn stop_recording(&self) -> Result<Option<TrackRecord>, AppError> {
        let (record, last_points) = {
            let mut inner = self.shared.inner();
            let mut rec = inner.recording.take().ok_or(AppError::NoRecording)?;
            rec.recorder.stop()?;
            let record = inner.finish(rec.track_id, rec.recorder.track(), rec.activity)?;
            (record, std::mem::take(&mut rec.pending))
        };
        self.shared.listener.on_recording(&RecordingStatus {
            appended: last_points,
            ..IDLE
        });
        Ok(record)
    }

    pub fn recording_status(&self) -> RecordingStatus {
        let mut inner = self.shared.inner();
        match &mut inner.recording {
            // Do not consume the points still owed to the listener.
            Some(rec) => {
                let pending = rec.pending.clone();
                let status = rec.status();
                rec.pending = pending;
                RecordingStatus {
                    appended: Vec::new(),
                    ..status
                }
            }
            None => IDLE,
        }
    }

    /// Geometry of the track being recorded: `[lon, lat]` positions per segment.
    pub fn recording_line(&self) -> Vec<Vec<[f64; 2]>> {
        self.shared
            .inner()
            .recording
            .as_ref()
            .map(|rec| line_of(rec.recorder.track()))
            .unwrap_or_default()
    }

    /// A recording that was in progress when the app last ended, if any.
    pub fn interrupted_recording(&self) -> Result<Option<TrackRecord>, AppError> {
        let inner = self.shared.inner();
        let open = inner.storage.open_recording()?;
        Ok(open.filter(|t| {
            inner
                .recording
                .as_ref()
                .is_none_or(|rec| rec.track_id != t.id)
        }))
    }

    /// Continues the interrupted recording in a new segment.
    pub fn continue_interrupted(&self) -> Result<RecordingStatus, AppError> {
        let status = {
            let mut inner = self.shared.inner();
            if inner.recording.is_some() {
                return Err(AppError::RecordingActive);
            }
            let record = inner
                .storage
                .open_recording()?
                .ok_or(AppError::NothingInterrupted)?;
            let track = inner.storage.load_track(record.id)?;
            inner
                .storage
                .set_track_state(record.id, TrackState::Recording, now_ms())?;
            let recorder = Recorder::continue_from(track, record.activity);
            let rec =
                inner
                    .recording
                    .insert(ActiveRecording::new(record.id, record.activity, recorder));
            rec.status()
        };
        self.shared.listener.on_recording(&status);
        Ok(status)
    }

    /// Finishes the interrupted recording with the points it has.
    pub fn finish_interrupted(&self) -> Result<Option<TrackRecord>, AppError> {
        let mut inner = self.shared.inner();
        let open = inner.storage.open_recording()?;
        let record = open
            .filter(|t| {
                inner
                    .recording
                    .as_ref()
                    .is_none_or(|rec| rec.track_id != t.id)
            })
            .ok_or(AppError::NothingInterrupted)?;
        let track = inner.storage.load_track(record.id)?;
        inner.finish(record.id, &track, record.activity)
    }

    // --- tracks -----------------------------------------------------------

    /// The finished tracks; the one being recorded and an interrupted one are not among them.
    pub fn list_tracks(&self) -> Result<Vec<TrackRecord>, AppError> {
        Ok(self.shared.inner().storage.list_tracks()?)
    }

    pub fn track_detail(&self, id: Uuid) -> Result<TrackDetail, AppError> {
        let (record, track) = {
            let inner = self.shared.inner();
            (inner.storage.track(id)?, inner.storage.load_track(id)?)
        };
        let dem_stats = self.dem_stats(&track, record.activity)?;
        Ok(TrackDetail {
            record,
            line: line_of(&track),
            dem_stats,
        })
    }

    /// Statistics of a track with its elevations taken from the terrain model.
    fn dem_stats(&self, track: &Track, activity: Activity) -> Result<Option<TrackStats>, AppError> {
        let store = self.shared.maps.store();
        let mut corrected = track.clone();
        let (mut total, mut known) = (0usize, 0usize);
        for point in corrected.segments.iter_mut().flat_map(|s| &mut s.points) {
            point.ele = store.elevation(point.lat, point.lon)?.map(f64::from);
            total += 1;
            known += usize::from(point.ele.is_some());
        }
        if total == 0 || (known as f64) < DEM_MIN_COVERAGE * total as f64 {
            return Ok(None);
        }
        let base = StatsParams::for_activity(activity);
        let params = StatsParams {
            ele_smooth_m: base.ele_smooth_m * DEM_SMOOTH_FACTOR,
            ..base
        };
        Ok(Some(compute_stats(&corrected, &params)))
    }

    /// Sets the activity of a finished track, such as the one chosen after
    /// recording, and computes its statistics anew with that activity's values.
    pub fn set_track_activity(
        &self,
        id: Uuid,
        activity: Activity,
    ) -> Result<TrackRecord, AppError> {
        let mut inner = self.shared.inner();
        if inner.recording_track() == Some(id) {
            return Err(AppError::RecordingActive);
        }
        let track = inner.storage.load_track(id)?;
        let stats = compute_stats(&track, &StatsParams::for_activity(activity));
        inner
            .storage
            .set_track_activity(id, activity, &stats, now_ms())?;
        Ok(inner.storage.track(id)?)
    }

    pub fn rename_track(&self, id: Uuid, name: &str) -> Result<(), AppError> {
        Ok(self
            .shared
            .inner()
            .storage
            .rename_track(id, name, now_ms())?)
    }

    /// Deletes a track; not the one being recorded, which would be lost on stopping.
    pub fn delete_track(&self, id: Uuid) -> Result<(), AppError> {
        let mut inner = self.shared.inner();
        if inner.recording_track() == Some(id) {
            return Err(AppError::RecordingActive);
        }
        Ok(inner.storage.delete_track(id, now_ms())?)
    }

    /// Imports every track of a GPX document. `activity` overrides the type named
    /// in the file and is required if the file names none.
    pub fn import_gpx(
        &self,
        xml: &str,
        activity: Option<Activity>,
    ) -> Result<Vec<TrackRecord>, AppError> {
        let doc = parse_gpx(xml)?;
        let activities: Vec<Activity> = doc
            .tracks
            .iter()
            .map(|t| activity.or(t.activity).ok_or(AppError::ActivityRequired))
            .collect::<Result<_, _>>()?;
        let mut inner = self.shared.inner();
        let mut records = Vec::new();
        for (track, activity) in doc.tracks.iter().zip(activities) {
            let stats = compute_stats(track, &StatsParams::for_activity(activity));
            let id = inner
                .storage
                .import_track(track, activity, &stats, now_ms())?;
            records.push(inner.storage.track(id)?);
        }
        Ok(records)
    }

    /// Serialises a stored track as GPX 1.1.
    pub fn export_gpx(&self, id: Uuid) -> Result<String, AppError> {
        let track = self.shared.inner().storage.load_track(id)?;
        Ok(write_gpx(&GpxDocument {
            name: track.name.clone(),
            waypoints: Vec::new(),
            tracks: vec![track],
        })?)
    }
}

impl Drop for App {
    fn drop(&mut self) {
        // The source's thread holds a reference to the shared state; stop it so
        // both can go away.
        self.stop_location_source();
    }
}
