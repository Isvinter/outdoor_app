//! Detection of rests from the stream of fixes.

use crate::TrackPoint;
use crate::geo::haversine_m;

/// When standing counts as a rest.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RestParams {
    /// Time to stay within `radius_m` of the rest point before a rest begins.
    pub dwell_ms: i64,
    /// Radius around the rest point, in metres.
    pub radius_m: f64,
}

impl RestParams {
    /// Five minutes within 50 m.
    pub const DEFAULT: RestParams = RestParams {
        dwell_ms: 300_000,
        radius_m: 50.0,
    };
}

/// A change reported by [`RestDetector::offer`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestEvent {
    Started,
    Ended,
}

/// State machine over the fixes of a recording that recognises rests.
///
/// Before a rest, only usable fixes count: those with a reported accuracy within the
/// radius, or none reported. GPS jitter of a poor fix can neither fake a rest nor move
/// the rest point. The first usable fix after a reset becomes the rest point; a usable
/// fix farther than the radius from it becomes the new rest point. The rest starts with
/// the first usable fix within the radius at least `dwell_ms` after the rest point.
///
/// During a rest the receiver may be throttled to coarse fixes, so every fix counts,
/// with its accuracy as a margin: a fix is away when it lies farther than radius plus
/// accuracy (0 if unknown) from the rest point. Two away fixes in a row end the rest;
/// a fix within in between starts the count over, so a single jump ends nothing.
#[derive(Debug, Clone)]
pub struct RestDetector {
    params: RestParams,
    /// Rest point: position and time of the fix the dwell time is counted from.
    anchor: Option<(f64, f64, i64)>,
    resting: bool,
    /// Consecutive away fixes during a rest.
    away: u32,
}

impl RestDetector {
    pub fn new(params: RestParams) -> Self {
        Self {
            params,
            anchor: None,
            resting: false,
            away: 0,
        }
    }

    /// Feeds the next fix. Fixes without a time are ignored.
    pub fn offer(&mut self, p: &TrackPoint) -> Option<RestEvent> {
        let time_ms = p.time_ms?;
        let accuracy = p
            .h_accuracy_m
            .map(f64::from)
            .filter(|a| a.is_finite() && *a >= 0.0);
        let distance = self
            .anchor
            .map(|(lat, lon, _)| haversine_m(lat, lon, p.lat, p.lon));
        let usable = accuracy.is_none_or(|a| a <= self.params.radius_m);

        if self.resting {
            let margin = self.params.radius_m + accuracy.unwrap_or(0.0);
            if distance.is_some_and(|d| d > margin) {
                self.away += 1;
                if self.away >= 2 {
                    self.reset();
                    if usable {
                        self.anchor = Some((p.lat, p.lon, time_ms));
                    }
                    return Some(RestEvent::Ended);
                }
            } else {
                self.away = 0;
            }
            return None;
        }

        if !usable {
            return None;
        }
        match (self.anchor, distance) {
            (Some((_, _, since)), Some(d)) if d <= self.params.radius_m => {
                if time_ms - since >= self.params.dwell_ms {
                    self.resting = true;
                    self.away = 0;
                    return Some(RestEvent::Started);
                }
            }
            _ => self.anchor = Some((p.lat, p.lon, time_ms)),
        }
        None
    }

    /// Forgets the rest point and ends a rest without reporting it.
    pub fn reset(&mut self) {
        self.anchor = None;
        self.resting = false;
        self.away = 0;
    }

    pub fn resting(&self) -> bool {
        self.resting
    }
}
