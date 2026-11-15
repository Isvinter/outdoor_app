//! Offline map regions: catalog, download, storage and tile access.

mod dem;
mod error;
mod fetch;
mod manifest;
mod pmtiles;
mod store;
mod style;
mod writer;

pub use dem::{DemTile, ProfilePoint, decode_dem_tile, decode_terrarium};
pub use error::MapsError;
pub use fetch::{DefaultFetcher, Fetched, Fetcher};
pub use manifest::{
    Catalog, CatalogEntry, FORMAT, Layer, ROUTING_FILE, RegionFile, RegionManifest, RoutingInfo,
    TilePath,
};
pub use pmtiles::{Archive, Header, TileType, tile_id};
pub use store::{InstalledRegion, Progress, RegionStore, RoutingTiles, Tile};
pub use style::{CONTOUR_MAX_ZOOM, Contours, StyleOptions, offline_style};
pub use writer::{ArchiveInfo, write_archive};
