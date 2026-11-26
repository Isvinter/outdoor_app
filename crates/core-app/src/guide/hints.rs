//! The announcer: decides fix by fix which turn hint applies. Each turn is
//! told at most twice, ahead of it and when it is here, timed by the class of
//! the profile and the speed. It never changes the route.

use std::collections::VecDeque;

use core_routing::{Profile, TurnKind};

use super::line::LineTurn;
use super::turn_text::{HintStage, distance_text, label, spoken};

/// How far back fixes count for the speed.
const SPEED_WINDOW_MS: i64 = 5000;
/// This far past a turn, the user has passed it.
const PASSED_M: f64 = 10.0;

/// When the hints of a profile class are told; see "Abbiegehinweise" in
/// `docs/ALGORITHMEN.md`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HintParams {
    /// The announcement ahead comes this many seconds before the turn …
    pub ahead_s: f64,
    /// … but at least this far before it.
    pub ahead_min_m: f64,
    /// From this speed on, the announcement ahead comes …
    pub fast_from_mps: Option<f64>,
    /// … at least this far before the turn.
    pub fast_ahead_m: f64,
    /// The hint "now" comes this many seconds before the turn …
    pub now_s: f64,
    /// … but at least this far before it.
    pub now_min_m: f64,
    /// The speed without a usable one from the fixes; `None` where the
    /// distances do not depend on the speed.
    pub default_speed_mps: Option<f64>,
}

impl HintParams {
    /// The values of the class of `profile`; `None` for [`Profile::Manual`],
    /// which has no turns.
    #[must_use]
    pub fn for_profile(profile: Profile) -> Option<HintParams> {
        let hike = HintParams {
            ahead_s: 0.0,
            ahead_min_m: 50.0,
            fast_from_mps: None,
            fast_ahead_m: 0.0,
            now_s: 0.0,
            now_min_m: 12.0,
            default_speed_mps: None,
        };
        match profile {
            Profile::Hike => Some(hike),
            Profile::RoadBike | Profile::Trekking | Profile::Mtb => Some(HintParams {
                ahead_s: 25.0,
                ahead_min_m: 100.0,
                now_s: 6.0,
                now_min_m: 30.0,
                default_speed_mps: Some(4.5),
                ..hike
            }),
            Profile::Car => Some(HintParams {
                ahead_s: 35.0,
                ahead_min_m: 400.0,
                fast_from_mps: Some(22.0),
                fast_ahead_m: 1500.0,
                now_s: 9.0,
                now_min_m: 100.0,
                default_speed_mps: Some(14.0),
            }),
            Profile::Manual => None,
        }
    }

    /// How far before a turn the announcement ahead comes at `speed_mps`.
    fn ahead_m(&self, speed_mps: f64) -> f64 {
        let by_time = self.ahead_s * speed_mps;
        if self.fast_from_mps.is_some_and(|fast| speed_mps >= fast) {
            return by_time.max(self.fast_ahead_m);
        }
        by_time.max(self.ahead_min_m)
    }

    /// How far before a turn the hint "now" comes at `speed_mps`.
    fn now_m(&self, speed_mps: f64) -> f64 {
        (self.now_s * speed_mps).max(self.now_min_m)
    }
}

/// The hint for the next turn.
#[derive(Debug, Clone, PartialEq)]
pub struct TurnHint {
    pub kind: TurnKind,
    /// Number of the exit to take at a roundabout.
    pub exit: Option<u8>,
    /// A second turn right behind this one, told together with it.
    pub then: Option<(TurnKind, Option<u8>)>,
    pub stage: HintStage,
    /// Distance to the turn, at the latest fix.
    pub distance_m: f64,
    /// [`Self::distance_m`] as told to the user.
    pub distance_text: String,
    /// The name of the turn, such as "rechts".
    pub label: String,
    /// The sentence from the moment the hint was told.
    pub spoken: String,
}

/// A hint told for one turn.
#[derive(Debug, Clone)]
struct Told {
    /// Index of the turn.
    turn: usize,
    stage: HintStage,
    then: Option<(TurnKind, Option<u8>)>,
    spoken: String,
}

/// One fix for the speed.
#[derive(Debug, Clone, Copy)]
struct Sample {
    time_ms: i64,
    along_m: f64,
    /// The speed at this fix, from the GPS or from the progress since the
    /// fix before.
    speed_mps: Option<f64>,
}

/// Decides which hint applies along the turns of one route line.
#[derive(Debug, Clone)]
pub struct Announcer {
    turns: Vec<LineTurn>,
    /// The stage reached so far for each turn; it never goes back.
    reached: Vec<Option<HintStage>>,
    /// Index of the first turn not yet passed.
    next: usize,
    told: Option<Told>,
    /// Counts up with every hint told.
    seq: u32,
    samples: VecDeque<Sample>,
}

impl Announcer {
    /// An announcer for `turns`, sorted by where they lie.
    #[must_use]
    pub fn new(turns: &[LineTurn]) -> Announcer {
        debug_assert!(
            turns.windows(2).all(|w| w[0].at_m <= w[1].at_m),
            "turns are sorted by where they lie"
        );
        Announcer {
            turns: turns.to_vec(),
            reached: vec![None; turns.len()],
            next: 0,
            told: None,
            seq: 0,
            samples: VecDeque::new(),
        }
    }

    /// The number of hints told so far.
    #[must_use]
    pub fn seq(&self) -> u32 {
        self.seq
    }

    /// Takes in the progress `along_m` at `time_ms` with the speed of the
    /// fix, on a leg of `profile`; the hint that applies now.
    pub fn update(
        &mut self,
        along_m: f64,
        speed_mps: Option<f32>,
        time_ms: i64,
        profile: Profile,
    ) -> Option<TurnHint> {
        let speed = self.speed_mps(along_m, speed_mps, time_ms);
        while self.is_passed(self.next, along_m) {
            self.next += 1;
        }
        if self.told.as_ref().is_some_and(|t| t.turn != self.next) {
            self.told = None;
        }
        let index = self.next;
        let turn = *self.turns.get(index)?;
        // A stage end is told on any leg, also on a straight one, with the
        // fixed distances of walking there.
        let params = HintParams::for_profile(profile).or_else(|| {
            (turn.kind == TurnKind::StageEnd)
                .then(|| HintParams::for_profile(Profile::Hike))
                .flatten()
        })?;
        let speed = speed.or(params.default_speed_mps).unwrap_or(0.0);
        let distance_m = turn.at_m - along_m;
        let now_m = params.now_m(speed);
        let due = if distance_m <= now_m {
            Some(HintStage::Now)
        } else if distance_m <= params.ahead_m(speed) && turn.kind != TurnKind::StageEnd {
            Some(HintStage::Ahead)
        } else {
            None
        };
        if let Some(stage) = due.filter(|&stage| self.reached[index] < Some(stage)) {
            self.reached[index] = Some(stage);
            // A turn right behind this one is told with it, and gets no
            // announcement of its own. A stage end is always told alone; the
            // turn after it comes as soon as it is passed.
            let then = self
                .turns
                .get(index + 1)
                .filter(|after| after.at_m - turn.at_m <= now_m && turn.kind != TurnKind::StageEnd);
            if let Some(after) = then {
                // Within the distance for "passed", its "now" is this one;
                // a stage end is still told on its own.
                let with = if stage == HintStage::Now
                    && after.at_m - turn.at_m < PASSED_M
                    && after.kind != TurnKind::StageEnd
                {
                    HintStage::Now
                } else {
                    HintStage::Ahead
                };
                let reached = &mut self.reached[index + 1];
                *reached = (*reached).max(Some(with));
            }
            let then = then.map(|after| (after.kind, after.exit));
            self.seq += 1;
            self.told = Some(Told {
                turn: index,
                stage,
                then,
                spoken: spoken(stage, distance_m, (turn.kind, turn.exit), then),
            });
        }
        // Announced ahead together with the turn before: shown until its
        // own "now", but not told again.
        if self.told.is_none() && self.reached[index] == Some(HintStage::Ahead) {
            self.told = Some(Told {
                turn: index,
                stage: HintStage::Ahead,
                then: None,
                spoken: spoken(HintStage::Ahead, distance_m, (turn.kind, turn.exit), None),
            });
        }
        let told = self.told.clone()?;
        let distance_m = distance_m.max(0.0);
        Some(TurnHint {
            kind: turn.kind,
            exit: turn.exit,
            then: told.then,
            stage: told.stage,
            distance_m,
            distance_text: distance_text(distance_m),
            label: label(turn.kind, turn.exit),
            spoken: told.spoken,
        })
    }

    /// Whether the user has passed turn `index` at `along_m`: 10 m behind
    /// it, or at it once its "now" is told and a stage end follows right
    /// behind, so that the stage end is told in time.
    fn is_passed(&self, index: usize, along_m: f64) -> bool {
        let Some(turn) = self.turns.get(index) else {
            return false;
        };
        let stage_end_follows = self.turns.get(index + 1).is_some_and(|after| {
            after.kind == TurnKind::StageEnd && after.at_m - turn.at_m < PASSED_M
        });
        let told_now = self.reached[index] == Some(HintStage::Now);
        along_m >= turn.at_m + PASSED_M
            || ((stage_end_follows || turn.kind == TurnKind::StageEnd)
                && told_now
                && along_m >= turn.at_m)
    }

    /// The mean speed over the fixes of the last [`SPEED_WINDOW_MS`], with
    /// the fix at `time_ms`; `None` without any.
    fn speed_mps(&mut self, along_m: f64, speed_mps: Option<f32>, time_ms: i64) -> Option<f64> {
        let measured = speed_mps
            .map(f64::from)
            .filter(|s| s.is_finite() && *s >= 0.0);
        while self
            .samples
            .front()
            .is_some_and(|s| s.time_ms < time_ms - SPEED_WINDOW_MS)
        {
            self.samples.pop_front();
        }
        // Only from a fix inside the window, not across a gap.
        let derived = self.samples.back().and_then(|last| {
            let dt_s = (time_ms - last.time_ms) as f64 / 1000.0;
            (dt_s > 0.0).then(|| ((along_m - last.along_m) / dt_s).max(0.0))
        });
        self.samples.push_back(Sample {
            time_ms,
            along_m,
            speed_mps: measured.or(derived),
        });
        let speeds: Vec<f64> = self.samples.iter().filter_map(|s| s.speed_mps).collect();
        (!speeds.is_empty()).then(|| speeds.iter().sum::<f64>() / speeds.len() as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use TurnKind::*;

    fn turn(at_m: f64, kind: TurnKind) -> LineTurn {
        LineTurn {
            at_m,
            kind,
            exit: None,
            leg: 0,
        }
    }

    /// A hint told: where, in which stage and what was said.
    #[derive(Debug, Clone, PartialEq)]
    struct Event {
        along_m: f64,
        stage: HintStage,
        spoken: String,
    }

    /// Feeds `(along_m, speed_mps, t_s)`; the hints told on the way and the
    /// hint after each fix.
    fn feed(
        announcer: &mut Announcer,
        profile: Profile,
        fixes: impl IntoIterator<Item = (f64, Option<f32>, f64)>,
    ) -> (Vec<Event>, Vec<(f64, Option<TurnHint>)>) {
        let mut events = Vec::new();
        let mut hints = Vec::new();
        for (along_m, speed, t_s) in fixes {
            let seq = announcer.seq();
            let hint = announcer.update(along_m, speed, (t_s * 1000.0) as i64, profile);
            if announcer.seq() > seq {
                assert_eq!(announcer.seq(), seq + 1, "one hint per fix");
                let hint = hint.clone().expect("a told hint is shown");
                events.push(Event {
                    along_m,
                    stage: hint.stage,
                    spoken: hint.spoken,
                });
            }
            hints.push((along_m, hint));
        }
        (events, hints)
    }

    /// From `from_m` to `to_m` in 1 m steps at `speed_mps`, starting at `t_s`.
    fn ride(
        from_m: f64,
        to_m: f64,
        speed_mps: f32,
        t_s: f64,
    ) -> impl Iterator<Item = (f64, Option<f32>, f64)> {
        let steps = (to_m - from_m).round() as usize;
        (0..=steps).map(move |i| {
            let i = i as f64;
            (from_m + i, Some(speed_mps), t_s + i / f64::from(speed_mps))
        })
    }

    fn event(along_m: f64, stage: HintStage, spoken: &str) -> Event {
        Event {
            along_m,
            stage,
            spoken: spoken.into(),
        }
    }

    fn hint_at(hints: &[(f64, Option<TurnHint>)], along_m: f64) -> Option<&TurnHint> {
        hints
            .iter()
            .find(|(m, _)| (*m - along_m).abs() < 1e-9)
            .unwrap_or_else(|| panic!("no fix at {along_m}"))
            .1
            .as_ref()
    }

    #[test]
    fn hike_tells_at_fixed_distances() {
        let mut announcer = Announcer::new(&[turn(300.0, Right)]);
        let (events, hints) = feed(&mut announcer, Profile::Hike, ride(0.0, 330.0, 1.2, 0.0));
        assert_eq!(
            events,
            [
                event(250.0, HintStage::Ahead, "In 50 m rechts"),
                event(288.0, HintStage::Now, "Jetzt rechts"),
            ]
        );
        assert_eq!(announcer.seq(), 2);
        assert_eq!(hint_at(&hints, 249.0), None);
        // The distance counts down; the sentence stays.
        let hint = hint_at(&hints, 263.0).unwrap();
        assert_eq!(hint.distance_m, 37.0);
        assert_eq!(hint.distance_text, "40 m");
        assert_eq!(hint.label, "rechts");
        assert_eq!(hint.spoken, "In 50 m rechts");
        assert_eq!(hint.kind, Right);
        assert_eq!(hint_at(&hints, 300.0).unwrap().distance_m, 0.0);
        assert_eq!(hint_at(&hints, 309.0).unwrap().distance_m, 0.0);
        assert_eq!(hint_at(&hints, 310.0), None);
        assert_eq!(hint_at(&hints, 330.0), None);
    }

    #[test]
    fn bike_scales_with_speed() {
        let mut announcer = Announcer::new(&[turn(300.0, Left)]);
        let (events, _) = feed(
            &mut announcer,
            Profile::Trekking,
            ride(0.0, 320.0, 8.0, 0.0),
        );
        assert_eq!(
            events,
            [
                event(100.0, HintStage::Ahead, "In 200 m links"),
                event(252.0, HintStage::Now, "Jetzt links"),
            ]
        );

        let mut announcer = Announcer::new(&[turn(300.0, Left)]);
        let (events, _) = feed(&mut announcer, Profile::Mtb, ride(0.0, 320.0, 3.0, 0.0));
        assert_eq!(
            events,
            [
                event(200.0, HintStage::Ahead, "In 100 m links"),
                event(270.0, HintStage::Now, "Jetzt links"),
            ]
        );
    }

    #[test]
    fn car_fast_announces_far() {
        let mut announcer = Announcer::new(&[turn(3000.0, SlightRight)]);
        let (events, _) = feed(&mut announcer, Profile::Car, ride(0.0, 3020.0, 25.0, 0.0));
        assert_eq!(
            events,
            [
                event(1500.0, HintStage::Ahead, "In 1,5 km leicht rechts"),
                event(2775.0, HintStage::Now, "Jetzt leicht rechts"),
            ]
        );

        // Slower, by time.
        let mut announcer = Announcer::new(&[turn(3000.0, SlightRight)]);
        let (events, _) = feed(&mut announcer, Profile::Car, ride(0.0, 3020.0, 15.0, 0.0));
        assert_eq!(events[0].along_m, 2475.0);
        assert_eq!(events[1].along_m, 2865.0);
    }

    #[test]
    fn stop_before_turn_tells_each_once() {
        let mut announcer = Announcer::new(&[turn(300.0, Right)]);
        let profile = Profile::RoadBike;
        let (events, _) = feed(&mut announcer, profile, ride(0.0, 260.0, 3.0, 0.0));
        assert_eq!(events, [event(200.0, HintStage::Ahead, "In 100 m rechts")]);
        // 60 s standing 40 m before the turn.
        let t0 = 260.0 / 3.0;
        let stand = (1..=60).map(|i| (260.0, Some(0.0), t0 + f64::from(i)));
        let (events, hints) = feed(&mut announcer, profile, stand);
        assert_eq!(events, []);
        assert!(hints.iter().all(|(_, h)| h.is_some()), "{hints:?}");
        let (events, _) = feed(&mut announcer, profile, ride(261.0, 320.0, 3.0, t0 + 61.0));
        assert_eq!(events, [event(270.0, HintStage::Now, "Jetzt rechts")]);
        assert_eq!(announcer.seq(), 2);
    }

    #[test]
    fn jitter_at_turn_tells_once() {
        let mut announcer = Announcer::new(&[turn(300.0, Left)]);
        let profile = Profile::Hike;
        let (events, _) = feed(&mut announcer, profile, ride(0.0, 280.0, 1.2, 0.0));
        assert_eq!(events.len(), 1);
        let jitter = [-5.0, 3.0, -4.0, 8.0, 12.0, 8.0, 15.0]
            .into_iter()
            .enumerate()
            .map(|(i, d)| (300.0 + d, Some(1.2), 240.0 + i as f64));
        let (events, hints) = feed(&mut announcer, profile, jitter);
        assert_eq!(events, [event(295.0, HintStage::Now, "Jetzt links")]);
        let shown: Vec<bool> = hints.iter().map(|(_, h)| h.is_some()).collect();
        assert_eq!(shown, [true, true, true, true, false, false, false]);
        assert_eq!(announcer.seq(), 2);
    }

    #[test]
    fn first_turn_inside_now_distance_is_now() {
        let mut announcer = Announcer::new(&[turn(300.0, Right), turn(900.0, Left)]);
        let (events, _) = feed(
            &mut announcer,
            Profile::Trekking,
            ride(280.0, 320.0, 4.0, 0.0),
        );
        assert_eq!(events, [event(280.0, HintStage::Now, "Jetzt rechts")]);
    }

    #[test]
    fn close_turns_are_told_together() {
        let turns = [turn(300.0, Right), turn(320.0, Left)];
        let mut announcer = Announcer::new(&turns);
        let (events, hints) = feed(
            &mut announcer,
            Profile::Trekking,
            ride(0.0, 350.0, 3.0, 0.0),
        );
        assert_eq!(
            events,
            [
                event(
                    200.0,
                    HintStage::Ahead,
                    "In 100 m rechts, dann gleich links"
                ),
                event(270.0, HintStage::Now, "Jetzt rechts, dann gleich links"),
                event(310.0, HintStage::Now, "Jetzt links"),
            ]
        );
        assert_eq!(hint_at(&hints, 250.0).unwrap().then, Some((Left, None)));
        assert_eq!(hint_at(&hints, 329.0).unwrap().kind, Left);
        assert_eq!(hint_at(&hints, 330.0), None);

        // Past the second one before it was due: nothing more.
        let mut announcer = Announcer::new(&turns);
        let fixes = ride(0.0, 305.0, 3.0, 0.0).chain([(335.0, Some(3.0), 120.0)]);
        let (events, hints) = feed(&mut announcer, Profile::Trekking, fixes);
        assert_eq!(events.len(), 2, "{events:?}");
        assert_eq!(hints.last().unwrap().1, None);
    }

    #[test]
    fn close_turn_told_ahead_stays_shown() {
        // At 10 m/s, 60 m count as close; slowing down after the first turn,
        // the second one is not yet due when the first is passed.
        let mut announcer = Announcer::new(&[turn(300.0, Right), turn(360.0, Left)]);
        let fixes = ride(0.0, 300.0, 10.0, 0.0).chain(ride(301.0, 380.0, 3.0, 30.1));
        let (events, hints) = feed(&mut announcer, Profile::Trekking, fixes);
        let spoken: Vec<&str> = events.iter().map(|e| e.spoken.as_str()).collect();
        assert_eq!(
            spoken,
            [
                "In 250 m rechts, dann gleich links",
                "Jetzt rechts, dann gleich links",
                "Jetzt links",
            ]
        );
        assert!(events[2].along_m > 310.0, "{events:?}");
        let shown = hint_at(&hints, 310.0).unwrap();
        assert_eq!((shown.kind, shown.stage), (Left, HintStage::Ahead));
    }

    #[test]
    fn turn_within_passed_distance_is_told_with_the_one_before() {
        let mut announcer = Announcer::new(&[turn(300.0, Right), turn(305.0, Left)]);
        let (events, hints) = feed(
            &mut announcer,
            Profile::Trekking,
            ride(0.0, 330.0, 3.0, 0.0),
        );
        assert_eq!(
            events,
            [
                event(
                    200.0,
                    HintStage::Ahead,
                    "In 100 m rechts, dann gleich links"
                ),
                event(270.0, HintStage::Now, "Jetzt rechts, dann gleich links"),
            ]
        );
        assert_eq!(announcer.seq(), 2);
        assert_eq!(hint_at(&hints, 312.0), None);
    }

    #[test]
    fn stage_end_at_a_turn_is_told_after_it() {
        let mut announcer = Announcer::new(&[turn(300.0, Right), turn(300.0, StageEnd)]);
        let (events, _) = feed(&mut announcer, Profile::Hike, ride(0.0, 330.0, 1.2, 0.0));
        assert_eq!(
            events,
            [
                event(250.0, HintStage::Ahead, "In 50 m rechts"),
                event(288.0, HintStage::Now, "Jetzt rechts"),
                event(300.0, HintStage::Now, "Etappenziel erreicht"),
            ]
        );
    }

    #[test]
    fn speed_after_a_gap_is_not_derived_across_it() {
        // Without GPS speed, after a gap of 60 s the profile default applies.
        for (along_m, told) in [(188.0, true), (187.0, false)] {
            let mut announcer = Announcer::new(&[turn(300.0, Right)]);
            announcer.update(0.0, None, 0, Profile::RoadBike);
            // 60 s later, far ahead: derived across the gap it would be
            // about 3 m/s, so the announcement would wait for 100 m.
            let hint = announcer.update(along_m, None, 60_000, Profile::RoadBike);
            assert_eq!(hint.is_some(), told, "{along_m}");
        }
    }

    #[test]
    fn speed_without_gps_uses_profile_default() {
        // At 4.5 m/s the announcement comes 112.5 m before the turn.
        for (along_m, told) in [(188.0, true), (187.0, false)] {
            let mut announcer = Announcer::new(&[turn(300.0, Right)]);
            let hint = announcer.update(along_m, None, 0, Profile::RoadBike);
            assert_eq!(hint.is_some(), told, "{along_m}");
        }
    }

    #[test]
    fn stage_end_is_told_only_now() {
        let mut announcer = Announcer::new(&[turn(300.0, StageEnd)]);
        let (events, _) = feed(&mut announcer, Profile::Hike, ride(0.0, 320.0, 1.2, 0.0));
        assert_eq!(
            events,
            [event(288.0, HintStage::Now, "Etappenziel erreicht")]
        );
    }

    #[test]
    fn turn_right_after_stage_end_is_told() {
        let mut announcer = Announcer::new(&[turn(300.0, StageEnd), turn(304.0, Right)]);
        let (events, _) = feed(&mut announcer, Profile::Hike, ride(0.0, 330.0, 1.2, 0.0));
        assert_eq!(
            events,
            [
                event(288.0, HintStage::Now, "Etappenziel erreicht"),
                event(300.0, HintStage::Now, "Jetzt rechts"),
            ]
        );
    }

    #[test]
    fn stage_end_on_a_straight_leg_is_told() {
        let mut announcer = Announcer::new(&[turn(300.0, StageEnd)]);
        let (events, _) = feed(&mut announcer, Profile::Manual, ride(0.0, 320.0, 1.2, 0.0));
        assert_eq!(
            events,
            [event(288.0, HintStage::Now, "Etappenziel erreicht")]
        );
    }

    #[test]
    fn manual_legs_have_no_hints() {
        assert_eq!(HintParams::for_profile(Profile::Manual), None);
        let mut announcer = Announcer::new(&[turn(300.0, Right)]);
        let (events, hints) = feed(&mut announcer, Profile::Manual, ride(0.0, 320.0, 1.2, 0.0));
        assert_eq!(events, []);
        assert!(hints.iter().all(|(_, h)| h.is_none()));
    }
}
