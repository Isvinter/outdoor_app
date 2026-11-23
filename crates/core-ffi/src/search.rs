//! Offline search and the start of a route to a result, for the mobile apps.

use crate::plan::{LatLon, PlanInput, request_from};
use crate::{CoreApp, CoreError};

/// One thing found.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SearchHit {
    /// "place", "street" or "poi".
    pub kind: String,
    /// "fuel", "lodging", …, for POIs.
    pub category: Option<String>,
    /// Empty for a POI without a name.
    pub name: String,
    pub town: Option<String>,
    pub lat: f64,
    pub lon: f64,
    pub distance_m: f64,
}

/// What a search found.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    /// Names of the enabled regions that could not be searched.
    pub unsearchable: Vec<String>,
}

/// The user's position and how old it is.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct KnownPosition {
    pub lat: f64,
    pub lon: f64,
    pub age_s: f64,
}

/// Where a route to a search result can start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum RouteStart {
    Ready,
    NoPosition,
    OtherMap,
}

fn results_to(results: core_app::SearchResults) -> SearchResults {
    SearchResults {
        hits: results
            .hits
            .into_iter()
            .map(|hit| SearchHit {
                kind: hit.kind,
                category: hit.category,
                name: hit.name,
                town: hit.town,
                lat: hit.lat,
                lon: hit.lon,
                distance_m: hit.distance_m,
            })
            .collect(),
        unsearchable: results.unsearchable,
    }
}

#[uniffi::export]
impl CoreApp {
    /// Finds places, streets and POIs around `near`. Call it off the main thread.
    pub fn search(&self, text: String, near: LatLon, limit: u32) -> SearchResults {
        results_to(self.app.search(&text, near.into(), limit as usize))
    }

    /// The nearest entries of a category. Call it off the main thread.
    pub fn search_category(
        &self,
        category: String,
        near: LatLon,
        limit: u32,
    ) -> Result<SearchResults, CoreError> {
        Ok(results_to(self.app.search_category(
            &category,
            near.into(),
            limit as usize,
        )?))
    }

    /// Whether a route to `target` can start at `position`.
    pub fn route_start(&self, position: Option<KnownPosition>, target: LatLon) -> RouteStart {
        let position = position.map(|p| core_app::KnownPosition {
            lat: p.lat,
            lon: p.lon,
            age_s: p.age_s,
        });
        match self.app.route_start(position, target.into()) {
            core_app::RouteStart::Ready => RouteStart::Ready,
            core_app::RouteStart::NoPosition => RouteStart::NoPosition,
            core_app::RouteStart::OtherMap => RouteStart::OtherMap,
        }
    }

    /// Whether `request` would be lost if replaced. A request the core cannot
    /// read counts as unsaved, so that the user is warned rather than not.
    pub fn route_is_unsaved(&self, request: PlanInput) -> bool {
        request_from(&request).map_or(true, |r| self.app.route_is_unsaved(&r))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn search_results_survive_the_bindings() {
        let results = results_to(core_app::SearchResults {
            hits: vec![core_app::SearchHit {
                kind: "poi".into(),
                category: Some("lodging".into()),
                name: "Hostel".into(),
                town: Some("Cusco".into()),
                lat: -13.5,
                lon: -72.0,
                distance_m: 350.0,
            }],
            unsearchable: vec!["Alt".into()],
        });
        let hit = results.hits.first().unwrap();
        assert_eq!(hit.kind, "poi");
        assert_eq!(hit.category.as_deref(), Some("lodging"));
        assert_eq!(hit.name, "Hostel");
        assert_eq!(hit.town.as_deref(), Some("Cusco"));
        assert_eq!((hit.lat, hit.lon, hit.distance_m), (-13.5, -72.0, 350.0));
        assert_eq!(results.unsearchable, ["Alt"]);
    }
}
