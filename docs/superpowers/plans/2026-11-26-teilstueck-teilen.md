# Trail: Teilstück wählen und in der Mitte teilen Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** In Trail ein Teilstück der geplanten Route antippen, seine Sportart ändern und es mit einem Punkt auf der halben Streckenlänge in zwei Teilstücke gleicher Sportart teilen.

**Architecture:** Der Kern (`core-app/src/plan.rs`) bekommt `PlanRequest::split_leg`, das den Mittelpunkt entlang der berechneten Linie bestimmt und den Request umbaut; `core-ffi` reicht es als `plan_split_leg` an Swift. Der bestehende Teilstück-Cache sorgt dafür, dass nur die zwei Hälften neu gerechnet werden. In Trail erkennt `RouteOverlay` Tipps auf die Linie, `PlanController` hält das gewählte Teilstück und leitet Sportwahl und Teilen darauf um, `PlanSheet` zeigt statt der Gesamtwerte die Werte des Teilstücks mit zwei Knöpfen.

**Tech Stack:** Rust, UniFFI, SwiftUI, MapLibre Native iOS, XCUITest.

**Spec:** `docs/superpowers/specs/2026-11-26-teilstueck-teilen-design.md`

## Global Constraints

- Branch `feature/leg-split` (besteht, Entwurf committet). Am Ende PR, Merge lokal mit `--no-ff`. Push nur nach Rückfrage.
- **Anhalten und dem Nutzer zeigen:** nach Task 1 (Kern, Linux) und nach Task 3 (Trail auf dem Mac).
- Die Logik liegt im Kern; Swift baut den Request nie selbst um.
- Einfügen oder Profilwechsel rechnet keine unveränderten Teilstücke neu. Gespeicherte Routen öffnen unverändert (kein neues Speicherformat).
- Der neue Punkt liegt auf der halben Länge der **berechneten** Linie; ohne Linie (fehlgeschlagenes Teilstück, weniger als zwei Punkte oder Länge 0) auf der Mitte der Luftlinie zwischen den beiden Wegpunkten.
- Beide Hälften erben `LegSpec` (Profil und Einstellungen) des geteilten Teilstücks.
- Ohne Auswahl gilt die Sportwahl wie heute für das letzte Teilstück und alle neuen.
- Texte wörtlich: Kopf „Teilstück {n}“ (n ab 1), Knopf „In der Mitte teilen“ (Bedienhilfe „Punkt in der Mitte einfügen“), Knopf Auswahl schließen mit Bedienhilfe „Auswahl aufheben“.
- Swift lässt sich unter Linux nicht bauen. Tasks 2–3 laufen auf dem Mac, vorher `apps/trail/scripts/build-core.sh` und `xcodegen generate` in `apps/trail`.
- UI-Tests mit echten Tipps (`app.place(x, y).tap()`), wie in `PlanUITests`. Neue Ansichten funktionieren auch im OLED-Dunkelmodus.
- Echte Karten in `data/` nicht anfassen. Jeder Commit endet mit `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. Teilen eines Teilstücks vor einem Etappenende: das Etappenende bleibt am selben Ort (Index um eins verschoben). → Test in Task 1.
2. Teilen eines fehlgeschlagenen (gestrichelten) Teilstücks: Punkt auf der Luftlinienmitte, kein Absturz. → Tests in Task 1 und Task 3.
3. Auswahl zeigt auf ein Teilstück, das es nach Rückgängig/Entfernen/Leeren/Laden nicht mehr gibt: Auswahl verschwindet, keine Index-Abstürze. → Test in Task 2.
4. Nach dem Aufheben der Auswahl ändert die Sportwahl wieder das letzte Teilstück, nicht das zuletzt gewählte. → Test in Task 2.
5. Doppeltipp auf die Linie zoomt nur und wählt nichts (bestehende Doppeltipp-Logik in `RouteOverlay.tapped`). → Test in Task 2.

---

### Task 1: Kern — `split_leg` und `plan_split_leg`

**Files:**
- Modify: `crates/core-app/src/plan.rs` (in `impl PlanRequest`, nach `set_leg`)
- Modify: `crates/core-ffi/src/plan.rs` (nach `plan_set_leg`), `crates/core-ffi/src/lib.rs:24-25` (Re-Export)
- Test: `crates/core-app/tests/plan_edit.rs`, `crates/core-app/tests/routes.rs`, `crates/core-ffi/tests/plan.rs`

**Interfaces:**
- Produces (Rust): `PlanRequest::split_leg(&self, index: usize, line: &[LatLon]) -> Self` (`#[must_use]`). Fügt einen Wegpunkt an Index `index + 1` ein, dupliziert `legs[index]` an `index + 1`, jedes Etappenende `> index` wird um eins erhöht. `index` außerhalb von `legs` → unveränderte Kopie.
- Produces (Swift über UniFFI): `planSplitLeg(request: PlanInput, index: UInt32, line: [LatLon]) -> PlanInput`.

- [ ] **Step 1: Tests in `plan_edit.rs` schreiben** (Helfer `at`, `leg`, `plan`, `profiles`, `valid` gibt es dort)

```rust
#[test]
fn split_inserts_point_on_half_length_of_the_line() {
    let request = plan(&[Hike, Mtb, Car], &[]);
    // Ein Knick: 0→1 kurz, 1→2 lang; die halbe Länge liegt im zweiten Stück.
    let line = vec![ll(47.0, 11.0), ll(47.0, 11.001), ll(47.003, 11.001)];
    let split = request.split_leg(1, &line);
    assert_eq!(profiles(&split), vec![Hike, Mtb, Mtb, Car]);
    assert_eq!(split.waypoints.len(), 5);
    assert_eq!(split.waypoints[1], at(1));
    assert_eq!(split.waypoints[3], at(2));
    let mid = split.waypoints[2];
    let total = dist(&line[0], &line[1]) + dist(&line[1], &line[2]);
    assert!((dist(&line[0], &line[1]) + dist(&line[1], &mid) - total / 2.0).abs() < 0.5);
    assert!((mid.lon - 11.001).abs() < 1e-9, "auf dem zweiten Stück der Linie");
    valid(&split);
}

#[test]
fn split_keeps_options_of_the_leg() { /* legs[0].options mit gesetztem Wert; nach split_leg(0, …) haben legs[0] und legs[1] dieselben options */ }

#[test]
fn split_shifts_later_stage_ends() {
    let request = plan(&[Hike, Hike, Hike, Hike], &[1, 3]);
    let split = request.split_leg(1, &[at(1), at(2)]);
    assert_eq!(split.stage_ends, vec![1, 4]);
    valid(&split);
}

#[test]
fn split_without_line_uses_middle_of_the_waypoints() {
    let request = plan(&[Hike], &[]);
    for line in [vec![], vec![at(0)], vec![at(0), at(0)]] {
        let split = request.split_leg(0, &line);
        let mid = split.waypoints[1];
        assert!((mid.lat - (at(0).lat + at(1).lat) / 2.0).abs() < 1e-12);
        assert!((mid.lon - (at(0).lon + at(1).lon) / 2.0).abs() < 1e-12);
    }
}

#[test]
fn split_outside_the_route_changes_nothing() {
    let request = plan(&[Hike, Mtb], &[]);
    assert_eq!(request.split_leg(2, &[at(0), at(1)]), request);
    assert_eq!(PlanRequest::empty().split_leg(0, &[]), PlanRequest::empty());
}
```

`ll(lat, lon)` und `dist(a, b)` (= `a.distance_m(b)`) als kleine Helfer in der Testdatei anlegen.

- [ ] **Step 2: Cache-Test in `routes.rs`** an `only_changed_legs_are_computed` anhängen (oder als eigener Test mit `counting_app("split")`):

```rust
// Teilen rechnet nur die zwei Hälften.
let before = calls();
let line = plan.legs[1].line.clone();
let split = request.split_leg(1, &line);
let result = app.plan_route(&split).unwrap();
assert_eq!(calls(), before + 2, "splitting computes only the two halves");
assert_eq!(result.legs.len(), request.legs.len() + 1);
```

- [ ] **Step 3: FFI-Test in `crates/core-ffi/tests/plan.rs`**: `plan_split_leg` auf einem `PlanInput` mit zwei Teilstücken (`"hike"`, `"mtb"`) und `index: 1` ergibt drei Teilstücke mit Profilen `["hike","mtb","mtb"]`; ein `LegInput` mit unbekanntem Profil im Request lässt den Request unverändert (wie die anderen `plan_*`-Funktionen über `edit`).

- [ ] **Step 4: Tests laufen lassen, sie schlagen fehl**

Run: `cargo test -p core-app --test plan_edit --test routes && cargo test -p core-ffi --test plan`
Expected: FAIL, `no method named split_leg` / `cannot find function plan_split_leg`.

- [ ] **Step 5: `split_leg` implementieren.** Mittelpunkt in einer privaten Hilfe `fn half_way(line: &[LatLon]) -> Option<LatLon>`: Länge mit `LatLon::distance_m` aufsummieren, `None` bei Länge 0 oder weniger als zwei Punkten, sonst linear zwischen den beiden Punkten interpolieren, zwischen denen die halbe Länge liegt (wie `guide/line.rs` `point_at`). Fallback: Mittel der Koordinaten von `waypoints[index]` und `waypoints[index + 1]`.

- [ ] **Step 6: `plan_split_leg` exportieren** in `core-ffi/src/plan.rs` mit `#[uniffi::export]`, Doc-Kommentar „Splits leg `index` at half its length along `line`; both halves keep its profile.“, Umsetzung über `edit(request, |r| r.split_leg(index as usize, &points))` mit `line` in `core_routing::LatLon` umgewandelt; in `lib.rs` re-exportieren.

- [ ] **Step 7: Tests und Lints**

Run: `cargo test -p core-app -p core-ffi && cargo clippy -p core-app -p core-ffi --all-targets -- -D warnings && cargo fmt --check`
Expected: alle grün, keine Warnungen.

- [ ] **Step 8: Commit** — `git commit -m "Split a leg at half its length in the core"` (mit Co-Authored-By-Zeile).

- [ ] **Step 9: Anhalten**, dem Nutzer zeigen, Übergabe für den Mac: welche Tasks folgen, `build-core.sh` + `xcodegen generate`.

---

### Task 2: Trail — Teilstück antippen, hervorheben, Sportart nur dafür

**Files:**
- Modify: `apps/trail/ZeroGridTrail/Plan/RouteLayers.swift`
- Modify: `apps/trail/ZeroGridTrail/Plan/PlanController.swift`
- Test: `apps/trail/ZeroGridTrailUITests/PlanUITests.swift`

**Interfaces:**
- Produces (`RouteOverlay`): `var onLegTap: ((Int) -> Void)?`; `func select(leg: Int?)`. Jedes Feature in `shape()` bekommt das Attribut `"leg": Int` (Index in `plan.legs`, auch für gestrichelte) und `"selected": Bool`.
- Produces (`PlanController`): `@Published private(set) var selectedLeg: Int?`; `func selectLeg(_ index: Int?)`. Statuszeile `selected_leg=<n|->`.

- [ ] **Step 1: UI-Tests schreiben** (mit `XCUIApplication.planning()`, `places`, `waitForPlan`; im Fixture `mini` sind Skitour-(manuell)-Teilstücke gerade Linien, ihre Mitte ist der Mittelwert der beiden Tippstellen):

```swift
/// Ein Tipp auf die Linie wählt das Teilstück, die Sportart gilt dann nur ihm.
func testTapOnLegSelectsItAndProfileAppliesToIt() {
    let app = XCUIApplication.planning()
    app.setWaypoints(Array(places.prefix(3)))
    app.waitForPlan(waypoints: 3)
    app.buttons["plan-pick"].tap()          // Setzmodus aus
    app.place(0.5, 0.415).tap()             // Mitte von places[0]–places[1]
    app.waitForStatus("leg 0 selected") { $0.values["selected_leg"] == "0" }
    app.chooseProfile("Wandern")
    let status = app.waitForStatus("first leg is a hike") {
        $0.int("planning") == 0 && $0.list("leg_profiles") == ["hike", "manual"]
    }
    XCTAssertEqual(status.int("waypoints"), 3)
}

/// Im Setzmodus wählt ein Tipp auf die Linie und setzt keinen Punkt.
func testTapOnLegInPickModeSetsNoPoint() { /* wie oben ohne Setzmodus-aus; danach waypoints == 3, selected_leg == 0 */ }

/// Nach dem Aufheben gilt die Sportart wieder dem letzten Teilstück.
func testDeselectedProfileAppliesToLastLegAgain() {
    /* Teilstück 0 wählen, Tipp ins Leere (place(0.85, 0.3)) → selected_leg == "-",
       chooseProfile("Wandern") → leg_profiles == ["manual","hike"] */
}

/// Eine Auswahl, deren Teilstück es nicht mehr gibt, verschwindet.
func testSelectionGoesWithItsLeg() {
    /* Teilstück 1 wählen, "plan-undo" tippen → selected_leg == "-", legs == 1;
       erneut wählen (Teilstück 0), "plan-clear" → selected_leg == "-" */
}

/// Ein Doppeltipp auf die Linie zoomt nur.
func testDoubleTapOnLegSelectsNothing() { /* place(0.5,0.415).doubleTap() → nach 1 s selected_leg == "-" */ }
```

Hinweis: Ein Tipp ins Leere außerhalb des Setzmodus ruft wie bisher `onFreeTap` (Suche/„Route hierher“); der Test wählt eine Stelle, an der das nichts öffnet, oder schließt, was erscheint.

- [ ] **Step 2: Tests auf dem Mac laufen lassen, sie schlagen fehl**

Run: `apps/trail/scripts/test.sh -only-testing:ZeroGridTrailUITests/PlanUITests`
Expected: die neuen Tests FAIL (kein `selected_leg` im Status), die alten PASS.

- [ ] **Step 3: `RouteOverlay` erweitern.** In `tapped()` (im verzögerten `DispatchWorkItem`, damit Doppeltipps weiter verworfen werden) zuerst `mapView.visibleFeatures(in: CGRect(center: tapPoint, size: 44×44), styleLayerIdentifiers: ["route", "route-failed"])` abfragen; trifft es ein Feature, `onLegTap?(leg)` mit dem nächstgelegenen (bei mehreren der kleinste Abstand zur Linie oder schlicht das erste), sonst wie bisher `onTap?(at)`. Neue Ebene `route-selected` unter `route`, aber über `route-casing`: gleiche Quelle, `predicate selected == TRUE`, Breite 10, Farbe des Teilstücks (mit `readableOnBlack` im Dunkelmodus), Deckkraft 0.45. `select(leg:)` merkt sich den Index und setzt `shape` neu.

- [ ] **Step 4: `PlanController` erweitern.**
  - `overlay.onLegTap = { selectLeg($0) }`; `selectLeg` ignoriert Indizes außerhalb von `request.legs` und läuft nicht während `replacing`.
  - Beim Wählen zeigt das Formular das Teilstück: `profile = legs[i].profile`, `options` aus `legs[i].options`.
  - `choose(profile:)` / `set(option:)` → `formChanged()`: mit Auswahl `planSetLeg(request:index:leg: formLeg)`, sonst wie heute `planSetLastLeg`. `lastProfileKey` nur ohne Auswahl schreiben.
  - Aufheben (`selectLeg(nil)`, Tipp ins Leere – also in `overlay.onTap` vor allem anderen: ist etwas gewählt, nur aufheben und den Tipp nicht weitergeben –, `clear()`, `load(_:)`): danach zeigt das Formular wieder das letzte Teilstück (Profil und Optionen), ohne neu zu rechnen.
  - Nach jeder Änderung von `request`: ist `selectedLeg >= request.legs.count`, aufheben.
  - `drawLegs()` gibt `selectedLeg` an `overlay.select(leg:)`; Status um `"selected_leg=\(selectedLeg.map(String.init) ?? "-")"` ergänzen.

- [ ] **Step 5: Tests laufen lassen**

Run: `apps/trail/scripts/test.sh -only-testing:ZeroGridTrailUITests/PlanUITests`
Expected: alle PASS.

- [ ] **Step 6: Commit** — `git commit -m "Select a leg of the route by tapping it"`.

---

### Task 3: Trail — Werte des Teilstücks und „In der Mitte teilen“

**Files:**
- Create: `apps/trail/ZeroGridTrail/Plan/LegSummary.swift`
- Modify: `apps/trail/ZeroGridTrail/Plan/PlanSheet.swift` (`totals`), `apps/trail/ZeroGridTrail/Plan/PlanController.swift`
- Test: `apps/trail/ZeroGridTrailUITests/PlanUITests.swift`

**Interfaces:**
- Consumes: `planSplitLeg(request:index:line:)` (Task 1), `selectedLeg`, `selectLeg(_:)` (Task 2).
- Produces (`PlanController`): `func splitSelectedLeg()` — ruft `planSplitLeg` mit `plan.legs[i].line` (leer für ein fehlgeschlagenes Teilstück), hebt die Auswahl auf, `compute()`.
- Produces: `struct LegSummary: View` mit `plan: PlanController`; Bezeichner `leg-summary` (Text), `leg-split` (Knopf), `leg-close` (Knopf).

- [ ] **Step 1: UI-Tests schreiben**

```swift
/// Teilen setzt einen Punkt in die Mitte; beide Hälften behalten die Sportart.
func testSplitInsertsPointInTheMiddle() {
    let app = XCUIApplication.planning()
    app.setWaypoints(Array(places.prefix(3)))
    app.waitForPlan(waypoints: 3)
    app.buttons["plan-pick"].tap()
    app.place(0.5, 0.415).tap()
    XCTAssertTrue(app.staticTexts["leg-summary"].waitForExistence(timeout: 5))
    XCTAssertTrue(app.staticTexts["leg-summary"].label.hasPrefix("Teilstück 1"))
    let before = app.waitForPlan(waypoints: 3).int("distance_m")
    app.buttons["leg-split"].tap()
    let after = app.waitForPlan(waypoints: 4)
    XCTAssertEqual(after.list("leg_profiles"), ["manual", "manual", "manual"])
    XCTAssertEqual(after.values["selected_leg"], "-")
    XCTAssertEqual(after.int("distance_m"), before, accuracy: 2, "die Route bleibt gleich")
}

/// Ein gestricheltes Teilstück lässt sich auch teilen.
func testSplitFailedLeg() {
    /* drei Punkte, letztes Teilstück auf "Wandern" (schlägt in mini fehl, dashed == 1),
       Teilstück 1 antippen (Mitte von places[1]–places[2]: place(0.6, 0.525)), "leg-split"
       → waypoints 4, leg_profiles ["manual","hike","hike"], dashed == 2 */
}

/// Schließen hebt die Auswahl auf, die Gesamtwerte kommen zurück.
func testCloseLegSummary() { /* wählen, "leg-close" → selected_leg "-", "plan-totals" existiert */ }
```

- [ ] **Step 2: Tests auf dem Mac laufen lassen, sie schlagen fehl**

Run: `apps/trail/scripts/test.sh -only-testing:ZeroGridTrailUITests/PlanUITests`
Expected: die drei neuen FAIL.

- [ ] **Step 3: `LegSummary` bauen und in `PlanSheet.totals` einsetzen.** Ist `plan.selectedLeg` gesetzt und `plan.plan` vorhanden, ersetzt `LegSummary` die Zeile `totals` (gleiche Höhe, damit die eingeklappte Stufe von 112 pt reicht): Farbpunkt des Teilstücks, Text „Teilstück {n}  ·  {Format.distance}  ·  ↑ {Format.elevation}  ·  ↓ {Format.elevation}“ (Auf-/Abstieg nur, wenn bekannt; bei fehlgeschlagenem Teilstück statt der Werte die Fehlermeldung über `PlanTexts.error(code:fallback:)`), rechts Knopf „In der Mitte teilen“ (`leg-split`) und ein Schließen-Symbol `xmark` (`leg-close`, Bedienhilfe „Auswahl aufheben“). Beide Knöpfe gesperrt, solange `plan.planning`.

- [ ] **Step 4: `splitSelectedLeg()` in `PlanController`** wie im Interfaces-Block.

- [ ] **Step 5: Alle Trail-Tests laufen lassen**

Run: `apps/trail/scripts/test.sh`
Expected: alle PASS (auch Hell/Dunkel-Ansicht des Teilstücks im Simulator kurz ansehen).

- [ ] **Step 6: Commit** — `git commit -m "Split the selected leg in the middle"`.

- [ ] **Step 7: Anhalten**, dem Nutzer zum Test auf dem iPhone geben; danach PR nach Rückfrage.

---

## Ergebnis (2026-11-26)

- Task 1 unter Linux (Rust-Tests, Clippy, fmt grün).
- Tasks 2 und 3 auf dem Mac: alle Plan-UI-Tests (19) im Simulator grün, ebenso Abnahme- und
  „Route hierher“-Tests. Ein Lauf der ganzen Suite vor der letzten Änderung: 60 UI-Tests, nur
  `AcceptanceUITests.testLegTimes` fehlgeschlagen (siehe unten), danach behoben.
- **Abweichung vom Plan, Entscheidung des Nutzers:** Im Setzmodus setzt jeder Tipp einen Punkt,
  auch auf der Linie (`testLegTimes` tippte auf die eigene Route; Hin- und Rückweg auf demselben
  Pfad). Teilstücke wählt man außerhalb des Setzmodus; Einschalten hebt die Auswahl auf.
  `testTapOnLegInPickModeSetsNoPoint` wurde zu `testTapOnLegInPickModeSetsPoint`.
- Doppeltipp: Der zweite Tipp konnte nach dem Abbrechen der wartenden Tipps noch ankommen und
  als Einzeltipp zählen; jetzt verworfen.
- Auf dem iPhone 13 Pro Max (Debug-Build) vom Nutzer geprüft: Teilstück wählen, Sportart nur
  dafür, In der Mitte teilen, Auswahl aufheben – alles wie vorgesehen.
- Offen: „Rückgängig“ nimmt nur den letzten Punkt zurück, nicht das Teilen.
