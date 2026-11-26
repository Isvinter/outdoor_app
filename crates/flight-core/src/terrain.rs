//! Ground elevation, for airspace limits given above ground (AGL).
//!
//! `flight-core` does not read map data itself; the app implements [`Terrain`] with
//! whatever elevation model it has (on the phone: the `dem.pmtiles` of the offline
//! regions). Code here only asks for the elevation at a position.

/// Something that knows the ground elevation.
///
/// `Send` lets the terrain move to another thread; `Sync` means it may be used from several
/// threads at once (through `&self`). Both together allow one `Arc<dyn Terrain>` (a shared,
/// reference-counted pointer to any terrain) to be held by the engine and a worker thread.
pub trait Terrain: Send + Sync {
    /// Elevation of the ground above mean sea level in metres, or `None` where it is
    /// not known (outside of all regions).
    fn elevation_m(&self, lat_deg: f64, lon_deg: f64) -> Option<f64>;
}

/// No elevation model at all.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoTerrain;

impl Terrain for NoTerrain {
    fn elevation_m(&self, _lat_deg: f64, _lon_deg: f64) -> Option<f64> {
        None
    }
}

/// The same elevation everywhere; for tests and the simulator.
#[derive(Debug, Clone, Copy)]
pub struct FlatTerrain(pub f64);

impl Terrain for FlatTerrain {
    fn elevation_m(&self, _lat_deg: f64, _lon_deg: f64) -> Option<f64> {
        Some(self.0)
    }
}
