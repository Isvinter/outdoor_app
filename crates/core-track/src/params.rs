use serde::{Deserialize, Serialize};

use crate::Activity;

/// Tunable parameters of the statistics pipeline. See PLAN.md, section 6.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatsParams {
    /// Speed at or above which the user counts as moving.
    pub v_move_mps: f64,
    /// Time below `v_move_mps` after which movement counts as stopped.
    pub stop_delay_s: f64,
    /// Time window for the speed used to detect movement.
    pub move_window_s: f64,
    /// Time window for the speed used for the maximum speed.
    pub speed_window_s: f64,
    /// Fixes implying a higher speed than this are GPS jumps.
    pub max_plausible_speed_mps: f64,
    /// Fixes implying a higher acceleration than this are GPS jumps.
    pub max_accel_mps2: f64,
    /// After this many consecutive rejected fixes the next one is accepted as a new anchor.
    pub max_reject_run: u32,
    /// Fixes with a worse reported horizontal accuracy are dropped.
    pub max_h_accuracy_m: f64,
    /// Displacement below this does not add distance (jitter while standing).
    pub min_move_m: f64,
    /// Intervals longer than this never count as moving time.
    pub max_gap_s: f64,
    /// Length of the distance window used to smooth elevation.
    pub ele_smooth_m: f64,
    /// Elevation change that must accumulate before a climb or descent is counted.
    pub ele_threshold_m: f64,
}

const KMH: f64 = 1.0 / 3.6;

impl StatsParams {
    /// Default parameters for an activity.
    pub fn for_activity(activity: Activity) -> Self {
        let base = Self {
            v_move_mps: 3.0 * KMH,
            stop_delay_s: 5.0,
            move_window_s: 5.0,
            speed_window_s: 5.0,
            max_plausible_speed_mps: 110.0 * KMH,
            max_accel_mps2: 6.0,
            max_reject_run: 5,
            max_h_accuracy_m: 50.0,
            min_move_m: 3.0,
            max_gap_s: 30.0,
            ele_smooth_m: 60.0,
            ele_threshold_m: 5.0,
        };
        match activity {
            Activity::Bike => base,
            Activity::Mtb => Self {
                v_move_mps: 2.0 * KMH,
                max_plausible_speed_mps: 90.0 * KMH,
                ele_smooth_m: 40.0,
                ..base
            },
            Activity::Hike => Self {
                v_move_mps: 1.0 * KMH,
                stop_delay_s: 10.0,
                move_window_s: 15.0,
                max_plausible_speed_mps: 25.0 * KMH,
                ele_smooth_m: 30.0,
                ..base
            },
            Activity::SkiTour => Self {
                v_move_mps: 0.8 * KMH,
                stop_delay_s: 10.0,
                move_window_s: 15.0,
                ele_smooth_m: 30.0,
                ..base
            },
        }
    }
}
