use std::str::FromStr;

use core_track::geo::haversine_m;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A position in degrees (WGS 84).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LatLon {
    pub lat: f64,
    pub lon: f64,
}

impl LatLon {
    /// Great-circle distance in metres.
    pub fn distance_m(&self, other: &LatLon) -> f64 {
        haversine_m(self.lat, self.lon, other.lat, other.lon)
    }
}

/// How a route is found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Profile {
    RoadBike,
    Trekking,
    Mtb,
    Hike,
    Car,
    /// Straight lines between the waypoints, for terrain without ways (ski tours).
    Manual,
}

/// Colour of a leg that could not be computed, drawn as a dashed straight line.
pub const FAILED_LEG_COLOR: &str = "#d93025";

impl Profile {
    /// One colour per profile, the same on the map and in the elevation
    /// profile on every platform (as `apps/base/ui/src/profile-colors.ts`).
    pub fn color(self) -> &'static str {
        match self {
            Profile::RoadBike => "#1565c0",
            Profile::Trekking => "#00897b",
            Profile::Mtb => "#e07b00",
            Profile::Hike => "#c62828",
            Profile::Car => "#546e7a",
            Profile::Manual => "#6a1b9a",
        }
    }
}

/// An adjustable setting of a profile, for user interfaces to build controls from.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct OptionSpec {
    /// Name of the field in [`RouteOptions`].
    pub key: &'static str,
    pub min: f64,
    pub max: f64,
    pub default: f64,
    pub step: f64,
}

const fn share(key: &'static str, default: f64) -> OptionSpec {
    OptionSpec {
        key,
        min: 0.0,
        max: 1.0,
        default,
        step: 0.1,
    }
}

impl Profile {
    pub const ALL: [Profile; 6] = [
        Profile::RoadBike,
        Profile::Trekking,
        Profile::Mtb,
        Profile::Hike,
        Profile::Car,
        Profile::Manual,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Profile::RoadBike => "road_bike",
            Profile::Trekking => "trekking",
            Profile::Mtb => "mtb",
            Profile::Hike => "hike",
            Profile::Car => "car",
            Profile::Manual => "manual",
        }
    }

    /// The settings of this profile with their ranges and defaults.
    ///
    /// `use_hills`, `use_roads`: 0 avoids, 1 does not mind. `avoid_bad_surfaces`:
    /// 1 refuses surfaces too rough for the bike. `max_hiking_difficulty`:
    /// highest `sac_scale` to walk, 1 (hiking) to 6 (difficult alpine hiking).
    pub fn options(self) -> &'static [OptionSpec] {
        const ROAD: [OptionSpec; 3] = [
            share("use_hills", 0.5),
            share("use_roads", 0.6),
            share("avoid_bad_surfaces", 0.8),
        ];
        const TREKKING: [OptionSpec; 3] = [
            share("use_hills", 0.5),
            share("use_roads", 0.3),
            share("avoid_bad_surfaces", 0.4),
        ];
        const MTB: [OptionSpec; 3] = [
            share("use_hills", 0.8),
            share("use_roads", 0.1),
            share("avoid_bad_surfaces", 0.0),
        ];
        const HIKE: [OptionSpec; 2] = [
            OptionSpec {
                key: "max_hiking_difficulty",
                min: 1.0,
                max: 6.0,
                default: 3.0,
                step: 1.0,
            },
            share("use_hills", 0.8),
        ];
        match self {
            Profile::RoadBike => &ROAD,
            Profile::Trekking => &TREKKING,
            Profile::Mtb => &MTB,
            Profile::Hike => &HIKE,
            Profile::Car | Profile::Manual => &[],
        }
    }
}

impl FromStr for Profile {
    type Err = RoutingError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Profile::ALL
            .into_iter()
            .find(|profile| profile.as_str() == s)
            .ok_or_else(|| RoutingError::InvalidOption(format!("unknown profile {s:?}")))
    }
}

/// Overrides for the settings of a profile; `None` keeps the default.
/// Settings a profile does not have are ignored.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RouteOptions {
    pub use_hills: Option<f64>,
    pub use_roads: Option<f64>,
    pub avoid_bad_surfaces: Option<f64>,
    pub max_hiking_difficulty: Option<u8>,
}

impl RouteOptions {
    /// The override for a setting, by its [`OptionSpec::key`].
    pub fn get(&self, key: &str) -> Option<f64> {
        match key {
            "use_hills" => self.use_hills,
            "use_roads" => self.use_roads,
            "avoid_bad_surfaces" => self.avoid_bad_surfaces,
            "max_hiking_difficulty" => self.max_hiking_difficulty.map(f64::from),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteRequest {
    /// Start, optional intermediate points, destination.
    pub waypoints: Vec<LatLon>,
    pub profile: Profile,
    #[serde(default)]
    pub options: RouteOptions,
    /// Direction of travel at the first point, in degrees clockwise from
    /// north. Only the first point is given a heading.
    #[serde(default)]
    pub heading_deg: Option<f32>,
}

/// A planned route.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Route {
    pub profile: Profile,
    pub waypoints: Vec<LatLon>,
    /// The way to go, from the first to the last waypoint.
    pub line: Vec<LatLon>,
    pub distance_m: f64,
    /// Travel time as estimated by the router; `None` for manual routes.
    pub duration_s: Option<f64>,
    /// Where the way turns off, in the order of the line. `None` when turns
    /// were not computed (manual routes), `Some(vec![])` when there are none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turns: Option<Vec<Turn>>,
}

/// What to do at a turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnKind {
    SlightLeft,
    SlightRight,
    Left,
    Right,
    SharpLeft,
    SharpRight,
    UTurn,
    KeepLeft,
    KeepRight,
    /// Enter a roundabout; [`Turn::exit`] tells which exit to take.
    Roundabout,
    /// The end of a stage (a waypoint between two legs) is reached.
    StageEnd,
}

/// A turn along a route line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turn {
    /// Index of the point in the line this turn belongs to.
    pub at: usize,
    pub kind: TurnKind,
    /// Number of the exit to take at a roundabout, counted from 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<u8>,
}

#[derive(Debug, Error)]
pub enum RoutingError {
    #[error("a route needs at least a start and a destination")]
    TooFewWaypoints,
    #[error("invalid route option: {0}")]
    InvalidOption(String),
    #[error("profile {} is not computed by a router", .0.as_str())]
    NotRoutable(Profile),
    #[error("no route found: {0}")]
    NoRoute(String),
    #[error("no routing data: {0}")]
    Unavailable(String),
    #[error("routing engine: {0}")]
    Engine(String),
    #[error("routing data built with Valhalla {data}, this app has {engine}")]
    Incompatible { data: String, engine: String },
}

/// Finds routes. Implemented by [`crate::JsonRouter`] and, with the feature
/// `valhalla`, by `ValhallaRouter`.
pub trait Router: Send + Sync {
    fn route(&self, request: &RouteRequest) -> Result<Route, RoutingError>;
    /// The profiles this router can compute.
    fn profiles(&self) -> Vec<Profile>;
}
