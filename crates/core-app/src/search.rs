//! Offline search over all enabled regions.

use core_maps::SearchSource;
use core_routing::LatLon;
use core_search::{Category, Hit, SearchError, SearchIndex, category_for_word, normalize};
use serde::Serialize;

use crate::AppError;
use crate::app::App;

/// Shorter queries would match nearly everything.
const MIN_QUERY_CHARS: usize = 2;
const MAX_LIMIT: usize = 100;
/// The same thing from two overlapping regions lies at most this far apart.
const SAME_PLACE_M: f64 = 30.0;

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
    /// From the point the search was made around.
    pub distance_m: f64,
}

/// What a search found.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    /// Names of the enabled regions that could not be searched.
    pub unsearchable: Vec<String>,
}

impl App {
    /// Finds places, streets and POIs by name around `near`. A query that is
    /// one word for a kind of place also finds the nearest of that category.
    #[must_use]
    pub fn search(&self, text: &str, near: LatLon, limit: usize) -> SearchResults {
        if normalize(text).chars().count() < MIN_QUERY_CHARS {
            return SearchResults::default();
        }
        let limit = limit.clamp(1, MAX_LIMIT);
        let category = category_for_word(text);
        self.search_regions(near, limit, |index, at| {
            let mut hits = index.search(text, Some(at), limit)?;
            if let Some(category) = category {
                hits.extend(index.by_category(category, at, limit)?);
            }
            Ok(hits)
        })
    }

    /// The nearest entries of a category, by its `Category::as_str` name.
    pub fn search_category(
        &self,
        category: &str,
        near: LatLon,
        limit: usize,
    ) -> Result<SearchResults, AppError> {
        let parsed =
            Category::parse(category).ok_or_else(|| AppError::SearchCategory(category.into()))?;
        let limit = limit.clamp(1, MAX_LIMIT);
        Ok(self.search_regions(near, limit, |index, at| {
            index.by_category(parsed, at, limit)
        }))
    }

    /// Runs `query` on every enabled region and merges the answers. An index
    /// is opened for one search only, so none stays open on a replaced file.
    fn search_regions(
        &self,
        near: LatLon,
        limit: usize,
        query: impl Fn(&SearchIndex, (f64, f64)) -> Result<Vec<Hit>, SearchError>,
    ) -> SearchResults {
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
        hits.sort_by(|a, b| {
            a.match_rank.cmp(&b.match_rank).then(
                a.distance_m
                    .unwrap_or(0.0)
                    .total_cmp(&b.distance_m.unwrap_or(0.0)),
            )
        });
        let mut kept: Vec<Hit> = Vec::new();
        for hit in hits {
            if kept.len() == limit {
                break;
            }
            if !kept.iter().any(|other| same_thing(other, &hit)) {
                kept.push(hit);
            }
        }
        SearchResults {
            hits: kept.into_iter().map(shown).collect(),
            unsearchable,
        }
    }
}

fn same_thing(a: &Hit, b: &Hit) -> bool {
    a.kind == b.kind
        && a.category == b.category
        && normalize(&a.name) == normalize(&b.name)
        && core_search::haversine_m((a.lat, a.lon), (b.lat, b.lon)) <= SAME_PLACE_M
}

fn shown(hit: Hit) -> SearchHit {
    SearchHit {
        kind: hit.kind.as_str().to_string(),
        category: hit.category.map(|c| c.as_str().to_string()),
        name: hit.name,
        town: hit.town,
        lat: hit.lat,
        lon: hit.lon,
        distance_m: hit.distance_m.unwrap_or(0.0),
    }
}
