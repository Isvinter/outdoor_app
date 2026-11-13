//! Tauri commands: each one unpacks its arguments, calls the core and returns
//! the result. Commands are `async` so they run off the UI thread.

use core_app::{App, AppError, RecordingStatus, RegionsOverview, TrackDetail};
use core_gpx::parse_gpx;
use core_location::{GpsdSource, GpxReplaySource};
use core_maps::InstalledRegion;
use core_storage::TrackRecord;
use core_track::Activity;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

use crate::error::{CommandError, CommandResult};

/// Set to run the rendering benchmark on start and exit (see README).
const PROBE_ENV: &str = "OUTDOOR_FPS_PROBE";
/// Set to a GPX file to run the UI self-test on start and exit (see README).
const SELFTEST_ENV: &str = "OUTDOOR_SELFTEST";
/// Set to a PNG path to save a picture of the map after start and exit; with
/// `OUTDOOR_SNAPSHOT_VIEW=<lon>,<lat>,<zoom>` the map shows that place first.
const SNAPSHOT_ENV: &str = "OUTDOOR_SNAPSHOT";
const SNAPSHOT_VIEW_ENV: &str = "OUTDOOR_SNAPSHOT_VIEW";
const DEFAULT_GPSD_ADDRESS: &str = "localhost:2947";

fn read_file(path: &str) -> CommandResult<String> {
    std::fs::read_to_string(path).map_err(|e| CommandError::io(path, &e))
}

/// Runs a long core operation (network, large files) on the blocking thread pool.
async fn blocking<T, F>(handle: AppHandle, operation: F) -> CommandResult<T>
where
    T: Send + 'static,
    F: FnOnce(&App) -> Result<T, AppError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || operation(&handle.state::<App>()))
        .await
        .map_err(|e| CommandError::task(&e))?
        .map_err(CommandError::from)
}

// --- map regions ------------------------------------------------------------

#[tauri::command]
pub async fn regions_overview(handle: AppHandle) -> CommandResult<RegionsOverview> {
    blocking(handle, |app| app.regions()).await
}

#[tauri::command]
pub async fn regions_set_source(app: State<'_, App>, source: String) -> CommandResult<()> {
    Ok(app.set_catalog_source(&source)?)
}

/// Returns when the region is installed; progress arrives as `regions:progress` events.
#[tauri::command]
pub async fn region_install(handle: AppHandle, id: String) -> CommandResult<InstalledRegion> {
    blocking(handle, move |app| app.install_region(&id)).await
}

#[tauri::command]
pub async fn region_cancel(app: State<'_, App>, id: String) -> CommandResult<()> {
    app.cancel_region_download(&id);
    Ok(())
}

#[tauri::command]
pub async fn region_delete(app: State<'_, App>, id: String) -> CommandResult<()> {
    Ok(app.delete_region(&id)?)
}

// --- recording ------------------------------------------------------------

#[tauri::command]
pub async fn recording_start(
    app: State<'_, App>,
    activity: Activity,
) -> CommandResult<RecordingStatus> {
    Ok(app.start_recording(activity)?)
}

#[tauri::command]
pub async fn recording_pause(app: State<'_, App>) -> CommandResult<RecordingStatus> {
    Ok(app.pause_recording()?)
}

#[tauri::command]
pub async fn recording_resume(app: State<'_, App>) -> CommandResult<RecordingStatus> {
    Ok(app.resume_recording()?)
}

#[tauri::command]
pub async fn recording_stop(app: State<'_, App>) -> CommandResult<Option<TrackRecord>> {
    Ok(app.stop_recording()?)
}

/// Everything the frontend needs to draw the recording after a (re)load.
#[derive(Serialize)]
pub struct RecordingState {
    status: RecordingStatus,
    line: Vec<Vec<[f64; 2]>>,
    interrupted: Option<TrackRecord>,
}

#[tauri::command]
pub async fn recording_state(app: State<'_, App>) -> CommandResult<RecordingState> {
    Ok(RecordingState {
        status: app.recording_status(),
        line: app.recording_line(),
        interrupted: app.interrupted_recording()?,
    })
}

#[tauri::command]
pub async fn recording_continue_interrupted(app: State<'_, App>) -> CommandResult<RecordingState> {
    let status = app.continue_interrupted()?;
    Ok(RecordingState {
        status,
        line: app.recording_line(),
        interrupted: None,
    })
}

#[tauri::command]
pub async fn recording_finish_interrupted(
    app: State<'_, App>,
) -> CommandResult<Option<TrackRecord>> {
    Ok(app.finish_interrupted()?)
}

// --- tracks ---------------------------------------------------------------

#[tauri::command]
pub async fn tracks_list(app: State<'_, App>) -> CommandResult<Vec<TrackRecord>> {
    Ok(app.list_tracks()?)
}

#[tauri::command]
pub async fn track_detail(app: State<'_, App>, id: Uuid) -> CommandResult<TrackDetail> {
    Ok(app.track_detail(id)?)
}

#[tauri::command]
pub async fn track_rename(app: State<'_, App>, id: Uuid, name: String) -> CommandResult<()> {
    Ok(app.rename_track(id, &name)?)
}

#[tauri::command]
pub async fn track_delete(app: State<'_, App>, id: Uuid) -> CommandResult<()> {
    Ok(app.delete_track(id)?)
}

#[tauri::command]
pub async fn track_import_gpx(
    app: State<'_, App>,
    path: String,
    activity: Option<Activity>,
) -> CommandResult<Vec<TrackRecord>> {
    Ok(app.import_gpx(&read_file(&path)?, activity)?)
}

#[tauri::command]
pub async fn track_export_gpx(app: State<'_, App>, id: Uuid, path: String) -> CommandResult<()> {
    let xml = app.export_gpx(id)?;
    std::fs::write(&path, xml).map_err(|e| CommandError::io(&path, &e))
}

// --- location -------------------------------------------------------------

/// Plays back the first track of a GPX file as if it were the GPS receiver.
#[tauri::command]
pub async fn location_start_replay(
    app: State<'_, App>,
    path: String,
    speed: f64,
) -> CommandResult<()> {
    let doc = parse_gpx(&read_file(&path)?).map_err(core_app::AppError::from)?;
    let points = doc
        .tracks
        .into_iter()
        .flat_map(|t| t.segments)
        .flat_map(|s| s.points)
        .collect();
    Ok(app.set_location_source(Box::new(GpxReplaySource::new(points, speed)))?)
}

#[tauri::command]
pub async fn location_start_gpsd(
    app: State<'_, App>,
    address: Option<String>,
) -> CommandResult<()> {
    let address = address.unwrap_or_else(|| DEFAULT_GPSD_ADDRESS.to_owned());
    Ok(app.set_location_source(Box::new(GpsdSource::new(address)))?)
}

#[tauri::command]
pub async fn location_stop(app: State<'_, App>) -> CommandResult<()> {
    app.stop_location_source();
    Ok(())
}

// --- diagnostics ----------------------------------------------------------

#[tauri::command]
pub fn probe_enabled() -> bool {
    std::env::var_os(PROBE_ENV).is_some()
}

/// Set to a catalog location to run the region self-test on start and exit (see README).
const SELFTEST_REGIONS_ENV: &str = "OUTDOOR_SELFTEST_REGIONS";

#[tauri::command]
pub fn selftest_regions() -> Option<String> {
    std::env::var(SELFTEST_REGIONS_ENV).ok()
}

#[tauri::command]
pub fn selftest_gpx() -> Option<String> {
    std::env::var(SELFTEST_ENV).ok()
}

#[tauri::command]
pub fn probe_report(app: tauri::AppHandle, report: serde_json::Value) {
    println!("{report}");
    app.exit(0);
}

/// Forwards frontend diagnostics to the terminal.
#[tauri::command]
pub fn ui_log(level: String, message: String) {
    eprintln!("[ui:{level}] {message}");
}

#[derive(Serialize)]
pub struct SnapshotRequest {
    path: String,
    view: Option<[f64; 3]>,
}

#[tauri::command]
pub fn snapshot_request() -> Option<SnapshotRequest> {
    let path = std::env::var(SNAPSHOT_ENV).ok()?;
    let view = std::env::var(SNAPSHOT_VIEW_ENV).ok().and_then(|value| {
        let numbers: Vec<f64> = value
            .split(',')
            .filter_map(|part| part.trim().parse().ok())
            .collect();
        <[f64; 3]>::try_from(numbers).ok()
    });
    Some(SnapshotRequest { path, view })
}

/// Writes the picture requested through `OUTDOOR_SNAPSHOT` and ends the app.
#[tauri::command]
pub fn snapshot_save(app: tauri::AppHandle, png: Vec<u8>) {
    let Ok(path) = std::env::var(SNAPSHOT_ENV) else {
        return;
    };
    let code = match std::fs::write(&path, &png) {
        Ok(()) if !png.is_empty() => 0,
        Ok(()) => {
            eprintln!("error: the map could not be captured");
            1
        }
        Err(e) => {
            eprintln!("error: {path}: {e}");
            1
        }
    };
    app.exit(code);
}
