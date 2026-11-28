//! Offline search over all enabled regions.

use std::cmp::Ordering;

use core_maps::SearchSource;
use core_routing::LatLon;
use core_search::{Category, Hit, Kind, SearchError, SearchIndex, category_for_word, normalize};
use serde::Serialize;

use crate::AppError;
use crate::app::App;
use crate::search_routes::{Chosen, RouteFrom};

/// Shorter queries would match nearly everything.
const MIN_QUERY_CHARS: usize = 2;
const MAX_LIMIT: usize = 100;
/// The same thing from two overlapping regions lies at most this far apart.
const SAME_PLACE_M: f64 = 30.0;
const MIN_RADIUS_M: f64 = 2_000.0;
const MAX_RADIUS_M: f64 = 50_000.0;
/// One in this many result slots is kept for places outside the radius.
const FAR_PLACE_SHARE: usize = 3;

/// One thing found, as the apps show it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SearchHit {
    /// `place`, `street` or `poi`.
    pub kind: String,
    /// As `Category::as_str`, for POIs.
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
    /// A place outside the radius; these come last, after everything inside.
    pub outside: bool,
}

/// What a search found.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    /// Names of the enabled regions that could not be searched.
    pub unsearchable: Vec<String>,
    /// The radius the search used, after its limits.
    pub radius_m: f64,
    /// Times come from a router that ignores the ascent (hiking).
    pub ascent_ignored: bool,
}

impl App {
    /// Finds streets and POIs by name within `radius_m` of `near` only, and
    /// places anywhere; places outside the radius come last, marked `outside`.
    /// A query that is one word for a kind of place also finds the nearest of
    /// that category.
    #[must_use]
    pub fn search(&self, text: &str, near: LatLon, radius_m: f64, limit: usize) -> SearchResults {
        self.search_from(text, near, radius_m, limit, None)
    }

    /// Like [`App::search`]; with a position in `from`, hits are measured
    /// from it and put in order by the way there.
    #[must_use]
    pub fn search_from(
        &self,
        text: &str,
        near: LatLon,
        radius_m: f64,
        limit: usize,
        from: Option<&RouteFrom>,
    ) -> SearchResults {
        if normalize(text).chars().count() < MIN_QUERY_CHARS {
            return SearchResults::default();
        }
        let limit = limit.clamp(1, MAX_LIMIT);
        let radius_m = usable_radius(radius_m);
        let category = category_for_word(text);
        let (hits, unsearchable) = self.search_regions(near, |index, at| {
            let mut hits = index.search_around(text, at, limit)?;
            if let Some(category) = category {
                hits.extend(index.by_category(category, at, limit)?);
            }
            Ok(hits)
        });
        let inside = |hit: &Hit| hit.distance_m.is_some_and(|d| d <= radius_m);
        let (main, far_places): (Vec<Hit>, Vec<Hit>) = hits
            .into_iter()
            .filter(|hit| hit.kind == Kind::Place || inside(hit))
            .partition(inside);
        let (hits, ascent_ignored) = self.measure_hits(share_limit(main, far_places, limit), from);
        SearchResults {
            hits,
            unsearchable,
            radius_m,
            ascent_ignored,
        }
    }

    /// The nearest entries of a category within `radius_m`, by its
    /// `Category::as_str` name.
    pub fn search_category(
        &self,
        category: &str,
        near: LatLon,
        radius_m: f64,
        limit: usize,
    ) -> Result<SearchResults, AppError> {
        self.search_category_from(category, near, radius_m, limit, None)
    }

    /// Like [`App::search_category`], measured and ordered from the position
    /// in `from` if there is one.
    pub fn search_category_from(
        &self,
        category: &str,
        near: LatLon,
        radius_m: f64,
        limit: usize,
        from: Option<&RouteFrom>,
    ) -> Result<SearchResults, AppError> {
        let parsed =
            Category::parse(category).ok_or_else(|| AppError::SearchCategory(category.into()))?;
        let limit = limit.clamp(1, MAX_LIMIT);
        let radius_m = usable_radius(radius_m);
        let (hits, unsearchable) = self.search_regions(near, |index, at| {
            index.by_category_within(parsed, at, radius_m, limit)
        });
        let chosen = hits
            .into_iter()
            .take(limit)
            .map(|hit| Chosen { hit, far: false })
            .collect();
        let (hits, ascent_ignored) = self.measure_hits(chosen, from);
        Ok(SearchResults {
            hits,
            unsearchable,
            radius_m,
            ascent_ignored,
        })
    }

    /// Runs `query` on every enabled region and merges the answers, best first
    /// and without duplicates. An index is opened for one search only, so none
    /// stays open on a replaced file.
    fn search_regions(
        &self,
        near: LatLon,
        query: impl Fn(&SearchIndex, (f64, f64)) -> Result<Vec<Hit>, SearchError>,
    ) -> (Vec<Hit>, Vec<String>) {
        let mut hits = Vec::new();
        let mut unsearchable = Vec::new();
        for SearchSource { name, path, .. } in self.shared.maps.store().search_sources() {
            let found = path.and_then(|path| {
                SearchIndex::open(&path)
                    .and_then(|index| query(&index, (near.lat, near.lon)))
                    .ok()
            });
            match found {
                Some(found) => hits.extend(found),
                None => unsearchable.push(name),
            }
        }
        hits.sort_by(best_first);
        let mut kept: Vec<Hit> = Vec::new();
        for hit in hits {
            if !kept.iter().any(|other| same_thing(other, &hit)) {
                kept.push(hit);
            }
        }
        (kept, unsearchable)
    }
}

/// The radius within its limits. Anything that is not a number searches widest.
fn usable_radius(radius_m: f64) -> f64 {
    if radius_m.is_finite() {
        radius_m.clamp(MIN_RADIUS_M, MAX_RADIUS_M)
    } else {
        MAX_RADIUS_M
    }
}

pub(crate) fn best_first(a: &Hit, b: &Hit) -> Ordering {
    a.match_rank.cmp(&b.match_rank).then(
        a.distance_m
            .unwrap_or(0.0)
            .total_cmp(&b.distance_m.unwrap_or(0.0)),
    )
}

/// Fills `limit` so that neither group can push the other out: far places get
/// up to a third, and whatever one group leaves free goes to the other. The
/// order is the one given, main hits first; the caller sorts.
fn share_limit(mut main: Vec<Hit>, mut far_places: Vec<Hit>, limit: usize) -> Vec<Chosen> {
    let reserved = far_places.len().min(limit / FAR_PLACE_SHARE);
    main.truncate(limit - reserved);
    far_places.truncate(limit - main.len());
    let far_start = main.len();
    main.append(&mut far_places);
    main.into_iter()
        .enumerate()
        .map(|(i, hit)| Chosen {
            hit,
            far: i >= far_start,
        })
        .collect()
}

fn same_thing(a: &Hit, b: &Hit) -> bool {
    a.kind == b.kind
        && a.category == b.category
        && core_search::haversine_m((a.lat, a.lon), (b.lat, b.lon)) <= SAME_PLACE_M
        && normalize(&a.name) == normalize(&b.name)
}

/// A hit as found: measured from the search point only.
pub(crate) fn shown(hit: Hit) -> SearchHit {
    let distance_m = hit.distance_m.unwrap_or(0.0);
    SearchHit {
        kind: hit.kind.as_str().to_string(),
        category: hit.category.map(|c| c.as_str().to_string()),
        name: hit.name,
        town: hit.town,
        lat: hit.lat,
        lon: hit.lon,
        distance_m,
        centre_distance_m: distance_m,
        route_m: None,
        route_s: None,
        no_route: false,
        outside: false,
    }
}
