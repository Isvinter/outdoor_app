//! What the home-network share hands out: the catalog of the installed
//! regions, their files and the saved routes.

use std::path::PathBuf;

use core_maps::{Catalog, CatalogEntry, catalog_location, fetch_document};
use core_storage::SyncedRoute;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppError;
use crate::app::{App, now_ms};
use crate::plan::RoutePlan;

/// A saved route as listed by the share.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedRouteInfo {
    pub id: Uuid,
    pub name: String,
    pub profile: String,
    pub distance_m: f64,
    pub ascent_m: Option<f64>,
    pub updated_at_ms: i64,
    pub rev: i64,
}

/// A saved route with its plan, as the share hands it out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedRoute {
    #[serde(flatten)]
    pub info: SharedRouteInfo,
    /// The stored plan as it was saved.
    pub plan: serde_json::Value,
}

impl App {
    /// The catalog of the share: every complete installed region, enabled or
    /// not, with the region id as its path.
    pub fn shared_catalog(&self) -> Catalog {
        let store = self.shared.maps.store();
        let regions = store
            .installed()
            .into_iter()
            .filter(|region| store.is_complete(&region.manifest.id))
            .map(|region| CatalogEntry {
                path: region.manifest.id.clone(),
                id: region.manifest.id,
                name: region.manifest.name,
                bbox: region.manifest.bbox,
                version: region.manifest.version,
                size: region.manifest.size,
            })
            .collect();
        Catalog {
            format: core_maps::FORMAT,
            regions,
        }
    }

    /// A file of an installed region named in its manifest, with its length.
    pub fn shared_region_file(&self, id: &str, name: &str) -> Option<(PathBuf, u64)> {
        self.shared.maps.store().package_file(id, name)
    }

    /// The saved routes, without deleted ones.
    pub fn shared_routes(&self) -> Result<Vec<SharedRouteInfo>, AppError> {
        Ok(self
            .saved_routes()?
            .into_iter()
            .map(|record| SharedRouteInfo {
                id: record.id,
                name: record.name,
                profile: record.profile,
                distance_m: record.distance_m,
                ascent_m: record.ascent_m,
                updated_at_ms: record.updated_at_ms,
                rev: record.rev,
            })
            .collect())
    }

    /// A saved route with its stored plan. A deleted or unknown route is
    /// `StorageError::NotFound`.
    pub fn shared_route(&self, id: Uuid) -> Result<SharedRoute, AppError> {
        let info = self
            .shared_routes()?
            .into_iter()
            .find(|info| info.id == id)
            .ok_or(core_storage::StorageError::NotFound(id))?;
        let json = self.shared.inner().storage.route_plan(id)?;
        let plan = serde_json::from_str(&json).map_err(|e| AppError::RoutePlan(e.to_string()))?;
        Ok(SharedRoute { info, plan })
    }
}

/// How a route offered by the desktop relates to this device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteRouteState {
    /// Not on this device.
    New,
    UpToDate,
    /// Changed on the desktop since it was taken over, unchanged here.
    DesktopNewer,
    /// Changed on this device since it was taken over.
    ChangedHere,
    DeletedHere,
}

/// A route the desktop offers, with its state here.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RemoteRoute {
    pub info: SharedRouteInfo,
    pub state: RemoteRouteState,
}

/// What taking a route over did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportOutcome {
    Added,
    Replaced,
    Restored,
    /// Changed here: the desktop version was stored as a new route.
    Copied,
}

/// A route taken over from the desktop.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedRoute {
    pub outcome: ImportOutcome,
    /// The route on this device; a new one if it was copied.
    pub id: Uuid,
    /// The planned route, if taking over replaced the route that was loaded:
    /// the desktop's version, which "save" overwrites from now on.
    pub reloaded: Option<RoutePlan>,
}

impl App {
    /// The routes the desktop offers at the catalog source, with their state
    /// on this device. Reads over the network, which may take a while.
    pub fn remote_routes(&self) -> Result<Vec<RemoteRoute>, AppError> {
        let source = self.catalog_source()?;
        if source.is_empty() {
            return Err(AppError::NoCatalog);
        }
        let json = fetch_document(
            self.shared.maps.fetcher(),
            &catalog_location(&source, "routes.json"),
        )?;
        let list: Vec<SharedRouteInfo> = serde_json::from_str(&json)
            .map_err(|e| AppError::RouteImport(format!("route list: {e}")))?;
        // Asked before the storage is locked: it reads the saved plan itself.
        let edited = self
            .loaded_route()
            .filter(|&id| self.loaded_route_is_edited(id));
        let inner = self.shared.inner();
        list.into_iter()
            .map(|info| {
                let state = match inner.storage.route_sync(info.id)? {
                    None => RemoteRouteState::New,
                    Some(sync) if sync.deleted => RemoteRouteState::DeletedHere,
                    Some(sync) if sync.rev != sync.synced_rev => RemoteRouteState::ChangedHere,
                    Some(_) if edited == Some(info.id) => RemoteRouteState::ChangedHere,
                    Some(sync) if sync.source_rev == Some(info.rev) => RemoteRouteState::UpToDate,
                    Some(_) => RemoteRouteState::DesktopNewer,
                };
                Ok(RemoteRoute { info, state })
            })
            .collect()
    }

    /// Takes a route over from the desktop and says what happened.
    ///
    /// A route counts as changed here, and the desktop's version comes as a
    /// copy, also while it is loaded and planned differently than it is
    /// saved: replacing it would throw the unsaved planning away.
    pub fn import_route(&self, id: Uuid) -> Result<ImportedRoute, AppError> {
        let source = self.catalog_source()?;
        if source.is_empty() {
            return Err(AppError::NoCatalog);
        }
        let json = fetch_document(
            self.shared.maps.fetcher(),
            &catalog_location(&source, &format!("routes/{id}.json")),
        )?;
        let route: SharedRoute =
            serde_json::from_str(&json).map_err(|e| AppError::RouteImport(e.to_string()))?;
        if route.info.id != id {
            return Err(AppError::RouteImport(format!(
                "the file is for route {}, not {id}",
                route.info.id
            )));
        }
        crate::routes::check_plan(&route.plan).map_err(|e| AppError::RouteImport(e.to_string()))?;
        let plan_json =
            serde_json::to_string(&route.plan).map_err(|e| AppError::RouteImport(e.to_string()))?;
        let info = &route.info;
        let now = now_ms();
        let edited = self.loaded_route_is_edited(id);

        let (outcome, stored) = {
            let mut inner = self.shared.inner();
            let synced = SyncedRoute {
                name: &info.name,
                profile: &info.profile,
                distance_m: info.distance_m,
                ascent_m: info.ascent_m,
                plan_json: &plan_json,
                source_rev: info.rev,
            };
            match inner.storage.route_sync(id)? {
                None => {
                    inner.storage.put_synced_route(id, &synced, now)?;
                    (ImportOutcome::Added, id)
                }
                Some(sync) if sync.deleted => {
                    inner.storage.put_synced_route(id, &synced, now)?;
                    (ImportOutcome::Restored, id)
                }
                Some(sync) if sync.rev == sync.synced_rev && !edited => {
                    inner.storage.put_synced_route(id, &synced, now)?;
                    (ImportOutcome::Replaced, id)
                }
                Some(_) => {
                    let copy = inner.storage.save_route(
                        &format!("{} (Desktop)", info.name),
                        &info.profile,
                        info.distance_m,
                        info.ascent_m,
                        &plan_json,
                        now,
                    )?;
                    (ImportOutcome::Copied, copy)
                }
            }
        };
        // The planned route must follow the new version, or "save" would
        // write the old plan over it.
        let reloaded = if outcome != ImportOutcome::Copied && self.loaded_route() == Some(id) {
            Some(self.saved_route(id)?)
        } else {
            None
        };
        Ok(ImportedRoute {
            outcome,
            id: stored,
            reloaded,
        })
    }
}
