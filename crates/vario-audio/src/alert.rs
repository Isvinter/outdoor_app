//! Warning sounds, e.g. for airspaces.
//!
//! They must not be mistaken for the vario: the vario beeps on one pitch or hums low,
//! a warning alternates between two pitches. Like [`crate::synth`], this runs in the
//! audio callback: no allocation, no locks, and every change of pitch or level is faded.

use std::f32::consts::TAU;

/// Which warning to play.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Alert {
    #[default]
    None,
    /// A short two-tone call, repeated every 30 s.
    Near,
    /// Fast two-tone calls, repeated every 4 s.
    Alarm,
}

impl Alert {
    /// For storing in an atomic.
    pub fn to_u8(self) -> u8 {
        match self {
            Alert::None => 0,
            Alert::Near => 1,
            Alert::Alarm => 2,
        }
    }

    pub fn from_u8(value: u8) -> Alert {
        match value {
            1 => Alert::Near,
            2 => Alert::Alarm,
            _ => Alert::None,
        }
    }
}

/// A burst of notes `(frequency_hz, seconds)`, repeated after `repeat_s` from its start.
struct Pattern {
    notes: &'static [(f32, f32)],
    repeat_s: f32,
}

const NEAR: Pattern = Pattern {
    notes: &[(740.0, 0.16), (988.0, 0.16), (740.0, 0.16), (988.0, 0.16)],
    repeat_s: 30.0,
};

const ALARM: Pattern = Pattern {
    notes: &[
        (1047.0, 0.1),
        (784.0, 0.1),
        (1047.0, 0.1),
        (784.0, 0.1),
        (1047.0, 0.1),
        (784.0, 0.1),
        (1047.0, 0.1),
        (784.0, 0.1),
    ],
    repeat_s: 4.0,
};

/// Short pause at the end of each note, so that the notes are heard separately.
const NOTE_GAP_S: f32 = 0.02;
/// Fade time at the start and end of each note.
const FADE_S: f32 = 0.004;
/// Time constant of volume changes.
const VOLUME_SMOOTHING_S: f32 = 0.02;
/// Peak of the waveform at full volume; leaves room for the ducked vario tone.
const LEVEL: f32 = 0.75;

impl Pattern {
    /// Frequency of the note sounding `position_s` after the start of a burst, `None`
    /// between notes and after the burst.
    fn note_at(&self, position_s: f32) -> Option<f32> {
        let mut start = 0.0;
        for &(frequency_hz, length_s) in self.notes {
            if position_s < start + length_s {
                return (position_s < start + length_s - NOTE_GAP_S).then_some(frequency_hz);
            }
            start += length_s;
        }
        None
    }

    fn burst_s(&self) -> f32 {
        self.notes.iter().map(|&(_, length_s)| length_s).sum()
    }
}

/// Generates the warning sounds sample by sample.
#[derive(Debug, Clone)]
pub struct AlertSynth {
    sample_rate: f32,
    alert: Alert,
    /// Seconds since the current pattern started.
    time_s: f32,
    phase: f32,
    frequency_hz: f32,
    envelope: f32,
    volume: f32,
    target_volume: f32,
    fade_step: f32,
    volume_coefficient: f32,
}

impl AlertSynth {
    pub fn new(sample_rate_hz: u32) -> Self {
        let sample_rate = sample_rate_hz as f32;
        AlertSynth {
            sample_rate,
            alert: Alert::None,
            time_s: 0.0,
            phase: 0.0,
            frequency_hz: 0.0,
            envelope: 0.0,
            volume: 1.0,
            target_volume: 1.0,
            fade_step: 1.0 / (FADE_S * sample_rate),
            volume_coefficient: 1.0 - (-1.0 / (VOLUME_SMOOTHING_S * sample_rate)).exp(),
        }
    }

    /// Switches the warning. A new or changed warning starts its pattern right away.
    pub fn set_alert(&mut self, alert: Alert) {
        if alert != self.alert {
            self.alert = alert;
            self.time_s = 0.0;
        }
    }

    pub fn alert(&self) -> Alert {
        self.alert
    }

    /// Volume from 0.0 to 1.0.
    pub fn set_volume(&mut self, volume: f32) {
        self.target_volume = if volume.is_finite() {
            volume.clamp(0.0, 1.0)
        } else {
            0.0
        };
    }

    /// Whether a burst is playing right now (the vario tone is turned down meanwhile).
    pub fn in_burst(&self) -> bool {
        match self.pattern() {
            Some(pattern) => self.time_s % pattern.repeat_s < pattern.burst_s(),
            None => self.envelope > 0.0,
        }
    }

    fn pattern(&self) -> Option<&'static Pattern> {
        match self.alert {
            Alert::None => None,
            Alert::Near => Some(&NEAR),
            Alert::Alarm => Some(&ALARM),
        }
    }

    /// The next audio sample.
    pub fn next_sample(&mut self) -> f32 {
        let note = self.pattern().and_then(|pattern| {
            let note = pattern.note_at(self.time_s % pattern.repeat_s);
            self.time_s += 1.0 / self.sample_rate;
            // Keep the time small so that f32 stays precise during long warnings.
            if self.time_s >= pattern.repeat_s {
                self.time_s -= pattern.repeat_s;
            }
            note
        });
        if let Some(frequency_hz) = note
            && self.envelope == 0.0
        {
            // Each note starts from silence, so the pitch can jump.
            self.frequency_hz = frequency_hz;
        }
        // A note only sounds once the previous one has faded out completely.
        let open = note.is_some_and(|f| f == self.frequency_hz);
        if open {
            self.envelope = (self.envelope + self.fade_step).min(1.0);
        } else {
            self.envelope = (self.envelope - self.fade_step).max(0.0);
        }
        self.volume += (self.target_volume - self.volume) * self.volume_coefficient;

        self.phase += self.frequency_hz / self.sample_rate;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        let angle = TAU * self.phase;
        // Odd harmonics make it sound harder than the vario tone.
        let wave = (angle.sin() + 0.25 * (3.0 * angle).sin() + 0.1 * (5.0 * angle).sin()) / 1.2;
        let fade = self.envelope * self.envelope * (3.0 - 2.0 * self.envelope);
        wave * fade * self.volume * LEVEL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_RATE: u32 = 48_000;

    fn render(synth: &mut AlertSynth, seconds: f32) -> Vec<f32> {
        (0..(seconds * SAMPLE_RATE as f32) as usize)
            .map(|_| synth.next_sample())
            .collect()
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0, |peak, s| peak.max(s.abs()))
    }

    /// Counts the rising zero crossings, i.e. the cycles of the fundamental.
    fn cycles(samples: &[f32]) -> usize {
        samples
            .windows(2)
            .filter(|pair| pair[0] < 0.0 && pair[1] >= 0.0)
            .count()
    }

    #[test]
    fn silent_without_alert() {
        let mut synth = AlertSynth::new(SAMPLE_RATE);
        assert_eq!(peak(&render(&mut synth, 1.0)), 0.0);
    }

    #[test]
    fn near_plays_two_pitches_once_and_then_waits() {
        let mut synth = AlertSynth::new(SAMPLE_RATE);
        synth.set_alert(Alert::Near);
        assert!(synth.in_burst());
        let first_note = render(&mut synth, 0.14);
        let second_note = render(&mut synth, 0.16);
        // 740 Hz for 0.14 s is about 104 cycles; the second window holds the gap and
        // 0.14 s of 988 Hz, about 138 cycles.
        assert!((100..=106).contains(&cycles(&first_note)));
        assert!((134..=142).contains(&cycles(&second_note)));
        assert!(peak(&first_note) > 0.5);
        render(&mut synth, 0.5);
        assert!(!synth.in_burst());
        // Silent until the repeat after 30 s.
        assert_eq!(peak(&render(&mut synth, 29.0)), 0.0);
        render(&mut synth, 0.2);
        assert!(peak(&render(&mut synth, 0.1)) > 0.5, "repeated");
    }

    #[test]
    fn alarm_repeats_quickly_and_ends_softly() {
        let mut synth = AlertSynth::new(SAMPLE_RATE);
        synth.set_alert(Alert::Alarm);
        render(&mut synth, 4.05);
        assert!(peak(&render(&mut synth, 0.05)) > 0.5, "second burst");
        synth.set_alert(Alert::None);
        let tail = render(&mut synth, 0.05);
        assert_eq!(*tail.last().unwrap(), 0.0);
        // Fading out takes a few milliseconds, no jumps.
        let max_step = tail
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).abs())
            .fold(0.0, f32::max);
        assert!(max_step < 0.2, "{max_step}");
    }

    #[test]
    fn notes_start_and_end_without_clicks() {
        let mut synth = AlertSynth::new(SAMPLE_RATE);
        synth.set_alert(Alert::Alarm);
        let samples = render(&mut synth, 1.0);
        // Steepest slope of a clean 1047 Hz tone with harmonics at this level is about
        // 2π·f/sr·(1 + 0.75 + 0.5)/1.2·0.75 ≈ 0.19 per sample.
        let max_step = samples
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).abs())
            .fold(0.0, f32::max);
        assert!(max_step < 0.22, "{max_step}");
    }
}
