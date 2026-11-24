//! Following a planned route: where the user is along it, whether they have
//! left it and how to get back.

// The file name `guide.rs` is part of the plan; the module is the guide itself.
#[allow(clippy::module_inception)]
mod guide;
mod line;
#[cfg(test)]
mod testing;

pub use guide::{
    Guide, GuideParams, GuidePhase, GuideStatus, NextWaypoint, Remaining, WayBack, WayBackRouter,
    WayBackTarget,
};
pub use line::{LegKind, LegSpan, Projection, RouteLine};
