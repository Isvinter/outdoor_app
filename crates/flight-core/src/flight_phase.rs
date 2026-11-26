//! Takeoff and landing detection.
//!
//! Fed with one [`PhaseSample`] per GPS fix, [`FlightPhaseDetector`] tells when a flight
//! starts and ends, both dated back to when they really happened. The rules follow the
//! Flymaster GPS M where it makes sense (landing: slow and calm for 30 s), and use the
//! terrain where it is known:
//!
//! - **Takeoff** with terrain: at least [`TAKEOFF_HEIGHT_M`] above it for
//!   [`TAKEOFF_HOLD_S`]. A car stays on the ground, so driving never starts a flight;
//!   a launch in strong wind with hardly any speed over ground still does. Without
//!   terrain: the takeoff speed for [`TAKEOFF_HOLD_S`], as on the Flymaster.
//! - **Landing**: slower than [`LANDING_SPEED_MPS`] and climb within
//!   ±[`LANDING_CLIMB_MPS`], with terrain also below [`LANDING_HEIGHT_M`] above it, for
//!   [`LANDING_HOLD_S`] ([`LANDING_HOLD_NO_TERRAIN_S`] without terrain). Being close to
//!   the ground keeps soaring in strong wind (no speed over ground, no climb, but high
//!   above the slope) from counting as a landing.
//!
//! Missing data never ends a flight: an unknown speed is not calm, and a gap in the
//! samples restarts every count.

use std::collections::VecDeque;

/// Height above terrain that counts as airborne, in metres.
pub const TAKEOFF_HEIGHT_M: f64 = 20.0;
/// How long the takeoff condition must hold, in seconds.
pub const TAKEOFF_HOLD_S: f64 = 5.0;
/// Default takeoff speed where the terrain is unknown: 15 km/h.
pub const DEFAULT_TAKEOFF_SPEED_MPS: f64 = 15.0 / 3.6;
/// Below this speed over ground (5 km/h) the glider may be on the ground.
pub const LANDING_SPEED_MPS: f64 = 5.0 / 3.6;
/// Climb rates within ± this value are calm enough for the ground.
pub const LANDING_CLIMB_MPS: f64 = 1.5;
/// Below this height above terrain the glider may be on the ground.
pub const LANDING_HEIGHT_M: f64 = 20.0;
/// How long it must stay calm near the ground before the flight ends, in seconds.
pub const LANDING_HOLD_S: f64 = 30.0;
/// The same where the height above terrain is unknown: longer, because nothing
/// separates soaring without speed over ground from standing in the landing field.
pub const LANDING_HOLD_NO_TERRAIN_S: f64 = 60.0;
/// How far back the detector remembers samples, for dating the takeoff back.
pub const HISTORY_S: f64 = 60.0;
/// Samples further apart than this do not continue a count.
pub const MAX_GAP_S: f64 = 3.0;
/// Standing still before a launch the climb rate stays close to zero; a glider being
/// lifted off by strong wind climbs although it hardly moves over ground.
const REST_CLIMB_MPS: f64 = 0.5;

/// One sample per GPS fix.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhaseSample {
    pub time_s: f64,
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    /// Speed over ground; `None` if the GPS did not report it.
    pub ground_speed_mps: Option<f64>,
    /// Climb rate of the vario; `None` before the filter has a reading.
    pub climb_rate_mps: Option<f64>,
    /// GPS altitude minus terrain; `None` without terrain or without a 3D fix.
    pub height_above_terrain_m: Option<f64>,
}

impl PhaseSample {
    /// Calm enough to be standing in a landing field. Unknown speed is not calm (no
    /// landing from missing data); unknown climb or height do not object.
    fn is_calm(&self) -> bool {
        self.ground_speed_mps
            .is_some_and(|speed| speed < LANDING_SPEED_MPS)
            && self
                .climb_rate_mps
                .is_none_or(|climb| climb.abs() < LANDING_CLIMB_MPS)
            && self
                .height_above_terrain_m
                .is_none_or(|height| height < LANDING_HEIGHT_M)
    }

    /// Standing still on the ground, the last moment before a takeoff.
    fn is_at_rest(&self) -> bool {
        self.is_calm()
            && self
                .climb_rate_mps
                .is_none_or(|climb| climb.abs() < REST_CLIMB_MPS)
    }
}

/// Where the glider is in its flight. `Ready` and `Landed` both wait for a takeoff; they
/// differ only in what the pilot is told.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Ready,
    Airborne,
    Landed,
}

/// What [`FlightPhaseDetector::push`] reports. An `enum` whose variants carry data:
/// each variant has its own fields.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PhaseEvent {
    /// Dated back to the last sample at rest before the takeoff (or the oldest
    /// remembered sample if there was none).
    Takeoff {
        time_s: f64,
        latitude_deg: f64,
        longitude_deg: f64,
    },
    /// Dated back to the first sample of the calm phase that ended the flight.
    Landing { time_s: f64 },
}

#[derive(Debug, Clone)]
pub struct FlightPhaseDetector {
    takeoff_speed_mps: f64,
    phase: Phase,
    /// Samples of the last [`HISTORY_S`], oldest first.
    history: VecDeque<PhaseSample>,
    /// Time of the first sample of the current run of samples that meet the takeoff
    /// (on the ground) or landing (in the air) condition.
    run_start_s: Option<f64>,
    /// Whether every sample of the current landing run knew its height above terrain.
    run_knows_height: bool,
    last_time_s: Option<f64>,
    takeoff_time_s: Option<f64>,
    landing_time_s: Option<f64>,
}

impl FlightPhaseDetector {
    pub fn new(takeoff_speed_mps: f64) -> Self {
        FlightPhaseDetector {
            takeoff_speed_mps,
            phase: Phase::Ready,
            history: VecDeque::new(),
            run_start_s: None,
            run_knows_height: true,
            last_time_s: None,
            takeoff_time_s: None,
            landing_time_s: None,
        }
    }

    pub fn set_takeoff_speed(&mut self, takeoff_speed_mps: f64) {
        self.takeoff_speed_mps = takeoff_speed_mps;
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// Time of the takeoff of the current or last flight.
    pub fn takeoff_time_s(&self) -> Option<f64> {
        self.takeoff_time_s
    }

    /// Time of the landing of the last flight; `None` while airborne or before the first.
    pub fn landing_time_s(&self) -> Option<f64> {
        self.landing_time_s
    }

    /// Takes the next sample (in time order) and reports a takeoff or landing.
    pub fn push(&mut self, sample: PhaseSample) -> Option<PhaseEvent> {
        if self
            .last_time_s
            .is_some_and(|last| sample.time_s - last > MAX_GAP_S)
        {
            self.run_start_s = None;
        }
        self.last_time_s = Some(sample.time_s);

        let event = match self.phase {
            Phase::Ready | Phase::Landed => self.check_takeoff(&sample),
            Phase::Airborne => self.check_landing(&sample),
        };

        self.history.push_back(sample);
        while self
            .history
            .front()
            .is_some_and(|oldest| sample.time_s - oldest.time_s > HISTORY_S)
        {
            self.history.pop_front();
        }
        event
    }

    fn takeoff_condition(&self, sample: &PhaseSample) -> bool {
        match sample.height_above_terrain_m {
            Some(height) => height >= TAKEOFF_HEIGHT_M,
            None => sample
                .ground_speed_mps
                .is_some_and(|speed| speed >= self.takeoff_speed_mps),
        }
    }

    fn check_takeoff(&mut self, sample: &PhaseSample) -> Option<PhaseEvent> {
        if !self.takeoff_condition(sample) {
            self.run_start_s = None;
            return None;
        }
        let run_start_s = *self.run_start_s.get_or_insert(sample.time_s);
        if sample.time_s - run_start_s < TAKEOFF_HOLD_S {
            return None;
        }
        // The last moment at rest before the condition began; `rev` walks the history
        // from the newest sample back.
        let start = self
            .history
            .iter()
            .rev()
            .filter(|earlier| earlier.time_s < run_start_s)
            .find(|earlier| earlier.is_at_rest())
            .or(self.history.front())
            .copied()
            .unwrap_or(*sample);
        self.phase = Phase::Airborne;
        self.run_start_s = None;
        self.takeoff_time_s = Some(start.time_s);
        self.landing_time_s = None;
        Some(PhaseEvent::Takeoff {
            time_s: start.time_s,
            latitude_deg: start.latitude_deg,
            longitude_deg: start.longitude_deg,
        })
    }

    fn check_landing(&mut self, sample: &PhaseSample) -> Option<PhaseEvent> {
        if !sample.is_calm() {
            self.run_start_s = None;
            return None;
        }
        if self.run_start_s.is_none() {
            self.run_start_s = Some(sample.time_s);
            self.run_knows_height = true;
        }
        self.run_knows_height &= sample.height_above_terrain_m.is_some();
        let run_start_s = self.run_start_s?;
        let hold_s = if self.run_knows_height {
            LANDING_HOLD_S
        } else {
            LANDING_HOLD_NO_TERRAIN_S
        };
        if sample.time_s - run_start_s < hold_s {
            return None;
        }
        self.phase = Phase::Landed;
        self.run_start_s = None;
        self.landing_time_s = Some(run_start_s);
        Some(PhaseEvent::Landing {
            time_s: run_start_s,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sample at 1 Hz; the position moves a little with the time so that the takeoff
    /// position tells which sample it came from.
    fn sample(time_s: f64, speed: f64, climb: f64, height: Option<f64>) -> PhaseSample {
        PhaseSample {
            time_s,
            latitude_deg: 47.0 + time_s * 1e-5,
            longitude_deg: 11.0,
            ground_speed_mps: Some(speed),
            climb_rate_mps: Some(climb),
            height_above_terrain_m: height,
        }
    }

    /// Feeds the samples and collects the events.
    fn run(
        detector: &mut FlightPhaseDetector,
        samples: impl IntoIterator<Item = PhaseSample>,
    ) -> Vec<PhaseEvent> {
        samples
            .into_iter()
            .filter_map(|sample| detector.push(sample))
            .collect()
    }

    fn detector() -> FlightPhaseDetector {
        FlightPhaseDetector::new(DEFAULT_TAKEOFF_SPEED_MPS)
    }

    /// 30 s at rest at launch, then a launch run and a climb away from the slope at
    /// 2 m/s height gain; the detector is airborne afterwards.
    fn launch(detector: &mut FlightPhaseDetector) -> Vec<PhaseEvent> {
        let rest = (0..30).map(|t| sample(t as f64, 0.2, 0.0, Some(1.0)));
        let away = (30..60).map(|t| {
            let s = (t - 30) as f64;
            sample(t as f64, 9.0, -0.5, Some(1.0 + 2.0 * s))
        });
        run(detector, rest.chain(away))
    }

    #[test]
    fn standing_at_launch_stays_ready() {
        let mut detector = detector();
        // GPS and terrain disagree by a few metres, the vario wobbles.
        let events = run(
            &mut detector,
            (0..300).map(|t| {
                let t = t as f64;
                sample(
                    t,
                    0.3,
                    0.2 * (t * 0.7).sin(),
                    Some(1.0 + 3.0 * (t * 0.3).sin()),
                )
            }),
        );
        assert!(events.is_empty());
        assert_eq!(detector.phase(), Phase::Ready);
    }

    #[test]
    fn takeoff_by_height_above_terrain() {
        let mut detector = detector();
        let events = launch(&mut detector);
        assert_eq!(events.len(), 1, "{events:?}");
        assert!(matches!(events[0], PhaseEvent::Takeoff { .. }));
        assert_eq!(detector.phase(), Phase::Airborne);
    }

    #[test]
    fn takeoff_is_dated_back_to_the_last_rest() {
        let mut detector = detector();
        let events = launch(&mut detector);
        let PhaseEvent::Takeoff {
            time_s,
            latitude_deg,
            ..
        } = events[0]
        else {
            panic!("{events:?}");
        };
        assert_eq!(time_s, 29.0);
        assert_eq!(latitude_deg, sample(29.0, 0.0, 0.0, None).latitude_deg);
        assert_eq!(detector.takeoff_time_s(), Some(29.0));
    }

    #[test]
    fn strong_wind_launch_is_detected() {
        // Hardly any speed over ground, before and after the launch; the wind lifts the
        // glider off and it climbs up the slope.
        let mut detector = detector();
        let rest = (0..30).map(|t| sample(t as f64, 0.5, 0.0, Some(1.0)));
        let lifted = (30..90).map(|t| {
            let s = (t - 30) as f64;
            sample(t as f64, 1.0, 1.5, Some(1.0 + 1.5 * s))
        });
        let events = run(&mut detector, rest.chain(lifted));
        assert_eq!(
            events,
            vec![PhaseEvent::Takeoff {
                time_s: 29.0,
                latitude_deg: sample(29.0, 0.0, 0.0, None).latitude_deg,
                longitude_deg: 11.0,
            }]
        );
    }

    #[test]
    fn car_ride_with_terrain_is_no_takeoff() {
        // 50 km/h on a mountain road for ten minutes: the height above terrain moves a
        // few metres with GPS and terrain errors, the altitude changes along the road.
        let mut detector = detector();
        let events = run(
            &mut detector,
            (0..600).map(|t| {
                let t = t as f64;
                sample(t, 14.0, (t * 0.05).sin(), Some(2.0 + 4.0 * (t * 0.2).sin()))
            }),
        );
        assert!(events.is_empty());
    }

    #[test]
    fn takeoff_by_speed_without_terrain() {
        let mut detector = detector();
        let rest = (0..10).map(|t| sample(t as f64, 0.0, 0.0, None));
        let away = (10..20).map(|t| sample(t as f64, 5.0, -1.0, None));
        let events = run(&mut detector, rest.chain(away));
        assert_eq!(events.len(), 1);
        assert_eq!(detector.takeoff_time_s(), Some(9.0));
    }

    #[test]
    fn short_speed_spike_without_terrain_is_no_takeoff() {
        let mut detector = detector();
        let samples = (0..60).map(|t| {
            let speed = if (20..23).contains(&t) { 6.0 } else { 0.0 };
            sample(t as f64, speed, 0.0, None)
        });
        assert!(run(&mut detector, samples).is_empty());
    }

    #[test]
    fn landing_after_thirty_calm_seconds_near_ground() {
        let mut detector = detector();
        launch(&mut detector);
        let glide = (60..120).map(|t| sample(t as f64, 9.0, -1.0, Some(60.0 - (t - 60) as f64)));
        let standing = (120..200).map(|t| sample(t as f64, 0.3, 0.1, Some(1.0)));
        let events = run(&mut detector, glide.chain(standing));
        assert_eq!(events, vec![PhaseEvent::Landing { time_s: 120.0 }]);
        assert_eq!(detector.phase(), Phase::Landed);
        assert_eq!(detector.landing_time_s(), Some(120.0));
    }

    #[test]
    fn soaring_high_with_no_ground_speed_is_no_landing() {
        // Strong wind on the ridge: the glider hangs still over the slope for five
        // minutes, neither climbing nor sinking.
        let mut detector = detector();
        launch(&mut detector);
        let soaring = (60..360).map(|t| sample(t as f64, 0.5, 0.2, Some(150.0)));
        assert!(run(&mut detector, soaring).is_empty());
        assert_eq!(detector.phase(), Phase::Airborne);
    }

    #[test]
    fn without_terrain_landing_needs_sixty_seconds() {
        let mut detector = detector();
        let rest = (0..10).map(|t| sample(t as f64, 0.0, 0.0, None));
        let flight = (10..100).map(|t| sample(t as f64, 9.0, -1.0, None));
        run(&mut detector, rest.chain(flight));
        let calm = |range: std::ops::Range<i32>| range.map(|t| sample(t as f64, 0.5, 0.0, None));
        assert!(run(&mut detector, calm(100..146)).is_empty());
        assert_eq!(detector.phase(), Phase::Airborne);
        assert_eq!(
            run(&mut detector, calm(146..162)),
            vec![PhaseEvent::Landing { time_s: 100.0 }]
        );
    }

    #[test]
    fn gps_loss_in_the_air_keeps_airborne() {
        let mut detector = detector();
        launch(&mut detector);
        // 20 s calm near the ground, then two minutes without any sample, then calm
        // again for 20 s: neither part is long enough, and the gap does not join them.
        let before = (60..80).map(|t| sample(t as f64, 0.5, 0.0, Some(5.0)));
        let after = (200..220).map(|t| sample(t as f64, 0.5, 0.0, Some(5.0)));
        assert!(run(&mut detector, before.chain(after)).is_empty());
        assert_eq!(detector.phase(), Phase::Airborne);
    }

    #[test]
    fn unknown_speed_is_no_landing() {
        let mut detector = detector();
        launch(&mut detector);
        let no_speed = (60..200).map(|t| PhaseSample {
            ground_speed_mps: None,
            ..sample(t as f64, 0.0, 0.0, Some(1.0))
        });
        assert!(run(&mut detector, no_speed).is_empty());
    }

    #[test]
    fn unknown_climb_counts_as_calm_for_landing() {
        let mut detector = detector();
        launch(&mut detector);
        let no_climb = (60..100).map(|t| PhaseSample {
            climb_rate_mps: None,
            ..sample(t as f64, 0.3, 0.0, Some(1.0))
        });
        assert_eq!(
            run(&mut detector, no_climb),
            vec![PhaseEvent::Landing { time_s: 60.0 }]
        );
    }

    #[test]
    fn relaunch_after_top_landing() {
        let mut detector = detector();
        launch(&mut detector);
        // Top landing: calm near the ground for 40 s, then a new launch.
        let top = (60..100).map(|t| sample(t as f64, 0.3, 0.0, Some(1.0)));
        let again = (100..140).map(|t| {
            let s = (t - 100) as f64;
            sample(t as f64, 9.0, -0.5, Some(1.0 + 2.0 * s))
        });
        let events = run(&mut detector, top.chain(again));
        assert_eq!(events.len(), 2, "{events:?}");
        assert_eq!(events[0], PhaseEvent::Landing { time_s: 60.0 });
        assert!(matches!(events[1], PhaseEvent::Takeoff { time_s, .. } if time_s == 99.0));
        assert_eq!(detector.phase(), Phase::Airborne);
        assert_eq!(detector.landing_time_s(), None);
    }

    #[test]
    fn takeoff_speed_can_be_changed() {
        let mut detector = detector();
        detector.set_takeoff_speed(8.0);
        let samples = (0..20).map(|t| sample(t as f64, 6.0, 0.0, None));
        assert!(run(&mut detector, samples).is_empty());
    }
}
