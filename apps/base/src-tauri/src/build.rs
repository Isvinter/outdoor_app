//! Building regions chosen on the map: one build at a time, its progress as
//! `regions:build` events, cancel, and installing the result.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use core_app::App;
use core_maps::InstalledRegion;
use region_builder::{BuildError, BuildPlan, Builder, StepProgress};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::error::{CommandError, CommandResult};

/// Set to `<west>,<south>,<east>,<north>;<name>` to build a region through the
/// user interface on start, report and exit (see README).
const SELFTEST_BUILD_ENV: &str = "OUTDOOR_SELFTEST_BUILD";
/// How long closing the app waits for a cancelled build to stop.
const SHUTDOWN_WAIT: Duration = Duration::from_secs(15);

/// The build in progress, if any.
struct Running {
    progress: StepProgress,
    cancel: Arc<AtomicBool>,
}

pub struct Builds {
    builder: Builder,
    running: Mutex<Option<Running>>,
}

impl Builds {
    pub fn new(data_dir: &Path) -> Self {
        Builds {
            builder: Builder::new(data_dir),
            running: Mutex::default(),
        }
    }

    fn running(&self) -> MutexGuard<'_, Option<Running>> {
        self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn is_building(&self, id: &str) -> bool {
        self.running()
            .as_ref()
            .is_some_and(|running| running.progress.region == id)
    }
}

/// Marks a build as running for as long as it lives.
struct RunningGuard<'a>(&'a Builds);

impl Drop for RunningGuard<'_> {
    fn drop(&mut self) {
        *self.0.running() = None;
    }
}

#[derive(Serialize)]
pub struct BuildStatus {
    /// Codes of what this machine lacks for a build; empty if it can build.
    missing: Vec<String>,
    running: Option<StepProgress>,
}

async fn blocking<T, F>(handle: AppHandle, operation: F) -> CommandResult<T>
where
    T: Send + 'static,
    F: FnOnce(&AppHandle) -> CommandResult<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || operation(&handle))
        .await
        .map_err(|e| CommandError::task(&e))?
}

#[tauri::command]
pub async fn region_build_check(handle: AppHandle) -> CommandResult<BuildStatus> {
    blocking(handle, |handle| {
        let builds = handle.state::<Builds>();
        Ok(BuildStatus {
            missing: builds.builder.check(),
            running: builds.running().as_ref().map(|r| r.progress.clone()),
        })
    })
    .await
}

#[tauri::command]
pub async fn region_build_plan(handle: AppHandle, bbox: [f64; 4]) -> CommandResult<BuildPlan> {
    blocking(handle, move |handle| {
        Ok(handle.state::<Builds>().builder.plan(bbox)?)
    })
    .await
}

/// Builds a new region (`name` and `bbox`) or again one that is defined
/// (`id`), then installs it. Returns when the region is installed; steps
/// arrive as `regions:build` events.
#[tauri::command]
pub async fn region_build_start(
    handle: AppHandle,
    name: Option<String>,
    bbox: Option<[f64; 4]>,
    id: Option<String>,
    refresh: bool,
) -> CommandResult<InstalledRegion> {
    blocking(handle, move |handle| {
        let builds = handle.state::<Builds>();
        let app = handle.state::<App>();
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut running = builds.running();
            if running.is_some() {
                return Err(BuildError::Busy.into());
            }
            *running = Some(Running {
                progress: StepProgress {
                    region: id.clone().unwrap_or_default(),
                    step: String::new(),
                    index: 0,
                    total: 0,
                },
                cancel: Arc::clone(&cancel),
            });
        }
        let _guard = RunningGuard(&builds);

        let id = match (id, name, bbox) {
            (Some(id), _, _) => id,
            (None, Some(name), Some(bbox)) => {
                let taken: Vec<String> = app.regions()?.regions.into_iter().map(|r| r.id).collect();
                let definition = builds
                    .builder
                    .define(&name, bbox, &|id| taken.iter().any(|t| t == id))?;
                definition.id
            }
            _ => {
                return Err(BuildError::InvalidArea("neither a region nor an area".into()).into());
            }
        };
        let report = |progress: StepProgress| {
            if let Some(running) = builds.running().as_mut() {
                running.progress = progress.clone();
            }
            if let Err(e) = handle.emit("regions:build", progress) {
                eprintln!("error: cannot emit regions:build: {e}");
            }
        };
        report(StepProgress {
            region: id.clone(),
            step: String::new(),
            index: 0,
            total: 0,
        });
        let package = builds
            .builder
            .build(&id, refresh, &mut |step| report(step), &cancel)?;
        let installed = app.install_local_region(&package)?;
        builds.builder.cleanup(&id, false);
        Ok(installed)
    })
    .await
}

#[tauri::command]
pub async fn region_build_cancel(builds: State<'_, Builds>) -> CommandResult<()> {
    if let Some(running) = builds.running().as_ref() {
        running.cancel.store(true, Ordering::SeqCst);
    }
    Ok(())
}

/// Removes a region with everything a build left behind.
#[tauri::command]
pub async fn region_delete(handle: AppHandle, id: String) -> CommandResult<()> {
    blocking(handle, move |handle| {
        let builds = handle.state::<Builds>();
        if builds.is_building(&id) {
            return Err(BuildError::Busy.into());
        }
        handle.state::<App>().delete_region(&id)?;
        builds.builder.cleanup(&id, true);
        Ok(())
    })
    .await
}

#[tauri::command]
pub fn selftest_build() -> Option<String> {
    std::env::var(SELFTEST_BUILD_ENV).ok()
}

/// Cancels a running build and waits a while for it to stop, so that no
/// build tools keep running after the app is gone.
pub fn shutdown(handle: &AppHandle) {
    let Some(builds) = handle.try_state::<Builds>() else {
        return;
    };
    match builds.running().as_ref() {
        Some(running) => running.cancel.store(true, Ordering::SeqCst),
        None => return,
    }
    let started = Instant::now();
    while builds.running().is_some() && started.elapsed() < SHUTDOWN_WAIT {
        std::thread::sleep(Duration::from_millis(100));
    }
}
