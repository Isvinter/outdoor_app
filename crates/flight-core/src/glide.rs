//! Glide ratio over ground, measured from positions and altitude over a sliding window.
//!
//! Only straight flight counts: circling, climbing and standing still give no value, because
//! a wrong number is worse than none. Positions (not GPS course) are used, so noisy course
//! readings at low speed do not matter.

use std::collections::VecDeque;

use crate::geo::{bearing_deg, distance_m};

/// Mean ground speed below which no glide ratio is reported (about 5 km/h).
pub const MIN_GROUND_SPEED_MPS: f64 = 1.4;
/// Upper limit of the reported ratio; flatter glides are shown as this value.
pub const MAX_RATIO: f64 = 40.0;

/// Seconds of samples the buffer keeps, whatever the window, so that a window change
/// applies at once (the longest window is 60 s).
const KEEP_S: f64 = 62.0;
/// A pause between two samples longer than this clears the buffer.
const MAX_GAP_S: f64 = 3.0;
/// Displacement divided by path length must reach this for the flight to count as straight.
const MIN_STRAIGHTNESS: f64 = 0.8;
/// Mean sink rate below which there is no glide (climbing or zero sinker).
const MIN_SINK_MPS: f64 = 0.1;

/// One GPS fix with altitude.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlideSample {
    pub time_s: f64,
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    pub altitude_m: f64,
}

/// The glide over ground within the window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glide {
    /// Distance over ground per metre of height lost, capped at [`MAX_RATIO`].
    pub ratio: f64,
    /// Course over ground in degrees (first to last position); points backwards when the
    /// wind carries the glider backwards.
    pub course_deg: f64,
    /// Mean ground speed over the window in m/s.
    pub ground_speed_mps: f64,
}

/// Collects samples and evaluates the glide over the last `window_s` seconds.
#[derive(Debug, Clone)]
pub struct GlideEstimator {
    window_s: f64,
    // A `VecDeque` is a queue that is cheap to push at the back and pop at the front.
    samples: VecDeque<GlideSample>,
}

impl GlideEstimator {
    pub fn new(window_s: f64) -> Self {
        Self {
            window_s,
            samples: VecDeque::new(),
        }
    }

    /// Changes the window; the next [`glide`](Self::glide) call already uses it.
    pub fn set_window(&mut self, window_s: f64) {
        self.window_s = window_s;
    }

    pub fn push(&mut self, sample: GlideSample) {
        let gap = self
            .samples
            .back()
            .is_some_and(|last| sample.time_s - last.time_s > MAX_GAP_S);
        if gap {
            self.samples.clear();
        }
        self.samples.push_back(sample);
        while let Some(first) = self.samples.front() {
            if sample.time_s - first.time_s > KEEP_S {
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }

    /// The glide over the window, or `None` when it is not full, not straight, too slow
    /// or not sinking.
    pub fn glide(&self) -> Option<Glide> {
        let newest = self.samples.back()?;
        let mut in_window = self
            .samples
            .iter()
            .filter(|s| newest.time_s - s.time_s <= self.window_s);
        let first = in_window.next()?;
        let span = newest.time_s - first.time_s;
        if span < self.window_s - 1.0 || span <= 0.0 {
            return None;
        }

        let mut path = 0.0;
        let mut previous = first;
        for s in in_window {
            path += distance_m(
                previous.latitude_deg,
                previous.longitude_deg,
                s.latitude_deg,
                s.longitude_deg,
            );
            previous = s;
        }
        let displacement = distance_m(
            first.latitude_deg,
            first.longitude_deg,
            newest.latitude_deg,
            newest.longitude_deg,
        );
        if path <= 0.0 || displacement / path < MIN_STRAIGHTNESS {
            return None;
        }

        let ground_speed_mps = displacement / span;
        if ground_speed_mps < MIN_GROUND_SPEED_MPS {
            return None;
        }
        let height_lost = first.altitude_m - newest.altitude_m;
        if height_lost / span < MIN_SINK_MPS {
            return None;
        }

        Some(Glide {
            ratio: (displacement / height_lost).min(MAX_RATIO),
            course_deg: bearing_deg(
                first.latitude_deg,
                first.longitude_deg,
                newest.latitude_deg,
                newest.longitude_deg,
            ),
            ground_speed_mps,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::destination;

    const LAT: f64 = 47.0;
    const LON: f64 = 11.0;

    /// 1 Hz samples moving `speed` m/s along `course` while losing `sink` m/s.
    fn line(start_s: f64, count: usize, course: f64, speed: f64, sink: f64) -> Vec<GlideSample> {
        (0..count)
            .map(|i| {
                let t = i as f64;
                let (lat, lon) = destination(LAT, LON, course, speed * t);
                GlideSample {
                    time_s: start_s + t,
                    latitude_deg: lat,
                    longitude_deg: lon,
                    altitude_m: 2000.0 - sink * t,
                }
            })
            .collect()
    }

    fn fed(window: f64, samples: &[GlideSample]) -> GlideEstimator {
        let mut e = GlideEstimator::new(window);
        for s in samples {
            e.push(*s);
        }
        e
    }

    #[test]
    fn straight_glide_gives_distance_over_height() {
        let g = fed(20.0, &line(0.0, 30, 0.0, 10.0, 1.25)).glide().unwrap();
        assert!((g.ratio - 8.0).abs() < 0.05, "{}", g.ratio);
        assert!(g.course_deg < 0.5 || g.course_deg > 359.5);
        assert!((g.ground_speed_mps - 10.0).abs() < 0.05);
    }

    #[test]
    fn nothing_until_the_window_is_full() {
        let s = line(0.0, 20, 0.0, 10.0, 1.25);
        assert_eq!(fed(20.0, &s[..10]).glide(), None);
        assert!(fed(20.0, &s).glide().is_some());
    }

    #[test]
    fn circling_gives_nothing() {
        let s: Vec<GlideSample> = (0..30)
            .map(|i| {
                let t = i as f64;
                let angle = (t / 20.0 * 360.0).rem_euclid(360.0);
                let (lat, lon) = destination(LAT, LON, angle, 25.0);
                GlideSample {
                    time_s: t,
                    latitude_deg: lat,
                    longitude_deg: lon,
                    altitude_m: 2000.0 - t,
                }
            })
            .collect();
        assert_eq!(fed(20.0, &s).glide(), None);
    }

    #[test]
    fn climbing_and_zero_sink_give_nothing() {
        assert_eq!(fed(20.0, &line(0.0, 30, 0.0, 10.0, -0.5)).glide(), None);
        assert_eq!(fed(20.0, &line(0.0, 30, 0.0, 10.0, 0.05)).glide(), None);
    }

    #[test]
    fn very_flat_glides_are_capped() {
        let g = fed(20.0, &line(0.0, 30, 0.0, 10.0, 0.2)).glide().unwrap();
        assert_eq!(g.ratio, 40.0);
    }

    #[test]
    fn below_the_speed_gate_gives_nothing() {
        assert_eq!(fed(20.0, &line(0.0, 30, 0.0, 1.0, 0.5)).glide(), None);
        let g = fed(20.0, &line(0.0, 30, 0.0, 2.0, 0.5)).glide().unwrap();
        assert!((g.ratio - 4.0).abs() < 0.05);
    }

    #[test]
    fn drifting_backwards_points_backwards() {
        // Displacement 2 m/s to the south while sinking 1 m/s.
        let g = fed(20.0, &line(0.0, 30, 180.0, 2.0, 1.0)).glide().unwrap();
        assert!((g.ratio - 2.0).abs() < 0.05);
        assert!((g.course_deg - 180.0).abs() < 0.5);
    }

    #[test]
    fn a_gps_gap_starts_over() {
        let mut s = line(0.0, 25, 0.0, 10.0, 1.25);
        s.extend(line(29.0, 5, 0.0, 10.0, 1.25));
        assert_eq!(fed(20.0, &s).glide(), None);
    }

    #[test]
    fn changing_the_window_applies_at_once() {
        // 60 s at ratio 8, then 20 s at ratio 4 (sink doubled).
        let mut s = line(0.0, 61, 0.0, 10.0, 1.25);
        let (lat0, lon0) = destination(LAT, LON, 0.0, 600.0);
        let alt0 = 2000.0 - 1.25 * 60.0;
        for i in 1..=20 {
            let t = i as f64;
            let (lat, lon) = destination(lat0, lon0, 0.0, 10.0 * t);
            s.push(GlideSample {
                time_s: 60.0 + t,
                latitude_deg: lat,
                longitude_deg: lon,
                altitude_m: alt0 - 2.5 * t,
            });
        }
        let mut e = fed(60.0, &s);
        let wide = e.glide().unwrap().ratio;
        assert!(wide > 4.0 && wide < 8.0, "{wide}");
        e.set_window(10.0);
        let narrow = e.glide().unwrap().ratio;
        assert!((narrow - 4.0).abs() < 0.05, "{narrow}");
    }
}
