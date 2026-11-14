//! Builds offline region packages on the desktop: writes the definition of a
//! region chosen on the map, picks the OpenStreetMap extracts that cover it
//! and runs `scripts/build-region.sh`. Not used on mobile.

mod definition;
mod error;
mod extracts;

pub use definition::{RegionDefinition, estimated_bytes, size_km, slug, unique_id, validate_bbox};
pub use error::BuildError;
pub use extracts::{Extract, GEOFABRIK_INDEX_URL, GeofabrikIndex};
