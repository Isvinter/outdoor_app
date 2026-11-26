//! Following a planned route: where the user is along it, whether they have
//! left it and how to get back.

mod app;
// The file name `guide.rs` is part of the plan; the module is the guide itself.
#[allow(clippy::module_inception)]
mod guide;
mod hints;
mod line;
#[cfg(test)]
mod testing;
mod turn_text;

pub(crate) use app::ActiveGuide;
pub use guide::{
    Guide, GuideParams, GuidePhase, GuideStatus, NextWaypoint, OffNetwork, Remaining, WayBack,
    WayBackRouter, WayBackTarget,
};
pub use hints::{HintParams, TurnHint};
pub use line::{LegKind, LegSpan, LineTurn, Projection, RouteLine};
pub use turn_text::HintStage;
