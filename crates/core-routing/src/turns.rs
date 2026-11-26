//! Direction changes along a route line: how sharply it bends at a point and
//! which kind of turn that is.

use crate::model::{LatLon, TurnKind};

/// How far before and after a turn the direction is measured, in metres.
pub const TURN_REACH_M: f64 = 20.0;

/// Change of direction at `line[index]` in degrees, −180…180, positive to the
/// right. The direction before and after is taken between the point and the
/// point about `reach_m` away along the line (or the end of the line, if it is
/// shorter). `None` when there is no line before or after the point.
pub fn angle_at(line: &[LatLon], index: usize, reach_m: f64) -> Option<f64> {
    let here = line.get(index)?;
    let before = reach(here, line[..index].iter().rev(), reach_m)?;
    let after = reach(here, line[index + 1..].iter(), reach_m)?;
    let turn = bearing_deg(here, after) - bearing_deg(before, here);
    // Into −180…180.
    Some((turn + 540.0).rem_euclid(360.0) - 180.0)
}

/// The kind of turn for a change of direction in degrees (positive to the
/// right). `None` below 30°, which counts as straight on.
pub fn kind_for_angle(deg: f64) -> Option<TurnKind> {
    let right = deg > 0.0;
    let side = |left, right_kind| if right { right_kind } else { left };
    match deg.abs() {
        a if a < 30.0 => None,
        a if a < 60.0 => Some(side(TurnKind::SlightLeft, TurnKind::SlightRight)),
        a if a < 120.0 => Some(side(TurnKind::Left, TurnKind::Right)),
        a if a < 160.0 => Some(side(TurnKind::SharpLeft, TurnKind::SharpRight)),
        _ => Some(TurnKind::UTurn),
    }
}

/// The first of `points` (walking away from `from`) at least `reach_m` along
/// the line from `from`, or the last one if the line is shorter. `None` when
/// there are no points or all of them lie on `from`.
fn reach<'a>(
    from: &LatLon,
    points: impl Iterator<Item = &'a LatLon>,
    reach_m: f64,
) -> Option<&'a LatLon> {
    let mut previous = from;
    let mut walked = 0.0;
    let mut found = None;
    for point in points {
        walked += previous.distance_m(point);
        previous = point;
        found = Some(point);
        if walked >= reach_m {
            break;
        }
    }
    found.filter(|_| walked > 0.0)
}

/// Initial great-circle bearing from `a` to `b`, degrees clockwise from north.
pub fn bearing_deg(a: &LatLon, b: &LatLon) -> f64 {
    let (lat1, lat2) = (a.lat.to_radians(), b.lat.to_radians());
    let dlon = (b.lon - a.lon).to_radians();
    let y = dlon.sin() * lat2.cos();
    let x = lat1.cos() * lat2.sin() - lat1.sin() * lat2.cos() * dlon.cos();
    y.atan2(x).to_degrees()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A point `metres` north and `east_m` east of a fixed origin.
    fn at(north_m: f64, east_m: f64) -> LatLon {
        let lat0: f64 = 47.7;
        LatLon {
            lat: lat0 + north_m / 111_195.0,
            lon: 11.75 + east_m / (111_195.0 * lat0.to_radians().cos()),
        }
    }

    /// 50 m north in 10 m steps, then 50 m east in 10 m steps.
    fn north_then_east() -> Vec<LatLon> {
        let mut line: Vec<LatLon> = (0..=5).map(|i| at(f64::from(i) * 10.0, 0.0)).collect();
        line.extend((1..=5).map(|i| at(50.0, f64::from(i) * 10.0)));
        line
    }

    #[test]
    fn right_angle_is_right() {
        let angle = angle_at(&north_then_east(), 5, 20.0).unwrap();
        assert!((angle - 90.0).abs() < 1.0, "{angle}");
        assert_eq!(kind_for_angle(angle), Some(TurnKind::Right));
    }

    #[test]
    fn gentle_bend_is_straight() {
        assert_eq!(kind_for_angle(20.0), None);
        assert_eq!(kind_for_angle(-20.0), None);
    }

    #[test]
    fn angle_thresholds() {
        assert_eq!(kind_for_angle(45.0), Some(TurnKind::SlightRight));
        assert_eq!(kind_for_angle(-100.0), Some(TurnKind::Left));
        assert_eq!(kind_for_angle(140.0), Some(TurnKind::SharpRight));
        assert_eq!(kind_for_angle(170.0), Some(TurnKind::UTurn));
        assert_eq!(kind_for_angle(-170.0), Some(TurnKind::UTurn));
    }

    #[test]
    fn angle_needs_line_on_both_sides() {
        let line = north_then_east();
        assert_eq!(angle_at(&line, 0, 20.0), None);
        assert_eq!(angle_at(&line, line.len() - 1, 20.0), None);
    }
}
