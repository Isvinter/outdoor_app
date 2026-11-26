//! The core of the app as the mobile apps see it, through UniFFI.
//!
//! A thin wrapper around [`core_app::App`]: types the bindings can carry,
//! errors as stable codes. No logic of its own.

use std::fmt;
use std::path::Path;
use std::sync::Arc;

use core_app::{App, AppError, AppListener, GuideStatus, RecordingStatus};
use core_location::{Fix, SourceStatus};
use core_maps::{Contours, Layer, StyleOptions, TilePath};

mod guide;
mod plan;
mod search;
mod share;
mod track;

pub use guide::{GuidanceInfo, TurnInfo};
pub use plan::{
    ElevationSample, EngineFailure, LatLon, LegInput, OptionInfo, OptionValue, PlanInput, PlanLeg,
    PlanResult, PlanTotals, ProfileChoice, SavedRoute, StageSummary, ValhallaEngine,
    failed_leg_color, leg_can_connect_straight, plan_add_waypoint, plan_empty, plan_move_waypoint,
    plan_remove_waypoint, plan_set_last_leg, plan_set_leg, plan_split_leg, plan_toggle_stage_end,
};
pub use search::{KnownPosition, RouteStart, SearchHit, SearchResults};
pub use share::{ImportedRoute, RegionEntry, RegionList, RemoteRouteEntry};
pub use track::{
    FixInput, LineVertex, RecordingInfo, RestSettings, TrackSection, TrackSummary, TrackView,
};

uniffi::setup_scaffolding!();

/// What the app is told about from inside the core.
#[uniffi::export(with_foreign)]
pub trait CoreListener: Send + Sync {
    /// A background operation failed.
    fn on_error(&self, message: String);
    /// Download progress of a region, in bytes.
    fn on_region_progress(&self, region: String, done: u64, total: u64);
    /// The recording changed: its state with the points added since the last call.
    fn on_recording(&self, status: RecordingInfo);
    /// The guidance changed: its status on start and whenever a fix changes
    /// it; `None` when it ends. Calls come in the order of the changes, under
    /// an internal lock: do not call into the guidance synchronously from
    /// inside it, not even `guidance_status` (it may deadlock with another
    /// thread); hand the status over to another thread instead.
    fn on_guidance(&self, status: Option<GuidanceInfo>);
}

/// A failure, with a stable code for a translated message.
#[derive(Debug, uniffi::Error)]
pub enum CoreError {
    Failed { code: String, message: String },
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let CoreError::Failed { code, message } = self;
        write!(f, "{code}: {message}")
    }
}

impl std::error::Error for CoreError {}

impl From<AppError> for CoreError {
    fn from(error: AppError) -> Self {
        CoreError::Failed {
            code: error.code().into(),
            message: error.to_string(),
        }
    }
}

impl From<core_maps::MapsError> for CoreError {
    fn from(error: core_maps::MapsError) -> Self {
        CoreError::Failed {
            code: error.code().into(),
            message: error.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TileData {
    pub data: Vec<u8>,
    pub mime: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RegionSummary {
    pub id: String,
    pub name: String,
    pub version: String,
    pub enabled: bool,
    /// Space the region takes on disk.
    pub bytes: u64,
}

/// The installed regions.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MapOverview {
    pub regions: Vec<RegionSummary>,
    /// West, south, east, north; `None` if nothing is installed.
    pub bounds: Option<Vec<f64>>,
    pub attribution: Vec<String>,
}

/// Hands the core's notifications to the app.
struct Listener(Arc<dyn CoreListener>);

impl AppListener for Listener {
    fn on_position(&self, _fix: &Fix) {}
    fn on_recording(&self, status: &RecordingStatus) {
        self.0.on_recording(track::recording_info(status));
    }
    fn on_guidance(&self, status: Option<&GuideStatus>) {
        self.0.on_guidance(status.map(guide::guidance_info));
    }
    fn on_source_status(&self, _status: &SourceStatus) {}
    fn on_error(&self, message: &str) {
        self.0.on_error(message.into());
    }
    fn on_region_progress(&self, progress: &core_maps::Progress) {
        self.0
            .on_region_progress(progress.region.clone(), progress.done, progress.total);
    }
}

#[derive(uniffi::Object)]
pub struct CoreApp {
    pub(crate) app: App,
}

#[uniffi::export]
impl CoreApp {
    /// Opens the core with its data in `data_dir` (database `outdoor.sqlite`,
    /// regions in `regions/`), creating what is missing.
    #[uniffi::constructor]
    pub fn open(data_dir: String, listener: Arc<dyn CoreListener>) -> Result<Arc<Self>, CoreError> {
        let db = Path::new(&data_dir).join("outdoor.sqlite");
        let app = App::open(&db, Arc::new(Listener(listener)))?;
        Ok(Arc::new(CoreApp { app }))
    }

    pub fn overview(&self) -> Result<MapOverview, CoreError> {
        let overview = self.app.map_overview();
        Ok(MapOverview {
            regions: overview
                .regions
                .into_iter()
                .filter_map(|region| {
                    region.installed_version.map(|version| RegionSummary {
                        id: region.id,
                        name: region.name,
                        version,
                        enabled: region.enabled,
                        bytes: region.size,
                    })
                })
                .collect(),
            bounds: overview.installed_bounds.map(Vec::from),
            attribution: overview.attribution,
        })
    }

    /// Installs a region package from a directory with `manifest.json` and its
    /// files. The files are verified and moved; a broken package is left as it is.
    pub fn install_local_region(&self, package_dir: String) -> Result<RegionSummary, CoreError> {
        let region = self.app.install_local_region(Path::new(&package_dir))?;
        Ok(RegionSummary {
            id: region.manifest.id,
            name: region.manifest.name,
            version: region.manifest.version,
            enabled: region.enabled,
            bytes: region.bytes,
        })
    }

    /// A map tile, `path` like `/basemap/14/8700/5720`; `None` if no region has it.
    pub fn tile(&self, path: String) -> Result<Option<TileData>, CoreError> {
        let tile: TilePath = path.parse().map_err(|()| CoreError::Failed {
            code: "tile_path".into(),
            message: format!("not a tile path: {path}"),
        })?;
        let found = self.app.map_tile(tile.layer, tile.z, tile.x, tile.y)?;
        Ok(found.map(|tile| TileData {
            data: tile.data,
            mime: tile.mime.into(),
        }))
    }

    /// A style of `styles/` filled for the installed regions, with tile and
    /// font URLs under `tiles_base`. Contour lines come from the packages;
    /// without any, the style has none. `dark` gives the black OLED variant.
    pub fn offline_style(
        &self,
        style_json: String,
        tiles_base: String,
        dark: bool,
    ) -> Result<String, CoreError> {
        let overview = self.app.map_overview();
        let options = StyleOptions {
            tiles_base: &tiles_base,
            bounds: overview.installed_bounds,
            attribution: &overview.attribution,
            appearance: if dark {
                core_maps::Appearance::Dark
            } else {
                core_maps::Appearance::Light
            },
            contours: if self.app.map_has_layer(Layer::Contours) {
                Contours::Tiles
            } else {
                Contours::Omit
            },
        };
        Ok(core_maps::offline_style(&style_json, &options)?)
    }
}
