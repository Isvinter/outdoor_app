//! Desktop shell. Contains wiring only; all logic lives in the `core-*` crates.

mod commands;
mod error;
mod events;
mod tiles;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use core_app::App;
use tauri::Manager;

/// Overrides the directory holding the database (default: the per-user app data directory).
const DATA_DIR_ENV: &str = "OUTDOOR_DATA_DIR";

fn data_dir(app: &tauri::App) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let dir = match std::env::var_os(DATA_DIR_ENV) {
        Some(dir) => PathBuf::from(dir),
        None => app.path().app_data_dir()?,
    };
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn main() -> ExitCode {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .register_asynchronous_uri_scheme_protocol(tiles::SCHEME, |context, request, responder| {
            // Tiles are read from disk; keep that off the UI thread.
            let handle = context.app_handle().clone();
            tauri::async_runtime::spawn_blocking(move || {
                responder.respond(tiles::respond(&handle, &request));
            });
        })
        .setup(|app| {
            let listener = Arc::new(events::EventForwarder::new(app.handle().clone()));
            let core = App::open(&data_dir(app)?.join("outdoor.sqlite"), listener)?;
            app.manage(core);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::recording_start,
            commands::recording_pause,
            commands::recording_resume,
            commands::recording_stop,
            commands::recording_state,
            commands::recording_continue_interrupted,
            commands::recording_finish_interrupted,
            commands::tracks_list,
            commands::track_detail,
            commands::track_rename,
            commands::track_delete,
            commands::track_import_gpx,
            commands::track_export_gpx,
            commands::location_start_replay,
            commands::location_start_gpsd,
            commands::location_stop,
            commands::regions_overview,
            commands::regions_set_source,
            commands::region_install,
            commands::region_cancel,
            commands::region_delete,
            commands::probe_enabled,
            commands::snapshot_request,
            commands::snapshot_save,
            commands::selftest_gpx,
            commands::selftest_regions,
            commands::probe_report,
            commands::ui_log,
        ])
        .run(tauri::generate_context!());
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
