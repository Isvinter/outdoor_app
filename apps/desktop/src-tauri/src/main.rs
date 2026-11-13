//! Desktop shell. Contains wiring only; all logic lives in the `core-*` crates.

use std::process::ExitCode;

/// Set to run the rendering benchmark on start and exit (see README).
const PROBE_ENV: &str = "OUTDOOR_FPS_PROBE";

#[tauri::command]
fn probe_enabled() -> bool {
    std::env::var_os(PROBE_ENV).is_some()
}

#[tauri::command]
fn probe_report(app: tauri::AppHandle, report: serde_json::Value) {
    println!("{report}");
    app.exit(0);
}

/// Forwards frontend diagnostics to the terminal.
#[tauri::command]
fn ui_log(level: String, message: String) {
    eprintln!("[ui:{level}] {message}");
}

fn main() -> ExitCode {
    let result = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            probe_enabled,
            probe_report,
            ui_log
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
