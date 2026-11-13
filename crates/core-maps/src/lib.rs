//! Offline map regions: catalog, download, storage and tile access.

mod error;
mod fetch;
mod manifest;
mod pmtiles;
mod store;

pub use error::MapsError;
pub use fetch::{DefaultFetcher, Fetched, Fetcher};
pub use manifest::{Catalog, CatalogEntry, FORMAT, Layer, RegionFile, RegionManifest, TilePath};
pub use pmtiles::{Archive, Header, TileType, tile_id};
pub use store::{InstalledRegion, Progress, RegionStore, Tile};
