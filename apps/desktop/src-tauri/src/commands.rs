//! Tauri commands: each one unpacks its arguments, calls the core and returns
//! the result. Commands are `async` so they run off the UI thread.

use core_app::{
    App, AppError, PlanRequest, ProfileInfo, RecordingStatus, RegionsOverview, RoutePlan,
    TrackDetail,
};
use core_gpx::parse_gpx;
use core_location::{GpsdSource, GpxReplaySource};
use core_maps::InstalledRegion;
use core_storage::{RouteRecord, TrackRecord};
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
/// `topo`, `winter` or `online`: the map style to switch to before the picture is taken.
const SNAPSHOT_STYLE_ENV: &str = "OUTDOOR_SNAPSHOT_STYLE";
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

// `region_delete` is in `build`: it also removes what a build left behind.

// --- routes -----------------------------------------------------------------

#[tauri::command]
pub async fn route_profiles(app: State<'_, App>) -> CommandResult<Vec<ProfileInfo>> {
    Ok(app.route_profiles())
}

/// Computes a route leg by leg; it becomes the planned route.
#[tauri::command]
pub async fn route_plan(handle: AppHandle, request: PlanRequest) -> CommandResult<RoutePlan> {
    blocking(handle, move |app| app.plan_route(&request)).await
}

#[tauri::command]
pub async fn route_clear(app: State<'_, App>) -> CommandResult<()> {
    app.clear_planned_route();
    Ok(())
}

#[tauri::command]
pub async fn route_save(app: State<'_, App>, name: String) -> CommandResult<RouteRecord> {
    Ok(app.save_planned_route(&name)?)
}

#[tauri::command]
pub async fn routes_list(app: State<'_, App>) -> CommandResult<Vec<RouteRecord>> {
    Ok(app.saved_routes()?)
}

/// Loads a saved route; it becomes the planned route.
#[tauri::command]
pub async fn route_load(app: State<'_, App>, id: Uuid) -> CommandResult<RoutePlan> {
    Ok(app.saved_route(id)?)
}

#[tauri::command]
pub async fn route_delete(app: State<'_, App>, id: Uuid) -> CommandResult<()> {
    Ok(app.delete_route(id)?)
}

/// Writes the planned route as GPX.
#[tauri::command]
pub async fn route_export_gpx(
    app: State<'_, App>,
    name: String,
    path: String,
) -> CommandResult<()> {
    let xml = app.export_planned_route_gpx(&name)?;
    std::fs::write(&path, xml).map_err(|e| CommandError::io(&path, &e))
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

/// Set to `<lat>,<lon>;<lat>,<lon>[;…]` to plan a hike between these points
/// through the user interface on start, report and exit (see README).
const SELFTEST_ROUTE_ENV: &str = "OUTDOOR_SELFTEST_ROUTE";

#[tauri::command]
pub fn selftest_route() -> Option<String> {
    std::env::var(SELFTEST_ROUTE_ENV).ok()
}

/// Replay speed of the recording self-test; 1 records in real time.
#[tauri::command]
pub fn selftest_speed() -> Option<f64> {
    std::env::var("OUTDOOR_SELFTEST_SPEED").ok()?.parse().ok()
}

/// Set to report how long the page took to start and exit (see README).
#[tauri::command]
pub fn startup_probe() -> bool {
    std::env::var_os("OUTDOOR_STARTUP_PROBE").is_some()
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
    style: Option<String>,
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
    let style = std::env::var(SNAPSHOT_STYLE_ENV).ok();
    Some(SnapshotRequest { path, view, style })
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
