//! Desktop shell. Contains wiring only; all logic lives in the `core-*` crates.

mod build;
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
            let dir = data_dir(app)?;
            let core = App::open(&dir.join("outdoor.sqlite"), listener)?;
            app.manage(core);
            app.manage(build::Builds::new(&dir));
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
            build::region_delete,
            build::region_build_check,
            build::region_build_plan,
            build::region_build_start,
            build::region_build_cancel,
            build::selftest_build,
            commands::route_profiles,
            commands::route_plan,
            commands::route_clear,
            commands::route_save,
            commands::routes_list,
            commands::route_load,
            commands::route_delete,
            commands::route_export_gpx,
            commands::selftest_route,
            commands::selftest_speed,
            commands::startup_probe,
            commands::probe_enabled,
            commands::snapshot_request,
            commands::snapshot_save,
            commands::selftest_gpx,
            commands::selftest_regions,
            commands::probe_report,
            commands::ui_log,
        ])
        .build(tauri::generate_context!());
    match result {
        Ok(app) => {
            app.run(|handle, event| {
                if let tauri::RunEvent::ExitRequested { .. } = event {
                    build::shutdown(handle);
                }
            });
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
