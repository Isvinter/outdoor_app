//! Wind from full circles: while circling, the ground velocities lie on a circle whose
//! centre is the wind vector (air velocity turns around, the wind stays).
//!
//! Each GPS fix gives a velocity point (east, north) from ground speed and course. A full
//! turn of 360° gives one circle; an algebraic least-squares fit (Kåsa) finds its centre.
//! Recent circles of the same thermal are averaged, newer ones weigh more.

/// A pause between two samples longer than this aborts the circle in progress.
const MAX_GAP_S: f64 = 3.0;
/// Turn rate (degrees per second) a sample needs to count as circling.
const MIN_TURN_RATE_DEG_S: f64 = 4.0;
/// Consecutive weaker samples a circle in progress tolerates; one more aborts it.
const MAX_WEAK_SAMPLES: u32 = 2;
/// Minimum number of samples in an accepted circle.
const MIN_CIRCLE_SAMPLES: usize = 8;
/// Longest accepted circle in seconds.
const MAX_CIRCLE_S: f64 = 60.0;
/// Smallest accepted fitted radius (= airspeed) in m/s.
const MIN_RADIUS_MPS: f64 = 3.0;
/// Largest accepted RMS residual divided by the radius.
const MAX_RESIDUAL_RATIO: f64 = 0.3;
/// A circle starting longer than this after the previous one ended opens a new thermal.
const NEW_THERMAL_S: f64 = 120.0;
/// Number of circles averaged at most.
const MAX_CIRCLES: usize = 6;
/// Weight factor per older circle (newest 1, then 0.7, 0.49, ...).
const AGE_WEIGHT: f64 = 0.7;
/// A circle whose wind differs more than this (m/s) from the mean replaces the list.
const MAX_SHIFT_MPS: f64 = 2.5;

/// One GPS fix: ground speed and course over ground.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindSample {
    pub time_s: f64,
    pub ground_speed_mps: f64,
    /// Course over ground in degrees (0 = north, 90 = east).
    pub course_deg: f64,
}

/// The estimated wind.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindEstimate {
    /// Direction the wind blows from, in degrees 0..360 (aviation convention).
    pub from_deg: f64,
    pub speed_mps: f64,
    /// Confidence 1..=3.
    pub stars: u8,
}

/// Detects full circles in a stream of samples and averages their wind.
#[derive(Debug, Clone, Default)]
pub struct WindEstimator {
    previous: Option<WindSample>,
    progress: Option<Progress>,
    /// Accepted circles of the current thermal, newest first.
    circles: Vec<Circle>,
}

/// The circle being flown right now.
#[derive(Debug, Clone)]
struct Progress {
    samples: Vec<WindSample>,
    /// Summed course change in degrees; negative for left turns.
    turned_deg: f64,
    /// +1.0 for right turns, -1.0 for left turns.
    sign: f64,
    /// Consecutive samples turning too slowly (or the wrong way).
    weak: u32,
}

/// An accepted full circle.
#[derive(Debug, Clone, Copy)]
struct Circle {
    start_s: f64,
    end_s: f64,
    /// Fitted centre = wind vector (towards), east and north in m/s.
    cx: f64,
    cy: f64,
    /// RMS residual divided by the radius.
    residual_ratio: f64,
}

impl WindEstimator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, sample: WindSample) {
        // `Option::replace` stores the new value and hands back the old one.
        let Some(previous) = self.previous.replace(sample) else {
            return;
        };
        let dt = sample.time_s - previous.time_s;
        if dt <= 0.0 || dt > MAX_GAP_S {
            self.progress = None;
            return;
        }
        let diff = wrap_deg(sample.course_deg - previous.course_deg);
        let rate = diff / dt;
        let strong = rate.abs() >= MIN_TURN_RATE_DEG_S;

        let continues = match self.progress.as_mut() {
            Some(p) if strong && rate.signum() == p.sign => {
                p.weak = 0;
                true
            }
            Some(p) if p.weak < MAX_WEAK_SAMPLES => {
                p.weak += 1;
                true
            }
            _ => false,
        };
        if continues {
            // `continues` is only true when there is a circle in progress.
            let p = self.progress.as_mut().unwrap();
            p.samples.push(sample);
            p.turned_deg += diff;
        } else if strong {
            self.progress = Some(Progress {
                samples: vec![previous, sample],
                turned_deg: diff,
                sign: rate.signum(),
                weak: 0,
            });
        } else {
            self.progress = None;
        }

        let complete = self
            .progress
            .as_ref()
            .is_some_and(|p| p.turned_deg.abs() >= 360.0);
        if complete {
            // `take` moves the value out and leaves `None` behind.
            let done = self.progress.take().unwrap();
            if let Some(circle) = fit_circle(&done.samples) {
                self.accept(circle);
            }
            // The next circle starts at the sample that completed this one.
            self.progress = Some(Progress {
                samples: vec![sample],
                turned_deg: 0.0,
                sign: done.sign,
                weak: 0,
            });
        }
    }

    /// The averaged wind at time `now_s`, or `None` without a recent enough circle.
    pub fn estimate(&self, now_s: f64) -> Option<WindEstimate> {
        let newest = self.circles.first()?;
        let (cx, cy, quality) = self.mean()?;

        let count_stars: i32 = self.circles.len().min(3) as i32;
        let quality_stars = if quality <= 0.1 {
            3
        } else if quality <= 0.2 {
            2
        } else {
            1
        };
        let mut stars = count_stars.min(quality_stars);

        let age_s = now_s - newest.end_s;
        if age_s > 30.0 * 60.0 {
            return None;
        } else if age_s > 20.0 * 60.0 {
            stars -= 2;
        } else if age_s > 10.0 * 60.0 {
            stars -= 1;
        }
        // Old wind while a circle in a new thermal is half done.
        let new_thermal_started = self.progress.as_ref().is_some_and(|p| {
            p.turned_deg.abs() >= 180.0 && p.samples[0].time_s - newest.end_s > NEW_THERMAL_S
        });
        if new_thermal_started {
            stars -= 1;
        }
        if stars < 1 {
            return None;
        }

        Some(WindEstimate {
            from_deg: (-cx).atan2(-cy).to_degrees().rem_euclid(360.0),
            speed_mps: cx.hypot(cy),
            stars: stars as u8,
        })
    }

    fn accept(&mut self, circle: Circle) {
        let new_thermal = self
            .circles
            .first()
            .is_some_and(|newest| circle.start_s - newest.end_s > NEW_THERMAL_S);
        let shifted = self
            .mean()
            .is_some_and(|(cx, cy, _)| (circle.cx - cx).hypot(circle.cy - cy) > MAX_SHIFT_MPS);
        if new_thermal || shifted {
            self.circles.clear();
        }
        self.circles.insert(0, circle);
        self.circles.truncate(MAX_CIRCLES);
    }

    /// Weighted mean of the centres and of the residual ratios (newest weighs most).
    fn mean(&self) -> Option<(f64, f64, f64)> {
        if self.circles.is_empty() {
            return None;
        }
        let (mut sum_w, mut sum_x, mut sum_y, mut sum_q) = (0.0, 0.0, 0.0, 0.0);
        let mut weight = 1.0;
        for c in &self.circles {
            sum_w += weight;
            sum_x += weight * c.cx;
            sum_y += weight * c.cy;
            sum_q += weight * c.residual_ratio;
            weight *= AGE_WEIGHT;
        }
        Some((sum_x / sum_w, sum_y / sum_w, sum_q / sum_w))
    }

    #[cfg(test)]
    fn turned_deg(&self) -> f64 {
        self.progress.as_ref().map_or(0.0, |p| p.turned_deg)
    }

    #[cfg(test)]
    fn last_time_s(&self) -> Option<f64> {
        self.previous.map(|s| s.time_s)
    }
}

/// Wraps an angle difference into (-180°, 180°].
fn wrap_deg(d: f64) -> f64 {
    let w = d.rem_euclid(360.0);
    if w > 180.0 { w - 360.0 } else { w }
}

/// Fits a circle to the velocity points of `samples`; `None` if it fails the acceptance rules.
fn fit_circle(samples: &[WindSample]) -> Option<Circle> {
    let first = samples.first()?;
    let last = samples.last()?;
    if samples.len() < MIN_CIRCLE_SAMPLES || last.time_s - first.time_s > MAX_CIRCLE_S {
        return None;
    }
    let points: Vec<(f64, f64)> = samples
        .iter()
        .map(|s| {
            let c = s.course_deg.to_radians();
            (s.ground_speed_mps * c.sin(), s.ground_speed_mps * c.cos())
        })
        .collect();

    // Kåsa fit: minimise Σ(x² + y² + Dx + Ey + F)². Setting the derivatives by D, E and F
    // to zero gives three linear equations (the normal equations) A·[D, E, F] = b.
    let (mut sxx, mut sxy, mut syy, mut sx, mut sy) = (0.0, 0.0, 0.0, 0.0, 0.0);
    let (mut sxz, mut syz, mut sz) = (0.0, 0.0, 0.0);
    for &(x, y) in &points {
        let z = x * x + y * y;
        sxx += x * x;
        sxy += x * y;
        syy += y * y;
        sx += x;
        sy += y;
        sxz += x * z;
        syz += y * z;
        sz += z;
    }
    let n = points.len() as f64;
    let a = [[sxx, sxy, sx], [sxy, syy, sy], [sx, sy, n]];
    let b = [-sxz, -syz, -sz];
    let [d, e, f] = solve3(a, b)?;

    let (cx, cy) = (-d / 2.0, -e / 2.0);
    let r_squared = cx * cx + cy * cy - f;
    if r_squared <= 0.0 {
        return None;
    }
    let radius = r_squared.sqrt();
    if radius < MIN_RADIUS_MPS {
        return None;
    }
    let sum_sq: f64 = points
        .iter()
        .map(|&(x, y)| ((x - cx).hypot(y - cy) - radius).powi(2))
        .sum();
    let residual_ratio = (sum_sq / n).sqrt() / radius;
    if residual_ratio > MAX_RESIDUAL_RATIO {
        return None;
    }
    Some(Circle {
        start_s: first.time_s,
        end_s: last.time_s,
        cx,
        cy,
        residual_ratio,
    })
}

/// Solves the 3×3 system `a·x = b` with Cramer's rule; `None` if it is (nearly) singular.
fn solve3(a: [[f64; 3]; 3], b: [f64; 3]) -> Option<[f64; 3]> {
    let det = |m: [[f64; 3]; 3]| {
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    };
    let d = det(a);
    if d.abs() < 1e-9 {
        return None;
    }
    let mut x = [0.0; 3];
    for (col, value) in x.iter_mut().enumerate() {
        let mut m = a;
        for row in 0..3 {
            m[row][col] = b[row];
        }
        *value = det(m) / d;
    }
    Some(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Simple linear congruential generator: deterministic noise in -1..1.
    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self) -> f64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (self.0 >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
        }
    }

    /// 1 Hz samples of `n_circles` full circles (a negative period turns left), ground
    /// velocity = air velocity + wind. Each circle gets one spare sample (plus two at the
    /// end), so noise cannot leave a circle a few degrees short of 360°. `noise_seed` adds
    /// small noise (±0.3 m/s, ±2°).
    fn circles(
        start_s: f64,
        n_circles: usize,
        period_s: f64,
        airspeed: f64,
        wind_from: f64,
        wind_speed: f64,
        noise_seed: Option<u64>,
    ) -> Vec<WindSample> {
        let noise = noise_seed.map(|seed| (seed, 0.3));
        noisy_circles(
            start_s, n_circles, period_s, airspeed, wind_from, wind_speed, noise,
        )
    }

    /// Like [`circles`], with the speed noise amplitude chosen: `noise` is (seed, ±m/s).
    fn noisy_circles(
        start_s: f64,
        n_circles: usize,
        period_s: f64,
        airspeed: f64,
        wind_from: f64,
        wind_speed: f64,
        noise: Option<(u64, f64)>,
    ) -> Vec<WindSample> {
        let mut rng = noise.map(|(seed, amplitude)| (Lcg(seed), amplitude));
        // The wind blows towards `wind_from + 180°`.
        let wind_x = -wind_speed * wind_from.to_radians().sin();
        let wind_y = -wind_speed * wind_from.to_radians().cos();
        let count = n_circles * (period_s.abs() as usize + 1) + 2;
        (0..count)
            .map(|i| {
                let t = i as f64;
                let heading = (360.0 * t / period_s).to_radians();
                let x = airspeed * heading.sin() + wind_x;
                let y = airspeed * heading.cos() + wind_y;
                let mut speed = x.hypot(y);
                let mut course = x.atan2(y).to_degrees();
                if let Some((lcg, amplitude)) = rng.as_mut() {
                    speed += *amplitude * lcg.next();
                    course += 2.0 * lcg.next();
                }
                WindSample {
                    time_s: start_s + t,
                    ground_speed_mps: speed,
                    course_deg: course.rem_euclid(360.0),
                }
            })
            .collect()
    }

    /// Straight flight at 1 Hz with the given courses, 10 m/s.
    fn straight(start_s: f64, courses: impl Iterator<Item = f64>) -> Vec<WindSample> {
        courses
            .enumerate()
            .map(|(i, course_deg)| WindSample {
                time_s: start_s + i as f64,
                ground_speed_mps: 10.0,
                course_deg,
            })
            .collect()
    }

    fn fed(samples: &[WindSample]) -> WindEstimator {
        let mut e = WindEstimator::new();
        for s in samples {
            e.push(*s);
        }
        e
    }

    fn last_time(samples: &[WindSample]) -> f64 {
        samples.last().unwrap().time_s
    }

    /// Smallest difference between two directions in degrees.
    fn angle_diff(a: f64, b: f64) -> f64 {
        let d = (a - b).rem_euclid(360.0);
        d.min(360.0 - d)
    }

    #[test]
    fn one_circle_gives_the_wind_with_one_star() {
        let s = circles(0.0, 1, 20.0, 8.0, 270.0, 4.0, None);
        let w = fed(&s).estimate(last_time(&s)).unwrap();
        assert!(angle_diff(w.from_deg, 270.0) <= 3.0, "{w:?}");
        assert!((w.speed_mps - 4.0).abs() <= 0.3, "{w:?}");
        assert_eq!(w.stars, 1);
    }

    #[test]
    fn more_clean_circles_give_more_stars() {
        let s = circles(0.0, 3, 20.0, 8.0, 270.0, 4.0, None);
        let w = fed(&s).estimate(last_time(&s)).unwrap();
        assert_eq!(w.stars, 3, "{w:?}");
        assert!(angle_diff(w.from_deg, 270.0) <= 3.0, "{w:?}");
    }

    #[test]
    fn small_noise_keeps_three_stars() {
        let s = circles(0.0, 3, 20.0, 8.0, 270.0, 4.0, Some(7));
        let w = fed(&s).estimate(last_time(&s)).unwrap();
        assert_eq!(w.stars, 3, "{w:?}");
        assert!(angle_diff(w.from_deg, 270.0) <= 5.0, "{w:?}");
        assert!((w.speed_mps - 4.0).abs() <= 0.5, "{w:?}");
    }

    #[test]
    fn noisy_circles_cap_the_stars() {
        // ±2.5 m/s rather than ±1.5: at 8 m/s airspeed, uniform ±1.5 m/s noise leaves the
        // residual ratio around 0.07..0.11, right at the 0.1 star limit (most seeds keep 3
        // stars). ±2.5 m/s gives 0.11..0.18 for every seed tried.
        let s = noisy_circles(0.0, 3, 20.0, 8.0, 270.0, 4.0, Some((11, 2.5)));
        let w = fed(&s).estimate(last_time(&s)).unwrap();
        assert!(w.stars <= 2, "{w:?}");
        assert!(angle_diff(w.from_deg, 270.0) <= 20.0, "{w:?}");
        assert!((w.speed_mps - 4.0).abs() <= 1.0, "{w:?}");
    }

    #[test]
    fn straight_flight_gives_no_wind() {
        let s = straight(0.0, std::iter::repeat_n(42.0, 120));
        assert_eq!(fed(&s).estimate(last_time(&s)), None);
    }

    #[test]
    fn left_and_right_circles_both_work() {
        let right = circles(0.0, 3, 20.0, 8.0, 270.0, 4.0, None);
        let left = circles(0.0, 3, -20.0, 8.0, 270.0, 4.0, None);
        let r = fed(&right).estimate(last_time(&right)).unwrap();
        let l = fed(&left).estimate(last_time(&left)).unwrap();
        assert_eq!(l.stars, r.stars);
        assert!(angle_diff(l.from_deg, r.from_deg) <= 1.0, "{l:?} {r:?}");
        assert!((l.speed_mps - r.speed_mps).abs() <= 0.1, "{l:?} {r:?}");
    }

    #[test]
    fn course_wrapping_does_not_fake_a_circle() {
        let wobble = (0..120).map(|i| if i % 2 == 0 { 355.0 } else { 5.0 });
        let s = straight(0.0, wobble);
        assert_eq!(fed(&s).estimate(last_time(&s)), None);
    }

    #[test]
    fn a_wind_shift_replaces_the_old_wind() {
        let mut s = circles(0.0, 3, 20.0, 8.0, 180.0, 5.0, None);
        // Same thermal: the new circle starts 30 s after the old ones ended.
        s.extend(circles(last_time(&s) + 30.0, 1, 20.0, 8.0, 0.0, 7.0, None));
        let w = fed(&s).estimate(last_time(&s)).unwrap();
        assert!(angle_diff(w.from_deg, 0.0) <= 3.0, "{w:?}");
        assert!((w.speed_mps - 7.0).abs() <= 0.3, "{w:?}");
        assert_eq!(w.stars, 1);
    }

    #[test]
    fn old_wind_loses_stars_and_disappears() {
        let s = circles(0.0, 3, 20.0, 8.0, 270.0, 4.0, None);
        let e = fed(&s);
        assert_eq!(e.estimate(60.0 + 11.0 * 60.0).unwrap().stars, 2);
        assert_eq!(e.estimate(60.0 + 21.0 * 60.0).unwrap().stars, 1);
        assert_eq!(e.estimate(60.0 + 31.0 * 60.0), None);
    }

    #[test]
    fn old_wind_in_a_new_thermal_loses_a_star() {
        let old = circles(0.0, 3, 20.0, 8.0, 270.0, 4.0, None);
        let new = circles(last_time(&old) + 301.0, 1, 20.0, 8.0, 90.0, 3.0, None);
        let mut e = fed(&old);
        assert_eq!(e.estimate(last_time(&old)).unwrap().stars, 3);
        // Five minutes straight, on the course the new circle begins with.
        let course = new[0].course_deg;
        for s in straight(last_time(&old) + 1.0, std::iter::repeat_n(course, 300)) {
            e.push(s);
        }

        let mut rest = new.iter();
        // Feed the new circle until it has turned at least 200°.
        for s in rest.by_ref() {
            e.push(*s);
            if e.turned_deg().abs() >= 200.0 {
                break;
            }
        }
        assert!(e.turned_deg().abs() < 360.0);
        let now = e.last_time_s().unwrap();
        let w = e.estimate(now).unwrap();
        assert_eq!(w.stars, 2, "{w:?}");
        assert!(angle_diff(w.from_deg, 270.0) <= 3.0, "{w:?}");

        for s in rest {
            e.push(*s);
        }
        let w = e.estimate(last_time(&new)).unwrap();
        assert_eq!(w.stars, 1, "{w:?}");
        assert!(angle_diff(w.from_deg, 90.0) <= 3.0, "{w:?}");
        assert!((w.speed_mps - 3.0).abs() <= 0.3, "{w:?}");
    }
}
