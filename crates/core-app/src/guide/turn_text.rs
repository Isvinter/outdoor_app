//! The short German texts of turn hints. The same words go to the screen,
//! the notifications and the speech.

use core_routing::TurnKind;

/// How near a turn is when its hint is told; `Ahead` comes before `Now`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HintStage {
    /// The announcement ahead of the turn: "In 200 m rechts".
    Ahead,
    /// The turn is here: "Jetzt rechts".
    Now,
}

/// A distance as told to the user: below 100 m rounded to 10 m (never less
/// than "10 m"), then to 50 m, and from 975 m on in km with one decimal so
/// that "1000 m" never appears.
pub fn distance_text(m: f64) -> String {
    let m = m.max(0.0);
    if m >= 975.0 {
        return format!("{:.1} km", (m / 100.0).round() / 10.0).replace('.', ",");
    }
    let rounded = if m < 100.0 {
        ((m / 10.0).round() * 10.0).max(10.0)
    } else {
        (m / 50.0).round() * 50.0
    };
    format!("{rounded:.0} m")
}

/// The name of a maneuver; a roundabout without a known exit is just
/// "Kreisverkehr".
pub fn label(kind: TurnKind, exit: Option<u8>) -> String {
    match kind {
        TurnKind::SlightLeft => "leicht links".into(),
        TurnKind::SlightRight => "leicht rechts".into(),
        TurnKind::Left => "links".into(),
        TurnKind::Right => "rechts".into(),
        TurnKind::SharpLeft => "scharf links".into(),
        TurnKind::SharpRight => "scharf rechts".into(),
        TurnKind::UTurn => "wenden".into(),
        TurnKind::KeepLeft => "links halten".into(),
        TurnKind::KeepRight => "rechts halten".into(),
        TurnKind::Roundabout => match exit {
            Some(n) => format!("Kreisverkehr, {n}. Ausfahrt"),
            None => "Kreisverkehr".into(),
        },
        TurnKind::StageEnd => "Etappenziel erreicht".into(),
    }
}

/// The full sentence of a hint. A stage end is always "Etappenziel erreicht".
/// With `then`, a second turn right behind the first is added.
pub fn spoken(
    stage: HintStage,
    distance_m: f64,
    first: (TurnKind, Option<u8>),
    then: Option<(TurnKind, Option<u8>)>,
) -> String {
    let (kind, exit) = first;
    if kind == TurnKind::StageEnd {
        return label(kind, exit);
    }
    let mut text = match stage {
        HintStage::Ahead => format!("In {} {}", distance_text(distance_m), label(kind, exit)),
        HintStage::Now => format!("Jetzt {}", label(kind, exit)),
    };
    // A stage end after the turn needs no "then" in the sentence.
    if let Some((kind, exit)) = then.filter(|(k, _)| *k != TurnKind::StageEnd) {
        text.push_str(", dann gleich ");
        text.push_str(&label(kind, exit));
    }
    text
}

/// A compass bearing as told to the user, rounded to 10°, north as 0°.
fn bearing_text(deg: f64) -> u32 {
    ((deg.rem_euclid(360.0) / 10.0).round() as u32 * 10) % 360
}

/// Where the route lies from a user off the network of ways, for the screen:
/// "Die Route liegt 120 m bei 150°".
pub fn off_network_text(distance_m: f64, bearing_deg: f64) -> String {
    format!(
        "Die Route liegt {} bei {}°",
        distance_text(distance_m),
        bearing_text(bearing_deg)
    )
}

/// The same, as said when the user leaves the network of ways:
/// "Abseits der Route. Die Route liegt 120 m bei 150 Grad."
pub fn off_network_spoken(distance_m: f64, bearing_deg: f64) -> String {
    format!(
        "Abseits der Route. Die Route liegt {} bei {} Grad.",
        distance_text(distance_m),
        bearing_text(bearing_deg)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use TurnKind::*;

    #[test]
    fn distances_are_rounded() {
        for (m, text) in [
            (4.0, "10 m"),
            (73.0, "70 m"),
            (99.0, "100 m"),
            (230.0, "250 m"),
            (960.0, "950 m"),
            (980.0, "1,0 km"),
            (1240.0, "1,2 km"),
        ] {
            assert_eq!(distance_text(m), text, "{m}");
        }
    }

    #[test]
    fn every_kind_has_a_label() {
        for (kind, text) in [
            (SlightLeft, "leicht links"),
            (SlightRight, "leicht rechts"),
            (Left, "links"),
            (Right, "rechts"),
            (SharpLeft, "scharf links"),
            (SharpRight, "scharf rechts"),
            (UTurn, "wenden"),
            (KeepLeft, "links halten"),
            (KeepRight, "rechts halten"),
            (StageEnd, "Etappenziel erreicht"),
        ] {
            assert_eq!(label(kind, None), text);
        }
        assert_eq!(label(Roundabout, Some(2)), "Kreisverkehr, 2. Ausfahrt");
        assert_eq!(label(Roundabout, None), "Kreisverkehr");
    }

    #[test]
    fn off_the_network() {
        assert_eq!(
            off_network_text(118.0, 147.0),
            "Die Route liegt 100 m bei 150°"
        );
        assert_eq!(
            off_network_spoken(118.0, 147.0),
            "Abseits der Route. Die Route liegt 100 m bei 150 Grad."
        );
        assert_eq!(
            off_network_text(1240.0, 356.0),
            "Die Route liegt 1,2 km bei 0°"
        );
        assert_eq!(off_network_text(40.0, 4.0), "Die Route liegt 40 m bei 0°");
    }

    #[test]
    fn sentences() {
        let right = (Right, None);
        assert_eq!(
            spoken(HintStage::Ahead, 200.0, right, None),
            "In 200 m rechts"
        );
        assert_eq!(spoken(HintStage::Now, 0.0, right, None), "Jetzt rechts");
        assert_eq!(
            spoken(HintStage::Now, 0.0, right, Some((Left, None))),
            "Jetzt rechts, dann gleich links"
        );
        assert_eq!(
            spoken(HintStage::Ahead, 80.0, right, Some((Left, None))),
            "In 80 m rechts, dann gleich links"
        );
        for stage in [HintStage::Ahead, HintStage::Now] {
            assert_eq!(
                spoken(stage, 100.0, (StageEnd, None), None),
                "Etappenziel erreicht"
            );
        }
    }
}
