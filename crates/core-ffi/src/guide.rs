//! Guidance along the planned route for the apps.

use core_app::{GuidePhase, GuideStatus};

use crate::{CoreApp, CoreError, LatLon};

/// Where the user stands on the planned route and what is left.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct GuidanceInfo {
    /// "waiting", "on_route", "off_route", "no_way_back" or "arrived".
    pub phase: String,
    /// Progress along the planned route.
    pub along_m: f64,
    /// The waypoint ahead (or the one the way back leads to).
    pub next_waypoint: Option<u32>,
    /// Counts up with every new deviation.
    pub deviation: u32,
    /// The way back to the route; empty if there is none.
    pub way_back: Vec<LatLon>,
    pub way_back_m: Option<f64>,
    pub to_stage_end_m: Option<f64>,
    pub to_stage_end_s: Option<f64>,
    pub to_destination_m: f64,
    pub to_destination_s: Option<f64>,
}

pub(crate) fn guidance_info(status: &GuideStatus) -> GuidanceInfo {
    GuidanceInfo {
        phase: match status.phase {
            GuidePhase::Waiting => "waiting",
            GuidePhase::OnRoute => "on_route",
            GuidePhase::OffRoute => "off_route",
            GuidePhase::NoWayBack => "no_way_back",
            GuidePhase::Arrived => "arrived",
        }
        .into(),
        along_m: status.along_m,
        next_waypoint: status.next_waypoint.and_then(|i| u32::try_from(i).ok()),
        deviation: status.deviation,
        way_back: status
            .way_back
            .as_ref()
            .map(|w| w.line.iter().copied().map(LatLon::from).collect())
            .unwrap_or_default(),
        way_back_m: status.way_back.as_ref().map(|w| w.distance_m),
        to_stage_end_m: status.to_stage_end.map(|r| r.distance_m),
        to_stage_end_s: status.to_stage_end.and_then(|r| r.time_s),
        to_destination_m: status.to_destination.distance_m,
        to_destination_s: status.to_destination.time_s,
    }
}

#[uniffi::export]
impl CoreApp {
    /// Starts guidance along the planned route; without a recent position it
    /// waits for the first usable fix.
    pub fn start_guidance(&self) -> Result<GuidanceInfo, CoreError> {
        Ok(guidance_info(&self.app.start_guidance()?))
    }

    /// Like `start_guidance`, with the progress starting at waypoint `index`.
    pub fn guide_from_waypoint(&self, index: u32) -> Result<GuidanceInfo, CoreError> {
        Ok(guidance_info(
            &self.app.guide_from_waypoint(index as usize)?,
        ))
    }

    /// Ends the guidance; a recording goes on.
    pub fn stop_guidance(&self) {
        self.app.stop_guidance();
    }

    /// Where the guidance stands, if it runs.
    pub fn guidance_status(&self) -> Option<GuidanceInfo> {
        self.app.guidance_status().as_ref().map(guidance_info)
    }
}
