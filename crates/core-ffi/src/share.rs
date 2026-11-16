//! The region manager and the take-over of routes, for the apps.
//!
//! Type conversion only, plus the normalisation of the catalog address, which
//! only the phone needs (the desktop also accepts directories).

use core_app::{ImportOutcome, RegionInfo, RegionState, RemoteRoute, RemoteRouteState};

use crate::{CoreApp, CoreError, RegionSummary};

/// A region of the list, installed or on offer. Regions that are only defined
/// on the device and not built are left out.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RegionEntry {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub version: String,
    pub installed_version: Option<String>,
    /// `available`, `installed`, `update_available` or `downloading`.
    pub state: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RegionList {
    /// The catalog address; empty if none is set.
    pub source: String,
    /// Why the catalog could not be read; installed regions are listed anyway.
    pub catalog_error: Option<String>,
    pub regions: Vec<RegionEntry>,
}

/// A route the desktop offers.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RemoteRouteEntry {
    pub id: String,
    pub name: String,
    pub profile: String,
    pub distance_m: f64,
    pub ascent_m: Option<f64>,
    pub rev: i64,
    /// `new`, `up_to_date`, `desktop_newer`, `changed_here` or `deleted_here`.
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ImportedRoute {
    /// `added`, `replaced`, `restored` or `copied`.
    pub outcome: String,
    pub id: String,
}

fn entry(region: RegionInfo) -> Option<RegionEntry> {
    let state = match region.state {
        RegionState::NotBuilt => return None,
        RegionState::Available => "available",
        RegionState::Installed => "installed",
        RegionState::UpdateAvailable => "update_available",
        RegionState::Downloading => "downloading",
    };
    Some(RegionEntry {
        id: region.id,
        name: region.name,
        size: region.size,
        version: region.version,
        installed_version: region.installed_version,
        state: state.into(),
        enabled: region.enabled,
    })
}

fn remote_entry(route: RemoteRoute) -> RemoteRouteEntry {
    let state = match route.state {
        RemoteRouteState::New => "new",
        RemoteRouteState::UpToDate => "up_to_date",
        RemoteRouteState::DesktopNewer => "desktop_newer",
        RemoteRouteState::ChangedHere => "changed_here",
        RemoteRouteState::DeletedHere => "deleted_here",
    };
    RemoteRouteEntry {
        id: route.info.id.to_string(),
        name: route.info.name,
        profile: route.info.profile,
        distance_m: route.info.distance_m,
        ascent_m: route.info.ascent_m,
        rev: route.info.rev,
        state: state.into(),
    }
}

/// `host:port` becomes `http://host:port/`; an address ending in `.json` is
/// taken as it is. Blank means no source.
fn normalise_source(source: &str) -> String {
    let source = source.trim();
    if source.is_empty() {
        return String::new();
    }
    let mut out = if source.contains("://") {
        source.to_string()
    } else {
        format!("http://{source}")
    };
    if !out.ends_with('/') && !out.ends_with(".json") {
        out.push('/');
    }
    out
}

#[uniffi::export]
impl CoreApp {
    pub fn catalog_source(&self) -> Result<String, CoreError> {
        Ok(self.app.catalog_source()?)
    }

    /// Stores the address of the catalog (and the desktop's share) and
    /// returns what was stored.
    pub fn set_catalog_source(&self, source: String) -> Result<String, CoreError> {
        let source = normalise_source(&source);
        self.app.set_catalog_source(&source)?;
        Ok(source)
    }

    /// The regions on the device and in the catalog. Reads the catalog and
    /// blocks until it is read or has failed.
    pub fn regions(&self) -> Result<RegionList, CoreError> {
        let overview = self.app.regions()?;
        Ok(RegionList {
            source: overview.source,
            catalog_error: overview.catalog_error,
            regions: overview.regions.into_iter().filter_map(entry).collect(),
        })
    }

    /// Downloads and installs a region of the catalog. Blocks; progress goes
    /// to the listener.
    pub fn install_region(&self, id: String) -> Result<RegionSummary, CoreError> {
        let region = self.app.install_region(&id)?;
        Ok(RegionSummary {
            id: region.manifest.id,
            name: region.manifest.name,
            version: region.manifest.version,
            enabled: region.enabled,
            bytes: region.bytes,
        })
    }

    pub fn cancel_region_download(&self, id: String) {
        self.app.cancel_region_download(&id);
    }

    pub fn delete_region(&self, id: String) -> Result<(), CoreError> {
        Ok(self.app.delete_region(&id)?)
    }

    pub fn set_region_enabled(&self, id: String, enabled: bool) -> Result<(), CoreError> {
        Ok(self.app.set_region_enabled(&id, enabled)?)
    }

    /// The routes the desktop offers, with their state here. Blocks.
    pub fn remote_routes(&self) -> Result<Vec<RemoteRouteEntry>, CoreError> {
        Ok(self
            .app
            .remote_routes()?
            .into_iter()
            .map(remote_entry)
            .collect())
    }

    /// Takes a route of the desktop over; the core applies the merge rule.
    pub fn import_route(&self, id: String) -> Result<ImportedRoute, CoreError> {
        let uuid = crate::plan::route_id(&id)?;
        let (outcome, id) = self.app.import_route(uuid)?;
        let outcome = match outcome {
            ImportOutcome::Added => "added",
            ImportOutcome::Replaced => "replaced",
            ImportOutcome::Restored => "restored",
            ImportOutcome::Copied => "copied",
        };
        Ok(ImportedRoute {
            outcome: outcome.into(),
            id: id.to_string(),
        })
    }
}
