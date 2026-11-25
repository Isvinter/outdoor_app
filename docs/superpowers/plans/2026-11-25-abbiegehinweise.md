# Trail: Abbiegehinweise und Sprachansagen Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Während der Führung kündigt Trail jede Abzweigung zweimal kurz an („In 200 m rechts“, „Jetzt rechts“): als Feld in der App und wahlweise als Mitteilung im Hintergrund und als Sprachansage.

**Architecture:** Valhalla liefert beim Planen die Manöver, `core-routing` übersetzt sie in eine kleine eigene Liste von Abzweigungen, `core-app` speichert sie je Teilstück. Der Guide legt sie zusammen mit den Abzweigungen an Wegpunkten auf die Linie der Route. Ein neuer, reiner Ansager (`guide/hints.rs`) entscheidet je Fix, welcher Hinweis gilt, auch die Texte. `core-ffi` reicht den Hinweis in `GuidanceInfo` weiter. Trail zeigt ihn nur an, schickt die Mitteilung und spricht.

**Tech Stack:** Rust, UniFFI, Valhalla (valhalla-mobile auf iOS, Crate `valhalla` 0.7.6 unter Linux), SwiftUI, UserNotifications, AVFoundation (`AVSpeechSynthesizer`).

**Spec:** `docs/superpowers/specs/2026-11-25-abbiegehinweise-design.md`

## Global Constraints

- Branch `feature/turn-hints` (besteht, der Entwurf ist committet). Am Ende PR, Merge lokal mit `--no-ff`. Push nur nach Rückfrage.
- **Anhalten und dem Nutzer zeigen:** nach Task 6 (Kern fertig, Linux) und nach Task 8 (Trail auf dem Mac).
- Nur kurze Hinweise, keine Straßennamen. Die Hinweise verändern die Route nie.
- Keine Netzwerkaufrufe. Sprache nur mit der Stimme auf dem Gerät.
- Mitteilungen und Sprache sind je ein Schalter, **standardmäßig aus**.
- Texte wörtlich (Deutsch):
  - Arten: „leicht links“, „leicht rechts“, „links“, „rechts“, „scharf links“, „scharf rechts“, „wenden“, „links halten“, „rechts halten“, „Kreisverkehr, {n}. Ausfahrt“, „Etappenziel erreicht“
  - Vorankündigung: „In {Distanz} {Art}“, etwa „In 200 m rechts“
  - Unmittelbar: „Jetzt {Art}“; „Etappenziel erreicht“ ohne „Jetzt“
  - Zwei dicht beieinander: „{erster Text}, dann gleich {Art}“, etwa „Jetzt rechts, dann gleich links“
  - Distanz: unter 100 m auf 10 m gerundet („70 m“), bis 1 km auf 50 m („250 m“), ab 1 km auf 0,1 km mit Komma („1,2 km“)
  - Ohne Abzweigungen: „Diese Route hat keine Abbiegehinweise“
  - Gesprochen bei Sprache an, außerdem: „Abseits der Route“, „Ziel erreicht“
  - Schalter: „Abbiegehinweise als Mitteilung“, „Abbiegehinweise ansagen“
- Startwerte (in `docs/ALGORITHMEN.md` begründen, mit echten Tracks nachjustieren erlaubt):

| Klasse | Profile | Vorankündigung | Unmittelbar | Tempo ohne GPS |
|---|---|---|---|---|
| Zu Fuß | `Hike` | 50 m | 12 m | — |
| Rad | `RoadBike`, `Trekking`, `Mtb` | 25 s, mindestens 100 m | 6 s, mindestens 30 m | 4,5 m/s |
| Auto | `Car` | 35 s, mindestens 400 m; ab 22 m/s (80 km/h) 1500 m | 9 s, mindestens 100 m | 14 m/s |

  Vorbei an einer Abzweigung: Fortschritt 10 m hinter ihr. „Dicht beieinander“: Die nächste Abzweigung liegt höchstens so weit hinter der ersten, wie der Abstand für „unmittelbar“ der Klasse beträgt. Winkel: unter 30° geradeaus, 30–60° leicht, 60–120° normal, 120–160° scharf, ab 160° wenden. „Geradeaus“ von Valhalla gilt ab 45° als Abzweigung. Der Winkel wird je 20 m vor und hinter dem Punkt auf der Linie gemessen.
- Swift lässt sich unter Linux nicht bauen. Tasks 7–8 laufen auf dem Mac, vorher `apps/trail/scripts/build-core.sh` und `xcodegen generate`.
- Neue Ansichten funktionieren auch im OLED-Dunkelmodus. Echte Karten in `data/` nicht anfassen. Jeder Commit endet mit `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. **Halt kurz vor der Abzweigung** (Ampel, Rast): Das Tempo fällt auf 0, die Hinweise wiederholen sich nicht, und „Jetzt“ kommt trotzdem, sobald die Untergrenze in Metern erreicht ist. Test `stop_before_turn_tells_each_once` in Task 5.
2. **GPS wackelt um die Abzweigung** (Fortschritt springt ein paar Meter vor und zurück): kein zweites „Jetzt“, das Feld verschwindet nicht und kommt wieder. Test `jitter_at_turn_tells_once` in Task 5.
3. **Neuplanung nach einer Abweichung mit der ersten Abzweigung in 20 m:** Es kommt sofort „Jetzt …“, keine verspätete Vorankündigung. Test `first_turn_inside_now_distance_is_now` in Task 5.
4. **Wegpunkt mitten auf einer geraden Straße:** An der Naht zweier Teilstücke kommt kein Hinweis. Test `straight_seam_has_no_turn` in Task 3.
5. **Gespeicherte Route aus der Zeit vor diesem Plan** (Format 2): Sie lädt, die Führung läuft, die Leiste sagt einmal „Diese Route hat keine Abbiegehinweise“. Tests `format_2_plan_loads_without_turns` in Task 2 und `old_plan_reports_no_turns` in Task 5.

---

## Teil A: Kern (Linux)

### Task 1: Abzweigungen aus Valhallas Manövern

**Files:**
- Modify: `crates/core-routing/src/model.rs` (neue Typen, `Route.turns`)
- Modify: `crates/core-routing/src/valhalla_api.rs` (`directions_type`, Antwort lesen)
- Modify: `crates/core-routing/src/manual.rs` (`turns: None`)
- Create: `crates/core-routing/src/turns.rs` (Winkel und Einordnung; auch von Task 3 genutzt)
- Modify: `crates/core-routing/src/lib.rs` (Exporte)
- Test: Modultests in `valhalla_api.rs` und `turns.rs`, `crates/core-routing/tests/valhalla.rs`

**Interfaces:**
- Produces:
  - `pub enum TurnKind { SlightLeft, SlightRight, Left, Right, SharpLeft, SharpRight, UTurn, KeepLeft, KeepRight, Roundabout, StageEnd }` (Serde `snake_case`, `Copy, Eq`)
  - `pub struct Turn { pub at: usize, pub kind: TurnKind, #[serde(default, skip_serializing_if = "Option::is_none")] pub exit: Option<u8> }`. `at` ist der Index in der Linie, zu der die Abzweigung gehört.
  - `Route.turns: Option<Vec<Turn>>`. `None` bedeutet „nicht berechnet“ (manuell), `Some(vec![])` bedeutet „keine Abzweigung“.
  - `turns::angle_at(line: &[LatLon], index: usize, reach_m: f64) -> Option<f64>`: Richtungsänderung in Grad, −180…180, positiv nach rechts. `None`, wenn es vor oder hinter dem Punkt keine Linie gibt.
  - `turns::kind_for_angle(deg: f64) -> Option<TurnKind>`: `None` unter 30°, sonst die Art nach den Schwellen der Global Constraints.

- [ ] **Step 1: Failing tests in `turns.rs`**
  - `right_angle_is_right`: Eine Linie 50 m nach Norden, dann 50 m nach Osten. `angle_at` am Knick ergibt etwa +90, `kind_for_angle` ergibt `Right`.
  - `gentle_bend_is_straight`: 20° ergibt `None`.
  - `angle_thresholds`: 45 → `SlightRight`, −100 → `Left`, 140 → `SharpRight`, 170 → `UTurn`, −170 → `UTurn`.
  - `angle_needs_line_on_both_sides`: Index 0 und der letzte Index ergeben `None`.
- [ ] **Step 2: Failing tests in `valhalla_api.rs`**
  - `costing_json_asks_for_maneuvers`: `"directions_type": "maneuvers"`.
  - `maneuvers_become_turns`: eine Valhalla-Antwort als JSON-String mit einem Teilstück und den Manövern Start (Typ 1), Rechts (10), Kreisverkehr (26, `roundabout_exit_count: 2`), Geradeaus (8), Ziel (4). Ergebnis: `[Turn{at: <Index>, kind: Right}, Turn{at: <Index>, kind: Roundabout, exit: Some(2)}]`. Der Index stimmt mit `begin_shape_index` überein.
  - `continue_with_sharp_angle_is_a_turn`: ein Manöver 8, dessen Linie dort um 90° nach links knickt, ergibt `Left`. Bei einem Knick von 20° ergibt es nichts.
  - `turn_indices_follow_deduplicated_line`: eine Antwort mit zwei Valhalla-Legs (wie Durchgangspunkte), deren gemeinsamer Punkt doppelt ist. Ein Manöver im zweiten Leg zeigt danach auf denselben Punkt in `Route.line`.
  - `answer_without_maneuvers_has_empty_turns`: Fehlt `maneuvers`, ergibt das `Some(vec![])`.
- [ ] **Step 3: Run, expect FAIL.** Run: `cargo test -p core-routing` (Kompilierfehler oder FAIL)
- [ ] **Step 4: Implement.**
  - Valhallas `Maneuver` lesen (`type`, `begin_shape_index`, `roundabout_exit_count`). Die Indizes beim Zusammenfügen der Legs mit der Zahl der übersprungenen doppelten Punkte umrechnen.
  - Valhallas Typen auf `TurnKind`:
    - 9/16 leicht
    - 10/15 normal
    - 11/14 scharf
    - 12/13 wenden
    - 18, 20, 23, 37 rechts halten
    - 19, 21, 24, 38 links halten
    - 26 Kreisverkehr
    - 7, 8, 17, 22, 25 nur über den Winkel ab 45° (dann `kind_for_angle`)
    - alles andere nichts
  - Die Nummern vor dem Festlegen gegen `DirectionsLeg_Maneuver_Type` in den Valhalla-Quellen der genutzten Version prüfen.
  - Den Winkel immer selbst aus der Linie messen (`angle_at`, Reichweite 20 m). Valhallas native JSON-Antwort liefert `turn_degree` nicht verlässlich. Das weicht vom Anhaltspunkt im Entwurf ab, misst aber dasselbe.
- [ ] **Step 5: Test auf echten Kacheln** in `tests/valhalla.rs`: `route_has_turns`, Bahnhof Tegernsee → Neureuth mit `Profile::Hike`. Erwartet: `turns` ist `Some`, mindestens eine Abzweigung, jedes `at` kleiner als `line.len()`.
- [ ] **Step 6: Run, expect PASS.** Run: `cargo test -p core-routing && cargo test -p core-routing --features valhalla --test valhalla`
- [ ] **Step 7: Commit** „Read turns from Valhalla's maneuvers“

### Task 2: Abzweigungen im gespeicherten Teilstück

**Files:**
- Modify: `crates/core-app/src/plan.rs` (`Leg.turns`, `PLAN_FORMAT`, `measure`, Fehler-Legs)
- Modify: `crates/core-app/src/guide/testing.rs` (`leg()` und `failed_leg()` mit `turns`)
- Modify: alle weiteren Stellen mit `Leg { … }` (der Compiler zeigt sie)
- Test: Modultests in `plan.rs`

**Interfaces:**
- Consumes: `Route.turns`, `Turn` (Task 1)
- Produces: `Leg.turns: Option<Vec<Turn>>` mit `#[serde(default)]`. Die Indizes beziehen sich auf `Leg.line`. `PLAN_FORMAT = 3`.

- [ ] **Step 1: Failing tests**
  - `format_2_plan_loads_without_turns`: Ein `RoutePlan`-JSON mit `"format": 2` und einem Teilstück ohne `turns` lässt sich lesen (über denselben Weg wie gespeicherte Routen, `routes.rs` „Reads a current plan“). Danach gilt `legs[0].turns == None`.
  - `measured_leg_keeps_turns`: `measure` mit einer `Route` mit `turns: Some(vec![Turn{at: 3, kind: Left, exit: None}])` übernimmt sie unverändert.
  - `new_plans_have_format_3`.
- [ ] **Step 2: Run, expect FAIL.** Run: `cargo test -p core-app plan::`
- [ ] **Step 3: Implement.** Gescheiterte Teilstücke bekommen `turns: None`.
- [ ] **Step 4: Run, expect PASS.** Run: `cargo test -p core-app && cargo clippy -p core-app --all-targets -- -D warnings`
- [ ] **Step 5: Commit** „Keep the turns of each leg in the plan“

### Task 3: Abzweigungen entlang der ganzen Route

**Files:**
- Modify: `crates/core-app/src/guide/line.rs` (`RouteLine.turns`, `RouteLine.has_turns`)
- Test: Modultests in `line.rs`

**Interfaces:**
- Consumes: `Leg.turns` (Task 2), `turns::angle_at`, `turns::kind_for_angle` (Task 1)
- Produces:
  - `pub struct LineTurn { pub at_m: f64, pub kind: TurnKind, pub exit: Option<u8>, pub leg: usize }`
  - `RouteLine.turns: Vec<LineTurn>`, aufsteigend nach `at_m`
  - `RouteLine.has_turns: bool`. `false`, wenn ein berechnetes Teilstück (`LegKind::Routed`) `turns == None` hat.

- [ ] **Step 1: Failing tests** (Linien mit `north`/`east` aus `testing.rs`)
  - `leg_turns_are_placed_along_the_line`: zwei berechnete Teilstücke mit je einer Abzweigung. `at_m` ist die Länge bis zum jeweiligen Punkt, beim zweiten Teilstück mit der Länge des ersten.
  - `turn_at_seam_from_leg_directions`: Teilstück 1 läuft nach Norden, Teilstück 2 nach Osten, keines hat dort eine Abzweigung. Erwartet: `Right` bei `waypoint_m[1]`.
  - `straight_seam_has_no_turn`: beide Teilstücke laufen nach Norden, also kein Eintrag.
  - `seam_reversal_is_u_turn`: Teilstück 2 läuft zurück nach Süden, also `UTurn`.
  - `seam_next_to_manual_or_failed_has_no_turn`.
  - `stage_end_is_a_turn`: mit `stage_ends = [1]` ergibt sich `StageEnd` bei `waypoint_m[1]`. Liegt dort auch eine Richtungsänderung, gibt es beide Einträge, zuerst die Abzweigung.
  - `old_legs_mean_no_turns`: ein berechnetes Teilstück mit `turns: None` ergibt `has_turns == false`. Nur manuelle Teilstücke ergeben `has_turns == true` mit leerer Liste.
- [ ] **Step 2: Run, expect FAIL.** Run: `cargo test -p core-app guide::line`
- [ ] **Step 3: Implement** in `RouteLine::from_plan`. An der Naht wird der Winkel über die zusammengefügte Linie gemessen (`angle_at` auf `points` am Index des Wegpunkts), nur wenn beide angrenzenden Teilstücke `Routed` sind. Stammt eine Abzweigung von Valhalla und liegt innerhalb von 5 m um die Naht, gibt es keine zweite aus dem Winkel. Das Ziel bekommt keinen Eintrag, denn „Ziel erreicht“ kommt aus `GuidePhase::Arrived`.
- [ ] **Step 4: Run, expect PASS.** Run: `cargo test -p core-app guide::line`
- [ ] **Step 5: Commit** „Place the turns along the whole route, also at waypoints“

### Task 4: Texte der Hinweise

**Files:**
- Create: `crates/core-app/src/guide/turn_text.rs`
- Modify: `crates/core-app/src/guide/mod.rs`
- Test: Modultests in `turn_text.rs`

**Interfaces:**
- Consumes: `TurnKind` (Task 1)
- Produces:
  - `pub fn distance_text(m: f64) -> String`
  - `pub fn label(kind: TurnKind, exit: Option<u8>) -> String` (etwa „rechts“, „Kreisverkehr, 2. Ausfahrt“)
  - `pub fn spoken(stage: HintStage, distance_m: f64, first: (TurnKind, Option<u8>), then: Option<(TurnKind, Option<u8>)>) -> String`
  - `pub enum HintStage { Ahead, Now }` (hier definiert, `Copy, Eq`)

- [ ] **Step 1: Failing tests**, alle Werte aus den Global Constraints:
  - `distance_text`: 4 → „10 m“ (unter 10 m nie weniger), 73 → „70 m“, 99 → „100 m“, 230 → „250 m“, 980 → „1,0 km“, 1240 → „1,2 km“.
  - `label`: jede Art einmal. Kreisverkehr ohne Ausfahrt ergibt „Kreisverkehr“.
  - `spoken`: `Ahead`, 200 m, Rechts ergibt „In 200 m rechts“. `Now` ergibt „Jetzt rechts“. `Now` mit `then = Left` ergibt „Jetzt rechts, dann gleich links“. `StageEnd` ergibt in jeder Stufe „Etappenziel erreicht“.
- [ ] **Step 2: Run, expect FAIL.** Run: `cargo test -p core-app guide::turn_text`
- [ ] **Step 3: Implement.** Nach dem Runden auf 50 m werden Werte ab 975 m als km geschrieben, damit „1000 m“ nie erscheint.
- [ ] **Step 4: Run, expect PASS.**
- [ ] **Step 5: Commit** „Write the short texts of turn hints“

### Task 5: Der Ansager im Guide

**Files:**
- Create: `crates/core-app/src/guide/hints.rs`
- Modify: `crates/core-app/src/guide/guide.rs` (`Guide` hält den Ansager, `GuideStatus` bekommt den Hinweis)
- Modify: `crates/core-app/src/guide/mod.rs` (Exporte)
- Test: Modultests in `hints.rs` und `guide.rs`

**Interfaces:**
- Consumes: `RouteLine.turns`, `RouteLine.has_turns`, `LegSpan.spec.profile` (Task 3), Texte (Task 4), `Fix.speed_mps`
- Produces:
  - `pub struct HintParams { … }` mit den Werten der Global Constraints, `HintParams::for_profile(Profile) -> Option<HintParams>`. `Manual` ergibt `None`.
  - `pub struct TurnHint { pub kind: TurnKind, pub exit: Option<u8>, pub then: Option<(TurnKind, Option<u8>)>, pub stage: HintStage, pub distance_m: f64, pub distance_text: String, pub label: String, pub spoken: String }`. `distance_m` und `distance_text` werden bei jedem Fix neu berechnet, `spoken` bleibt der Text vom Moment des Hinweises.
  - `GuideStatus.turn: Option<TurnHint>`
  - `GuideStatus.turn_seq: u32`: zählt mit jedem neuen Hinweis hoch, jede Stufe einmal. Trail vibriert, meldet und spricht, wenn der Wert wächst.
  - `GuideStatus.has_turns: bool` (aus `RouteLine.has_turns`)

- [ ] **Step 1: Failing tests in `hints.rs`**. Der Ansager ist ein reiner Zustand mit `update(along_m, speed_mps, time_ms, profile)` und lässt sich ohne Guide prüfen.
  - `hike_tells_at_fixed_distances`: Zu Fuß mit 1,2 m/s auf eine Abzweigung bei 300 m zu. „Ahead“ kommt bei 250 m Fortschritt (50 m davor), „Now“ bei 288 m. Danach verschwindet der Hinweis ab 310 m. `turn_seq` steigt genau zweimal.
  - `bike_scales_with_speed`: Rad mit 8 m/s, also „Ahead“ bei 200 m davor. Mit 3 m/s greift die Untergrenze von 100 m.
  - `car_fast_announces_far`: Auto mit 25 m/s, also „Ahead“ bei 1500 m davor.
  - `stop_before_turn_tells_each_once`: Rad, „Ahead“ ist gekommen, dann 60 s lang Tempo 0 bei 40 m vor der Abzweigung. Es kommt nichts Neues. Weiter mit 3 m/s, „Now“ kommt bei 30 m.
  - `jitter_at_turn_tells_once`: Der Fortschritt springt um die Abzweigung herum (−5, +3, −4, +8, +12 m). Ergebnis: ein „Now“, das Feld bleibt bis +10 m, danach ist es weg und kommt nicht wieder.
  - `first_turn_inside_now_distance_is_now`: Der Start liegt 20 m vor der ersten Abzweigung (Rad), also sofort „Now“ ohne „Ahead“.
  - `close_turns_are_told_together`: Rechts bei 300 m, links bei 320 m (Rad). Der Hinweis lautet „Jetzt rechts, dann gleich links“. Für die zweite Abzweigung kommt danach kein eigenes „Ahead“, aber ihr eigenes „Now“, wenn sie bis dahin nicht schon vorbei ist.
  - `speed_without_gps_uses_profile_default`: `speed_mps = None` ergibt 4,5 m/s beim Rad.
- [ ] **Step 2: Failing tests in `guide.rs`**
  - `hint_hidden_off_route`: Mit einer Abweichung (Phase `OffRoute`) ist `turn == None`, `turn_seq` bleibt.
  - `old_plan_reports_no_turns`: Ein Plan mit `turns: None` ergibt `has_turns == false`, `turn == None`, und die Führung läuft wie bisher.
  - `replayed_track_tells_turns_once`: der echte Track `SYLVENSTEIN` als Plan (`plan_along`) mit von Hand gesetzten Abzweigungen an drei Knicken, Punkt für Punkt abgespielt. Jede Abzweigung ergibt höchstens zwei Hinweise, und kein „Now“ kommt vor seinem „Ahead“, außer beim ersten.
- [ ] **Step 3: Run, expect FAIL.** Run: `cargo test -p core-app guide::`
- [ ] **Step 4: Implement.**
  - Tempo: gleitender Mittelwert über die brauchbaren Fixes der letzten 5 s, aus `speed_mps`, sonst aus Strecke durch Zeit.
  - Der Ansager merkt sich je Abzweigung (Index in `RouteLine.turns`) die erreichte Stufe und geht nie zurück.
  - Die Profilklasse kommt vom Teilstück am Fortschritt.
  - Der Guide ruft den Ansager nur in `OnRoute` auf. In anderen Phasen ist `turn = None`, die Zustände bleiben erhalten.
  - Ein neuer Guide (neue Route, Neuplanung) beginnt mit einem neuen Ansager und `turn_seq = 0`. Trail erkennt das wie bei `deviation` am kleineren Wert.
- [ ] **Step 5: Run, expect PASS.** Run: `cargo test -p core-app && cargo clippy -p core-app --all-targets -- -D warnings`
- [ ] **Step 6: Commit** „Tell each turn twice, timed by profile and speed“

### Task 6: Hinweise über FFI und Dokumentation

**Files:**
- Modify: `crates/core-ffi/src/guide.rs` (`GuidanceInfo`, `guidance_info`)
- Modify: `docs/ALGORITHMEN.md` (neuer Abschnitt „7b. Abbiegehinweise“ nach Abschnitt 7)
- Test: `crates/core-ffi/tests/ffi.rs`

**Interfaces:**
- Consumes: `GuideStatus.turn`, `turn_seq`, `has_turns` (Task 5)
- Produces für Swift:
  - `#[derive(uniffi::Record)] pub struct TurnInfo { pub kind: String, pub stage: String, pub distance_m: f64, pub distance_text: String, pub label: String, pub spoken: String }`
  - `kind` ist der Serde-Name von `TurnKind`, etwa `"right"`, `"roundabout"`, `"stage_end"`.
  - `stage` ist `"ahead"` oder `"now"`.
  - `GuidanceInfo.turn: Option<TurnInfo>`, `GuidanceInfo.turn_seq: u32`, `GuidanceInfo.has_turns: bool`

- [ ] **Step 1: Failing test** `guidance_info_carries_turn`: Ein `GuideStatus` mit einem `TurnHint` ergibt `TurnInfo` mit denselben Texten und `kind == "right"`, `stage == "now"`.
- [ ] **Step 2: Run, expect FAIL.** Run: `cargo test -p core-ffi`
- [ ] **Step 3: Implement.** `docs/ALGORITHMEN.md` beschreibt:
  - die Zuordnung der Valhalla-Typen,
  - Winkel und Reichweite,
  - die Nähte an Wegpunkten,
  - die Tabelle der Abstände,
  - Glättung des Tempos, „vorbei“ und „dicht beieinander“,
  - dass Hinweise nur auf der Route gelten.
- [ ] **Step 4: Run, expect PASS.** Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
- [ ] **Step 5: Commit** „Pass turn hints to Swift and describe them“
- [ ] **Step 6: Anhalten.** Den Stand dem Nutzer zeigen und eine Übergabe für den Mac schreiben: `docs/superpowers/plans/2026-11-25-uebergabe-abbiegehinweise-mac.md`, nach dem Muster von `2026-11-24-uebergabe-aufzeichnung-mac.md`.

## Teil B: Trail (Mac)

### Task 7: Hinweisfeld in der App

**Files:**
- Create: `apps/trail/ZeroGridTrail/Guide/TurnBanner.swift`
- Modify: `apps/trail/ZeroGridTrail/Map/MapScreen.swift` (Feld über der `GuideBar`)
- Modify: `apps/trail/ZeroGridTrail/Guide/GuideBar.swift` (Satz „Diese Route hat keine Abbiegehinweise“)
- Modify: `apps/trail/ZeroGridTrail/Guide/DeviationAlert.swift` (`Haptics.turn()`)
- Test: Unit-Tests für die Zuordnung `kind` → SF Symbol, UI-Test im Simulator mit GPX-Bewegung

**Interfaces:**
- Consumes: `GuidanceInfo.turn`, `turnSeq`, `hasTurns` (Task 6, nach `build-core.sh`)
- Produces: `TurnBanner(info: TurnInfo)`, `TurnBanner.symbol(for kind: String) -> String`. Bei Kreisverkehr eine Zahl im Symbol, falls ein passendes SF Symbol fehlt.

- [ ] **Step 1: Failing tests**
  - `TurnBannerTests.symbols`: `"right"` → `arrow.turn.up.right`, `"slight_left"` → `arrow.up.left`, `"u_turn"` → `arrow.uturn.down`, `"roundabout"` → `arrow.triangle.turn.up.right.circle`, `"stage_end"` → `flag.checkered`. Die Namen in der SF-Symbols-App prüfen.
  - UI-Test `turn_banner_appears_and_goes`: eine Route mit einer Abzweigung führen, die simulierte Bewegung fährt durch. Das Element `turn-banner` erscheint mit „rechts“ und verschwindet danach.
- [ ] **Step 2: Run, expect FAIL** (Xcode, Schema `ZeroGridTrail`)
- [ ] **Step 3: Implement.**
  - `GuideController` bekommt den Zähler `countedTurns`, Rücksetzen wie bei `deviation`.
  - Wächst `turnSeq`, vibriert `Haptics.turn()` leicht (`UIImpactFeedbackGenerator(style: .light)`) und gibt den Hinweis an die Ansage (Task 8) weiter.
  - Das Feld zeigt Pfeil, `distanceText` und `label`. Bei `stage == "now"` oder `kind == "stage_end"` steht statt der Distanz „Jetzt“.
  - „Diese Route hat keine Abbiegehinweise“ steht 8 s nach dem Start der Führung in der Leiste, wenn `hasTurns == false`.
- [ ] **Step 4: Run, expect PASS.**
- [ ] **Step 5: Commit** „Show the next turn above the guide bar“

### Task 8: Mitteilung und Sprachansage

**Files:**
- Create: `apps/trail/ZeroGridTrail/Guide/TurnAnnouncer.swift`
- Modify: `apps/trail/ZeroGridTrail/Guide/DeviationAlert.swift` (`NotificationScheduling` um `remove(id:)` erweitern; ohne Ton, wenn Sprache an)
- Modify: `apps/trail/ZeroGridTrail/Map/MapToolbar.swift` (zwei Schalter neben „Signalton bei Abweichung“)
- Modify: `apps/trail/ZeroGridTrail/Info.plist` (`UIBackgroundModes` um `audio`)
- Modify: `apps/trail/ZeroGridTrail/ZeroGridTrailApp.swift` (verdrahten)
- Test: Unit-Tests mit Fake-Center und Fake-Sprecher

**Interfaces:**
- Consumes: `TurnInfo.spoken`, `GuidanceInfo.phase`, `turnSeq` (Task 6), Weitergabe aus Task 7
- Produces:
  - `protocol Speaking { func speak(_ text: String) }`
  - `final class SystemSpeaker: Speaking`: `AVSpeechSynthesizer`, deutsche Stimme. Die Audiositzung `.playback` mit `.duckOthers` und `.interruptSpokenAudioAndMixWithOthers` ist nur während des Sprechens aktiv und wird danach mit `.notifyOthersOnDeactivation` freigegeben. Ein neuer Text unterbricht den alten (`stopSpeaking(at: .immediate)`).
  - `final class TurnAnnouncer`: Schlüssel `turnNotifications` und `turnSpeech`, beide standardmäßig `false`. Methoden `turn(_ info: TurnInfo, appActive: Bool)`, `cleared()`, `deviation()`, `arrived()`.

- [ ] **Step 1: Failing tests (`TurnAnnouncerTests`)**
  - `nothingWhenSwitchesOff`: ein Hinweis im Hintergrund ergibt weder Mitteilung noch Sprache.
  - `notificationReplacesAndIsRemoved`: Mitteilungen an, App im Hintergrund, zwei Hinweise. Beide gehen an die Kennung `"turn"`. Danach entfernt `cleared()` sie.
  - `noNotificationInForeground`.
  - `speechSpeaksSpokenText`: Sprache an ergibt `speak("Jetzt rechts")`, im Vordergrund wie im Hintergrund.
  - `silentNotificationWhenSpeaking`: beide an, also Mitteilung mit `sound: false`.
  - `speaksDeviationAndArrival`: Sprache an ergibt „Abseits der Route“ und „Ziel erreicht“.
- [ ] **Step 2: Run, expect FAIL.**
- [ ] **Step 3: Implement und verdrahten.**
  - `GuideController` ruft `cleared()` auf, sobald `turn` nil wird oder die Führung endet.
  - Bei einer neuen Abweichung ruft er `deviation()` auf, bei `arrived` ruft er `arrived()` auf.
  - `DeviationAlert` schickt seine Mitteilung ohne Ton, wenn `turnSpeech` an ist.
- [ ] **Step 4: Run, expect PASS.** Danach auf dem iPhone von Hand prüfen:
  - Sprache bei gesperrtem Bildschirm,
  - mit Stummschalter,
  - Musik wird leiser und danach wieder lauter.
- [ ] **Step 5: Commit** „Announce turns as notification and by voice“
- [ ] **Step 6: Anhalten.** Den Stand dem Nutzer zeigen.

### Task 9: Test draußen und Abschluss

- [ ] Der Nutzer fährt eine Runde mit dem Rad und geht eine zu Fuß, mit eingeschaltetem und gesperrtem Bildschirm, je mit und ohne Sprache, einmal mit Stummschalter. Zu prüfen:
  - Kommen die Hinweise rechtzeitig und nicht doppelt?
  - Verschwinden Feld und Mitteilung wieder?
  - Wird Musik leiser und danach wieder lauter?
  - Kommen Abzweigungen an Wegpunkten und auf Wegen ohne Namen?
- [ ] Ergebnis in `docs/MESSPROTOKOLL.md`. Abstände in `HintParams` und `docs/ALGORITHMEN.md` nach Bedarf nachstellen.
- [ ] Prüfung des ganzen Branches (superpowers:requesting-code-review), dann PR, Merge lokal mit `--no-ff`, Push nach Rückfrage.
