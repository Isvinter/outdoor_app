//! Guidance along the planned route for the apps.

use core_app::{GuidePhase, GuideStatus, HintStage, TurnHint};
use core_routing::TurnKind;

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
    /// The hint for the next turn, only while on the route.
    pub turn: Option<TurnInfo>,
    /// Counts up with every hint told; Trail tells the user when it grows.
    pub turn_seq: u32,
    /// Whether the route has its turns; `false` for routes stored before
    /// turns existed.
    pub has_turns: bool,
}

/// The hint for the next turn, with its texts ready to show and speak.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TurnInfo {
    /// The kind of turn, such as "right", "roundabout" or "stage_end".
    pub kind: String,
    /// "ahead" for the announcement before the turn, "now" at the turn.
    pub stage: String,
    /// Distance to the turn, at the latest fix.
    pub distance_m: f64,
    /// `distance_m` as told to the user, such as "200 m".
    pub distance_text: String,
    /// The name of the turn, such as "rechts".
    pub label: String,
    /// The sentence from the moment the hint was told, such as
    /// "In 200 m rechts"; for the notification and the speech.
    pub spoken: String,
}

/// The name of `kind` as stored in plans, such as "stage_end".
fn turn_kind_name(kind: TurnKind) -> String {
    serde_json::to_value(kind)
        .ok()
        .and_then(|value| value.as_str().map(String::from))
        .unwrap_or_default()
}

/// `hint` for the apps.
fn turn_info(hint: &TurnHint) -> TurnInfo {
    TurnInfo {
        kind: turn_kind_name(hint.kind),
        stage: match hint.stage {
            HintStage::Ahead => "ahead",
            HintStage::Now => "now",
        }
        .into(),
        distance_m: hint.distance_m,
        distance_text: hint.distance_text.clone(),
        label: hint.label.clone(),
        spoken: hint.spoken.clone(),
    }
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
        turn: status.turn.as_ref().map(turn_info),
        turn_seq: status.turn_seq,
        has_turns: status.has_turns,
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

#[cfg(test)]
mod tests {
    use core_app::Remaining;

    use super::*;

    fn status(turn: Option<TurnHint>) -> GuideStatus {
        GuideStatus {
            phase: GuidePhase::OnRoute,
            along_m: 288.0,
            offset_m: Some(2.0),
            next_waypoint: Some(1),
            deviation: 0,
            way_back: None,
            to_stage_end: None,
            to_destination: Remaining {
                distance_m: 712.0,
                time_s: None,
            },
            turn,
            turn_seq: 2,
            has_turns: true,
        }
    }

    #[test]
    fn guidance_info_carries_turn() {
        let info = guidance_info(&status(Some(TurnHint {
            kind: TurnKind::Right,
            exit: None,
            then: None,
            stage: HintStage::Now,
            distance_m: 12.0,
            distance_text: "10 m".into(),
            label: "rechts".into(),
            spoken: "Jetzt rechts".into(),
        })));
        assert_eq!(
            info.turn,
            Some(TurnInfo {
                kind: "right".into(),
                stage: "now".into(),
                distance_m: 12.0,
                distance_text: "10 m".into(),
                label: "rechts".into(),
                spoken: "Jetzt rechts".into(),
            })
        );
        assert_eq!(info.turn_seq, 2);
        assert!(info.has_turns);
    }

    #[test]
    fn turn_kind_uses_serde_names() {
        assert_eq!(turn_kind_name(TurnKind::StageEnd), "stage_end");
        assert_eq!(turn_kind_name(TurnKind::Roundabout), "roundabout");
        assert_eq!(turn_kind_name(TurnKind::UTurn), "u_turn");
        assert_eq!(turn_kind_name(TurnKind::SlightLeft), "slight_left");
    }

    #[test]
    fn guidance_info_without_turn() {
        let mut old = status(None);
        old.turn_seq = 0;
        old.has_turns = false;
        let info = guidance_info(&old);
        assert_eq!(info.turn, None);
        assert_eq!(info.turn_seq, 0);
        assert!(!info.has_turns);
    }
}
