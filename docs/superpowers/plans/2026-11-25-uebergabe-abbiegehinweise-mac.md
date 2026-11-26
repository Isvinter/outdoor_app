# Übergabe: Abbiegehinweise in Trail auf dem Mac

Stand: 2026-11-25, Branch `feature/turn-hints`. **Noch kein PR und kein Merge.** Beides kommt nach Task 8 und dem Test draußen, nur nach Rückfrage.

Entwurf: `docs/superpowers/specs/2026-11-25-abbiegehinweise-design.md`
Plan: `docs/superpowers/plans/2026-11-25-abbiegehinweise.md`

## Was fertig ist (Teil A, Kern, unter Linux)

Tasks 1–6 des Plans sind fertig. Jeder Task wurde einzeln umgesetzt und geprüft. `cargo test --workspace` (805 Tests), clippy und fmt sind grün.

- **Task 1:** `core-routing` fragt Valhalla mit `directions_type: "maneuvers"`. Daraus entsteht `Route.turns` mit eigenen Arten (`TurnKind`). Eine Abzweigung, die Valhalla als „geradeaus“ meldet, zählt ab 45° doch. Den Winkel misst der Kern selbst aus der Linie (`core_routing::turns`).
- **Task 2:** `Leg.turns`, das Format der gespeicherten Routen ist jetzt 3. Routen im Format 2 laden weiter, nur ohne Abzweigungen.
- **Task 3:** `guide/line.rs`. Die Abzweigungen werden auf die ganze Linie gelegt, auch an Wegpunkten (aus den Richtungen der beiden Teilstücke) und an Etappenenden.
- **Task 4:** `guide/turn_text.rs`, die kurzen deutschen Texte und das Runden der Distanzen.
- **Task 5:** `guide/hints.rs`, der Ansager mit zwei Stufen je Abzweigung, abhängig von Sportart und Tempo. Hinweise gibt es nur auf der Route. Ein Etappenende an einer Abzweigung wird nach dieser eigens angesagt.
- **Task 6:** `core-ffi`. `GuidanceInfo` hat drei neue Felder: `turn: TurnInfo?`, `turnSeq: UInt32` und `hasTurns: Bool`. Werte und Regeln stehen in `docs/ALGORITHMEN.md`, Abschnitt 7b.

**Die Trail-Tests bauen erst wieder**, wenn `GuideControllerTests.swift` und `DeviationAlertTests.swift` die neuen Felder beim Anlegen von `GuidanceInfo(...)` mitgeben, etwa `turn: nil, turnSeq: 0, hasTurns: true`. Das ist der erste Schritt von Task 7.

## Reihenfolge auf dem Mac

1. Stand holen und bauen:
   ```bash
   git fetch origin
   git switch feature/turn-hints
   apps/trail/scripts/build-core.sh
   cd apps/trail && xcodegen generate && cd ../..
   ```
2. Task 7 des Plans: das Hinweisfeld mit leichter Vibration und dem Satz „Diese Route hat keine Abbiegehinweise“.
3. Task 8 des Plans: Mitteilung, Sprachansage und die zwei Schalter. Dazu `audio` in `UIBackgroundModes`. Danach anhalten und auf dem iPhone von Hand prüfen.
4. Task 9 des Plans: der Test draußen (der Nutzer). Danach die Gesamtprüfung des Branches, dann PR und Merge `--no-ff`, alles nach Rückfrage.

Umsetzung wie in Teil A: je Task ein Umsetzer, danach ein Prüfer (vom Nutzer gewählt).

## Was Trail vom Kern bekommt

- `turnSeq` wächst genau einmal je neuem Hinweis. **Nur dann** vibriert, meldet und spricht Trail. Eine neue Führung (neue Route, Umplanen) beginnt wieder bei 0. Trail setzt seinen Zähler dann zurück wie bei `deviation`.
- `turn` ist nil, sobald nichts angesagt ist: an der Abzweigung vorbei, abseits der Route oder ohne Abzweigung in Reichweite. Dann verschwindet das Feld, und `cleared()` entfernt die Mitteilung.
- `TurnInfo.spoken` ist der fertige Satz vom Moment des Hinweises, für Mitteilung und Sprache. `distanceText` und `distanceM` werden mit jedem Punkt neu berechnet, für das Feld.
- `TurnInfo.kind` ist der Serde-Name: `slight_left`, `slight_right`, `left`, `right`, `sharp_left`, `sharp_right`, `u_turn`, `keep_left`, `keep_right`, `roundabout`, `stage_end`. `stage` ist `ahead` oder `now`.
- `hasTurns == false` heißt, dass die Route aus der Zeit vor den Abzweigungen stammt. Ein Weg zurück macht das nie falsch.
- „Ziel erreicht“ ist kein Hinweis aus dem Kern. Trail spricht es bei der Phase `arrived`, wenn die Sprache an ist.

## Kleinigkeiten für die Gesamtprüfung (zurückgestellt)

- **Task 1:**
  - `angle_at` nimmt den ersten Punkt ab 20 m und interpoliert nicht. Auf Linien mit wenigen Punkten reicht die Messung deshalb etwas weiter.
  - Die Zuordnungen 12/13, 18–24 und 37/38 sind nicht einzeln getestet.
- **Task 2:** Ein Test in `plan.rs` räumt sein Temp-Verzeichnis nicht auf.
- **Task 3:**
  - Eine Valhalla-Abzweigung am Index 0 eines Teilstücks bekommt `leg = k` statt `k-1`.
  - Die 5-m-Prüfung vor einer Naht ist nicht getestet.
- **Task 4:**
  - Der Schritt „Test zuerst rot“ wurde übersprungen.
  - Es gibt keinen Test bei 974 m, 975 m und 95 m.
- **Task 5:**
  - Bei drei Abzweigungen, die je unter 10 m auseinanderliegen, bekommt die dritte ein spätes „Jetzt“.
  - Die Rückkehr auf die Route nach einem still gesetzten „Jetzt“ ist nicht getestet.
- **Task 6:**
  - `turn_kind_name` würde bei einem Serde-Fehler stumm einen leeren String liefern. Das ist mit der heutigen Aufzählung unmöglich.
