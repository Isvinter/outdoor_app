//! Search hits measured and ordered by the way from the user's position.

use std::cmp::Ordering;

use core_routing::{LatLon, MatrixRequest, Profile};
use core_search::{Hit, haversine_m};

use crate::App;
use crate::plan::{KnownPosition, LegSpec, MAX_POSITION_AGE_S};
use crate::search::{SearchHit, best_first, shown};

/// Hits further than this from the position (as the crow flies) are not
/// routed: the router's search would not reach them cheaply.
pub const ROUTE_LIMIT_M: f64 = 50_000.0;

/// Where search hits are measured from, and how one would travel there.
#[derive(Debug, Clone, PartialEq)]
pub struct RouteFrom {
    pub position: KnownPosition,
    pub leg: LegSpec,
}

/// A hit chosen to be shown; `far` if it is a place outside the radius.
pub(crate) struct Chosen {
    pub hit: Hit,
    pub far: bool,
}

/// What a hit came to, in the order it sorts within one match quality.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Group {
    Routed,
    NoRoute,
    Unrouted,
}

struct Measured {
    hit: SearchHit,
    match_rank: u8,
    far: bool,
    group: Group,
    /// Route length for the routed, otherwise the air distance.
    key: f64,
}

impl App {
    /// Turns the chosen hits into what the apps show. With a usable position
    /// they are measured from it and put in order by the way there.
    /// Returns the hits and whether times ignore the ascent.
    pub(crate) fn measure_hits(
        &self,
        chosen: Vec<Chosen>,
        from: Option<&RouteFrom>,
    ) -> (Vec<SearchHit>, bool) {
        let from = from.filter(|f| {
            f.position.age_s <= MAX_POSITION_AGE_S
                && f.position.lat.is_finite()
                && f.position.lon.is_finite()
        });
        let Some(from) = from else {
            let mut chosen = chosen;
            chosen.sort_by(|a, b| best_first(&a.hit, &b.hit));
            let hits = chosen.into_iter().map(|c| shown(c.hit)).collect();
            return (hits, false);
        };
        let origin = (from.position.lat, from.position.lon);
        let air: Vec<f64> = chosen
            .iter()
            .map(|c| haversine_m(origin, (c.hit.lat, c.hit.lon)))
            .collect();
        let matrix = self.matrix_from(from, &chosen, &air);
        let mut measured: Vec<Measured> = Vec::with_capacity(chosen.len());
        for (i, c) in chosen.into_iter().enumerate() {
            let match_rank = c.hit.match_rank;
            let far = c.far;
            let mut hit = shown(c.hit);
            hit.distance_m = air[i];
            let (group, key) = match matrix.as_ref().map(|m| m[i]) {
                Some(Slot::Routed(entry)) => {
                    hit.route_m = Some(entry.distance_m);
                    hit.route_s = Some(entry.duration_s);
                    (Group::Routed, entry.distance_m)
                }
                Some(Slot::NoWay) => {
                    hit.no_route = true;
                    (Group::NoRoute, air[i])
                }
                Some(Slot::NotRouted) | None => (Group::Unrouted, air[i]),
            };
            measured.push(Measured {
                hit,
                match_rank,
                far,
                group,
                key,
            });
        }
        measured.sort_by(|a, b| {
            a.match_rank
                .cmp(&b.match_rank)
                .then(a.far.cmp(&b.far))
                .then(a.group.cmp(&b.group))
                .then(a.key.partial_cmp(&b.key).unwrap_or(Ordering::Equal))
        });
        let ascent_ignored =
            from.leg.profile == Profile::Hike && measured.iter().any(|m| m.hit.route_s.is_some());
        (
            measured.into_iter().map(|m| m.hit).collect(),
            ascent_ignored,
        )
    }

    /// The way from the position to every hit worth routing, one answer per
    /// hit. `None` if nothing could be routed at all.
    fn matrix_from(&self, from: &RouteFrom, chosen: &[Chosen], air: &[f64]) -> Option<Vec<Slot>> {
        if from.leg.profile == Profile::Manual {
            return None;
        }
        let origin = (from.position.lat, from.position.lon);
        let store = self.shared.maps.store();
        let tiles = store.routing_tiles(&[origin])?;
        let engine = self
            .shared
            .routing
            .factory()
            .and_then(|factory| factory.engine_version());
        if let (Some(info), Some(engine)) = (&tiles.info, engine)
            && info.valhalla != engine
        {
            return None;
        }
        let router = self
            .shared
            .routing
            .router(&tiles.region, &tiles.path)
            .ok()?;
        let mut slots = vec![Slot::NotRouted; chosen.len()];
        let mut targets = Vec::new();
        let mut at = Vec::new();
        for (i, c) in chosen.iter().enumerate() {
            if c.far || air[i] > ROUTE_LIMIT_M {
                continue;
            }
            let target = (c.hit.lat, c.hit.lon);
            if store
                .routing_tiles(&[origin, target])
                .is_some_and(|t| t.region == tiles.region)
            {
                targets.push(LatLon {
                    lat: target.0,
                    lon: target.1,
                });
                at.push(i);
            } else {
                slots[i] = Slot::NoWay;
            }
        }
        if targets.is_empty() {
            return Some(slots);
        }
        let answers = router
            .matrix(&MatrixRequest {
                from: LatLon {
                    lat: origin.0,
                    lon: origin.1,
                },
                to: targets,
                profile: from.leg.profile,
                options: from.leg.options,
            })
            .ok()?;
        if answers.len() != at.len() {
            return None;
        }
        for (i, answer) in at.into_iter().zip(answers) {
            slots[i] = answer.map_or(Slot::NoWay, Slot::Routed);
        }
        Some(slots)
    }
}

#[derive(Clone, Copy)]
enum Slot {
    Routed(core_routing::MatrixEntry),
    NoWay,
    NotRouted,
}
