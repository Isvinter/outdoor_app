use core_app::{AppListener, RecordingStatus};
use core_location::{Fix, SourceStatus};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Forwards what happens in the core to the frontend as Tauri events.
pub struct EventForwarder {
    app: AppHandle,
}

impl EventForwarder {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }

    fn emit(&self, event: &str, payload: impl Serialize + Clone) {
        if let Err(e) = self.app.emit(event, payload) {
            eprintln!("error: cannot emit {event}: {e}");
        }
    }
}

impl AppListener for EventForwarder {
    fn on_position(&self, fix: &Fix) {
        self.emit("location:fix", *fix);
    }

    fn on_recording(&self, status: &RecordingStatus) {
        self.emit("recording:update", status.clone());
    }

    fn on_source_status(&self, status: &SourceStatus) {
        self.emit("location:status", status.clone());
    }

    fn on_error(&self, message: &str) {
        eprintln!("error: {message}");
        self.emit("app:error", message.to_owned());
    }
}
