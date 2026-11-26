//! Platform independent core of the flight computer.
//!
//! This crate must not depend on any UI, operating system or audio API. Time never comes
//! from the system clock: every sensor sample carries its own [`time::Timestamp`], which
//! keeps replay, accelerated simulation and tests deterministic.

pub mod airspace;
pub mod airspace_warning;
pub mod atmosphere;
pub mod flight_phase;
pub mod flightlog;
pub mod geo;
pub mod glide;
pub mod igc;
pub mod landing;
pub mod openaip;
pub mod sensor;
pub mod settings;
pub mod terrain;
pub mod time;
pub mod track;
pub mod vario;
pub mod wind;
