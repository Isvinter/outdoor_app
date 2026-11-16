//! What the home-network share hands out: the catalog of the installed
//! regions, their files and the saved routes.

use std::path::PathBuf;

use core_maps::{Catalog, CatalogEntry};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppError;
use crate::app::App;

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
