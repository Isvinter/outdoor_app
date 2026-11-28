//! Offline search and the start of a route to a result, for the mobile apps.

use crate::plan::{LatLon, LegInput, PlanInput, leg_from, request_from};
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
    /// From the own position if one was used, else from the search point.
    pub distance_m: f64,
    /// As the crow flies from the point the search was made around.
    pub centre_distance_m: f64,
    /// Length of the way from the own position, if one was found.
    pub route_m: Option<f64>,
    /// Travel time of that way.
    pub route_s: Option<f64>,
    /// Routing ran, but found no way within its reach.
    pub no_route: bool,
    /// A place outside the radius; these come last.
    pub outside: bool,
}

/// What a search found.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    /// Names of the enabled regions that could not be searched.
    pub unsearchable: Vec<String>,
    /// The radius the search used, after its limits.
    pub radius_m: f64,
    /// The way was found without regard to ascent.
    pub ascent_ignored: bool,
}

/// Where search hits are measured from, and how one would travel there.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RouteFrom {
    pub position: KnownPosition,
    pub leg: LegInput,
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
                centre_distance_m: hit.centre_distance_m,
                route_m: hit.route_m,
                route_s: hit.route_s,
                no_route: hit.no_route,
                outside: hit.outside,
            })
            .collect(),
        unsearchable: results.unsearchable,
        radius_m: results.radius_m,
        ascent_ignored: results.ascent_ignored,
    }
}

/// An unreadable leg (unknown profile) counts as no route start at all.
fn route_from(from: Option<&RouteFrom>) -> Option<core_app::RouteFrom> {
    let from = from?;
    Some(core_app::RouteFrom {
        position: core_app::KnownPosition {
            lat: from.position.lat,
            lon: from.position.lon,
            age_s: from.position.age_s,
        },
        leg: leg_from(&from.leg).ok()?,
    })
}

#[uniffi::export]
impl CoreApp {
    /// Finds streets and POIs within `radius_m` of `near` and places anywhere.
    /// With `from`, hits are ranked and measured by the way from there.
    /// Call it off the main thread.
    pub fn search(
        &self,
        text: String,
        near: LatLon,
        radius_m: f64,
        limit: u32,
        from: Option<RouteFrom>,
    ) -> SearchResults {
        results_to(self.app.search_from(
            &text,
            near.into(),
            radius_m,
            limit as usize,
            route_from(from.as_ref()).as_ref(),
        ))
    }

    /// The nearest entries of a category within `radius_m`. Call it off the main thread.
    pub fn search_category(
        &self,
        category: String,
        near: LatLon,
        radius_m: f64,
        limit: u32,
        from: Option<RouteFrom>,
    ) -> Result<SearchResults, CoreError> {
        Ok(results_to(self.app.search_category_from(
            &category,
            near.into(),
            radius_m,
            limit as usize,
            route_from(from.as_ref()).as_ref(),
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
                distance_m: 3_200.0,
                centre_distance_m: 900.0,
                route_m: Some(3_200.0),
                route_s: Some(3_120.0),
                no_route: false,
                outside: true,
            }],
            unsearchable: vec!["Alt".into()],
            radius_m: 5_000.0,
            ascent_ignored: true,
        });
        assert!(results.ascent_ignored);
        assert_eq!(results.radius_m, 5_000.0);
        let hit = results.hits.first().unwrap();
        assert_eq!(hit.kind, "poi");
        assert_eq!(hit.category.as_deref(), Some("lodging"));
        assert_eq!(hit.name, "Hostel");
        assert_eq!(hit.town.as_deref(), Some("Cusco"));
        assert_eq!((hit.lat, hit.lon, hit.distance_m), (-13.5, -72.0, 3_200.0));
        assert_eq!(hit.centre_distance_m, 900.0);
        assert_eq!((hit.route_m, hit.route_s), (Some(3_200.0), Some(3_120.0)));
        assert!(!hit.no_route);
        assert!(hit.outside);
        assert_eq!(results.unsearchable, ["Alt"]);
    }

    struct Quiet;
    impl crate::CoreListener for Quiet {
        fn on_error(&self, _message: String) {}
        fn on_region_progress(&self, _region: String, _done: u64, _total: u64) {}
        fn on_recording(&self, _status: crate::RecordingInfo) {}
        fn on_guidance(&self, _status: Option<crate::GuidanceInfo>) {}
    }

    fn rocket() -> RouteFrom {
        RouteFrom {
            position: KnownPosition {
                lat: 48.0,
                lon: 11.0,
                age_s: 5.0,
            },
            leg: crate::plan::LegInput {
                profile: "rocket".into(),
                options: vec![],
            },
        }
    }

    #[test]
    fn unknown_profile_searches_without_routes() {
        assert_eq!(route_from(Some(&rocket())), None);
        let mut valid = rocket();
        valid.leg.profile = "hike".into();
        assert!(route_from(Some(&valid)).is_some());

        let dir = std::env::temp_dir().join(format!("core-ffi-search-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let core = CoreApp::open(dir.display().to_string(), std::sync::Arc::new(Quiet)).unwrap();
        let near = LatLon {
            lat: 48.0,
            lon: 11.0,
        };
        let with = core.search("Hof".into(), near, 5_000.0, 10, Some(rocket()));
        let without = core.search("Hof".into(), near, 5_000.0, 10, None);
        assert_eq!(with, without);
        let with = core.search_category("fuel".into(), near, 5_000.0, 10, Some(rocket()));
        let without = core.search_category("fuel".into(), near, 5_000.0, 10, None);
        assert_eq!(with.unwrap(), without.unwrap());
    }
}
