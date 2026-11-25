//! Following a planned route: where the user is along it, whether they have
//! left it and how to get back.

mod app;
// The file name `guide.rs` is part of the plan; the module is the guide itself.
#[allow(clippy::module_inception)]
mod guide;
mod line;
#[cfg(test)]
mod testing;
#[allow(dead_code)] // used by the announcer (next task)
mod turn_text;

pub(crate) use app::ActiveGuide;
pub use guide::{
    Guide, GuideParams, GuidePhase, GuideStatus, NextWaypoint, Remaining, WayBack, WayBackRouter,
    WayBackTarget,
};
pub use line::{LegKind, LegSpan, Projection, RouteLine};
#[allow(unused_imports)] // used by the announcer (next task)
pub(crate) use turn_text::{HintStage, distance_text, label, spoken};
