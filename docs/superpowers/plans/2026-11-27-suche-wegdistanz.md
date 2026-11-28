# Suche: echte Wegdistanz Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Mit dem Umschalter „Weg“ in der Suchleiste und bekannter GPS-Position zeigt Trail zu jedem Suchtreffer Wegstrecke und Zeit von der eigenen Position für die eingestellte Sportart und sortiert danach.

**Architecture:** `core-routing` bekommt `Router::matrix` (eine Quelle, viele Ziele), von `ValhallaRouter` über `Actor::matrix` umgesetzt. `core-app` bekommt `search_from`/`search_category_from`, die nach der heutigen Suche die angezeigten Treffer von der Position aus routen und neu ordnen, noch mit `match_rank` in der Hand. `core-ffi` reicht ein optionales `RouteFrom` durch die bestehenden Suchaufrufe. Trail fragt die Position und die Etappen-Einstellungen nur ab, wenn der Umschalter an ist, und zeigt die neuen Felder.

**Tech Stack:** Rust, Valhalla 0.7.6 (`valhalla`-Crate, Feature `valhalla`), serde_json, UniFFI, SwiftUI, XCTest.

**Spec:** `docs/superpowers/specs/2026-11-27-suche-wegdistanz-design.md`

## Global Constraints

- Branch `feature/search-route-distance` (von `main`, ded03b7). Am Ende PR, Merge lokal mit `--no-ff`. Push nur nach Rückfrage.
- **Anhalten und dem Nutzer zeigen:** nach Task 1 (Messwerte), nach Task 3 (Kern fertig; Push für den Mac nach Rückfrage), nach Task 4 (iPhone-Test).
- Grenze fürs Routen: `ROUTE_LIMIT_M = 50_000.0` Luftlinie von der Position (Nutzer). Ist die Matrix bei 50 km deutlich über ~300 ms, **nicht selbst ändern, sondern den Nutzer fragen.**
- Suchgrenze der Matrix (Nutzer, Variante A): `max_matrix_distance` je Profil so, dass der ungünstige Fall auf `alpenraum-ost` bei 50 km etwa ≤ 300 ms bleibt; nicht erreichte Ziele = `None` = „Umweg oder kein Weg“.
- Position zählt nur, wenn `age_s <= MAX_POSITION_AGE_S` (120 s, `crates/core-app/src/plan.rs:27`).
- Ohne `RouteFrom`, mit zu alter Position: Ergebnis exakt wie heute (gleiche Reihenfolge, `distance_m` von der Kartenmitte, keine Wegfelder).
- Sortierung mit Position: zuerst `match_rank` (wie `best_first`), innerhalb gleicher Güte die Gruppen der Spec (1 mit Weg nach Wegstrecke, 2 ≤ 50 km ohne Weg nach Luftlinie, 3 sonst nach Luftlinie); ferne Orte außerhalb des Radius bleiben in ihren reservierten Plätzen am Ende.
- Texte wörtlich: Umschalter „Weg“, Hinweis „Umweg oder kein Weg“, Luftlinie „Luftl. “ + `Format.distance` (z. B. „Luftl. 1,10 km“), Hinweis unter der Liste „Zeiten ohne Höhenmeter.“
- Keine Änderung an Index, Suchradius, Kartenanpassung. Die Suche schaltet das GPS nicht ein. Umschalter nach Installation aus.
- Möglichst nicht anfassen (laufende Arbeit auf `feature/tour-export`): `crates/core-ffi/tests/ffi.rs`, `crates/core-app/src/app.rs`, `error.rs`. In `crates/core-app/src/lib.rs` nur die Exportzeile von `search` erweitern.
- Swift lässt sich unter Linux nicht bauen: Task 4 auf dem Mac, vorher `apps/trail/scripts/build-core.sh` und `xcodegen generate` in `apps/trail`. Echte Karten in `data/` nur lesen. Jeder Commit endet mit `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. Ziel liegt in einer anderen Region als die Position (Grenzgebiet, zwei Regionen installiert): kein Fehler, Treffer bekommt „kein Weg“, die anderen werden normal geroutet. → Test `target_outside_the_region_has_no_route` in Task 2.
2. Matrix scheitert ganz (Engine-Fehler, Routing-Daten anderer Valhalla-Version, Profil `manual`, keine Routing-Region an der Position): Suche liefert trotzdem, Luftlinie von der Position, ohne „kein Weg“-Etiketten. → Test `failed_matrix_falls_back_to_air_distance` in Task 2.
3. Treffer liegt fast auf der Position (< 5 m) oder zwei Treffer am selben Punkt: Matrix liefert 0 m bzw. gleiche Werte, nichts wird als „kein Weg“ gewertet, Reihenfolge stabil. → Test `target_at_the_position_counts_as_routed` in Task 1 (Fixture).
4. Schnell weitertippen mit Umschalter an: eine überholte Suche samt Matrix ersetzt nie die neuere Liste (bestehender `request`-Zähler muss die gesamte Arbeit umfassen). → Test `slow_routed_answer_does_not_replace_newer` in Task 4.
5. Umschalter umlegen ohne Position oder während eine Karte offen ist: kein Absturz, Liste unverändert bzw. Suche läuft neu, Karte des gewählten Treffers bleibt. → Test `toggle_without_position_keeps_list` in Task 4.

---

### Task 1: `core-routing` — Distanzmatrix

**Files:**
- Modify: `crates/core-routing/src/model.rs` (Typen, Trait-Methode mit Standard)
- Modify: `crates/core-routing/src/valhalla_api.rs` (JSON hin und zurück)
- Modify: `crates/core-routing/src/valhalla_router.rs` (`matrix`)
- Modify: `crates/core-routing/src/lib.rs` (Exporte)
- Test: `crates/core-routing/src/valhalla_api.rs` (Unit-Tests), `crates/core-routing/tests/valhalla.rs` (Fixture, Messung)

**Interfaces:**
- Produces:
  - `pub struct MatrixRequest { pub from: LatLon, pub to: Vec<LatLon>, pub profile: Profile, pub options: RouteOptions }` (`Debug, Clone, PartialEq`)
  - `pub struct MatrixEntry { pub distance_m: f64, pub duration_s: f64 }` (`Debug, Clone, Copy, PartialEq`)
  - im Trait `Router`: `fn matrix(&self, request: &MatrixRequest) -> Result<Vec<Option<MatrixEntry>>, RoutingError>`; Standard: `Err(RoutingError::Unavailable("this router has no matrix".into()))`. Ergebnis hat genau `request.to.len()` Einträge in Zielreihenfolge, `None` = kein Weg.
  - `pub fn matrix_json(request: &MatrixRequest) -> Result<String, RoutingError>` und `pub fn parse_valhalla_matrix(json: &str, targets: usize) -> Result<Vec<Option<MatrixEntry>>, RoutingError>` in `valhalla_api.rs`.

- [ ] **Step 1: Unit-Tests in `valhalla_api.rs` schreiben**
  - `matrix_json_has_sources_targets_and_costing`: für `Profile::Mtb`, zwei Ziele: `sources` hat einen Eintrag mit `lat`/`lon` von `from`, `targets` zwei, `costing == "bicycle"`, `costing_options.bicycle.bicycle_type == "mountain"`, `units == "kilometers"`, `verbose == true`; keine `locations`.
  - `matrix_json_rejects_manual`: `Profile::Manual` → `Err(RoutingError::NotRoutable(Profile::Manual))`.
  - `matrix_json_checks_options` : `use_hills: Some(2.0)` → `Err(RoutingError::InvalidOption(_))` (gleiche Prüfung wie `costing_json`).
  - `matrix_answer_becomes_entries`: Antwort `{"sources_to_targets":[[{"to_index":1,"time":600,"distance":2.5},{"to_index":0,"time":null,"distance":null}]]}` mit `targets = 2` → `[None, Some(MatrixEntry { distance_m: 2500.0, duration_s: 600.0 })]` (nach `to_index` eingeordnet, km → m).
  - `matrix_answer_with_wrong_shape_is_an_engine_error`: `{}` und eine Zeile mit 1 statt 2 Einträgen → `Err(RoutingError::Engine(_))`.

- [ ] **Step 2: Tests laufen lassen, sie scheitern**

Run: `cargo test -p core-routing --lib matrix`
Expected: FAIL (Funktionen fehlen)

- [ ] **Step 3: Umsetzen**
  - In `valhalla_api.rs` den Teil von `costing_json`, der aus Profil und `RouteOptions` `(costing, costing_options)` baut und prüft, in eine private Funktion ziehen; `costing_json` und `matrix_json` nutzen sie. Bestehende Tests von `costing_json` bleiben grün.
  - `matrix_json`: `{"sources":[…], "targets":[…], "costing", "costing_options", "units":"kilometers", "verbose":true}`.
  - `ValhallaRouter::matrix`: `Actor::parse_json_request(&json, Action::SourcesToTargets)`, dann `actor.matrix(&options)`; `Response::Json` → `parse_valhalla_matrix`, Fehler wie bei `route` über `engine_error`. Ein `no route`-Fehler für alle Ziele zusammen wird zu `Ok(vec![None; n])`.
  - `to` leer → `Ok(vec![])` ohne Valhalla-Aufruf.

- [ ] **Step 4: Unit-Tests grün**

Run: `cargo test -p core-routing --lib`
Expected: PASS

- [ ] **Step 5: Fixture-Tests in `tests/valhalla.rs`**
  - `matrix_matches_single_routes`: `MatrixRequest { from: STATION, to: vec![NEUREUTH, SOMMERWEG[0]], profile: Hike, … }`: beide `Some`; Distanz je innerhalb ±10 % der `route()`-Distanz derselben Strecke; `duration_s > 0`.
  - `target_at_the_position_counts_as_routed`: Ziel = `STATION` selbst → `Some` mit `distance_m < 100.0`.
  - `matrix_for_every_valhalla_profile`: für `Hike, Trekking, Mtb, RoadBike, Car` mit Ziel `NEUREUTH` gelingt der Aufruf (`Ok`, Länge 1).

Run: `cargo test -p core-routing --features valhalla --test valhalla matrix`
Expected: PASS

- [ ] **Step 6: Messung mit echter Region**

Test `measure_matrix_on_real_tiles` mit `#[ignore]`: liest `ZG_MATRIX_TILES` (Pfad zu `routing.tar`), Quelle Bozen `46.4983, 11.3548`; für jeden Radius 10, 25, 50 km je 30 Ziele gleichmäßig auf dem Kreis; für `Hike`, `Trekking`, `Mtb` eine Aufwärmrunde, dann drei Läufe; druckt je Profil und Radius Median ms und Zahl der `Some`. Prüft nichts außer `Ok`. Greift Valhallas `service_limits` (Fehler „exceeds the max distance“ o. ä.), in `ValhallaRouter::open` über `ConfigBuilder` für die Matrix `max_matrix_distance` ≥ 120 000 m und `max_matrix_location_pairs` ≥ 100 setzen und den Test wiederholen.

Run: `ZG_MATRIX_TILES=/mnt/data_ssd/Coding/zerogrid/data/regions/suedtirol/routing.tar cargo test --release -p core-routing --features valhalla --test valhalla measure_matrix -- --ignored --nocapture`
Expected: Tabelle der Zeiten. Am iPhone rechnete Valhalla im Spike so schnell wie der Desktop, die Zahlen sind also eine brauchbare Schätzung.

- [ ] **Step 7: Clippy und Commit**

Run: `cargo clippy -p core-routing --features valhalla --all-targets -- -D warnings` → keine Warnungen.

```bash
git add crates/core-routing
git commit -m "Route from one point to many with Valhalla's matrix"
```

**Anhalten:** dem Nutzer die Messtabelle zeigen. Liegt 50 km bei einem Profil deutlich über ~300 ms, nach der Grenze fragen, bevor Task 2 beginnt.

---

### Task 2: `core-app` — Treffer von der Position routen und ordnen

**Files:**
- Create: `crates/core-app/src/search_routes.rs` (Messen, Matrix, Ordnung)
- Modify: `crates/core-app/src/search.rs` (`search_from`, `search_category_from`, neue Felder, `share_limit` ordnet nicht mehr selbst)
- Modify: `crates/core-app/src/lib.rs` (`mod search_routes;`, Export `RouteFrom`)
- Test: `crates/core-app/tests/search.rs`

**Interfaces:**
- Consumes: `Router::matrix`, `MatrixRequest`, `MatrixEntry` (Task 1); `KnownPosition`, `LegSpec`, `MAX_POSITION_AGE_S` (`plan.rs`); `Store::routing_tiles`; `Routing::router`; Versionsprüfung wie in `route_on` (`plan.rs:547-575`).
- Produces:
  - `pub struct RouteFrom { pub position: KnownPosition, pub leg: LegSpec }` (`Debug, Clone, PartialEq`)
  - `SearchHit` neu: `pub centre_distance_m: f64` (Luftlinie zur Kartenmitte, immer gesetzt), `pub route_m: Option<f64>`, `pub route_s: Option<f64>`, `pub no_route: bool`. `distance_m` = Luftlinie von der Position, wenn die Position zählt, sonst wie heute.
  - `SearchResults` neu: `pub ascent_ignored: bool` (Profil `Hike` und mindestens ein Treffer mit `route_s`).
  - `impl App { pub fn search_from(&self, text: &str, near: LatLon, radius_m: f64, limit: usize, from: Option<&RouteFrom>) -> SearchResults; pub fn search_category_from(&self, category: &str, near: LatLon, radius_m: f64, limit: usize, from: Option<&RouteFrom>) -> Result<SearchResults, AppError> }`. `search`/`search_category` bleiben und rufen diese mit `None`.
  - `pub const ROUTE_LIMIT_M: f64 = 50_000.0;` in `search_routes.rs`.

- [ ] **Step 1: Tests schreiben** (`tests/search.rs`). Der `region`-Helfer bekommt eine Variante, die `search.sqlite` **und** ein Dummy-`routing.tar` ins Manifest schreibt; der Test-Router (`App::set_router_for_tests("…", Arc::new(FakeMatrix(…)))`) liefert feste `MatrixEntry` je Ziel (nach Koordinaten) und zählt Aufrufe.
  - `without_position_nothing_changes`: `search_from(…, None)` == `search(…)`; mit Position `age_s = 121.0` ebenso, Router 0 Aufrufe.
  - `routed_hits_sort_by_route_distance`: Kategorie `lodging`, Hostel A 1 km Luftlinie / 6 km Weg, B 2 km / 3 km → Reihenfolge B, A; `route_m`, `route_s` gesetzt; `distance_m` = Luftlinie von der Position; `centre_distance_m` = Luftlinie zur Kartenmitte.
  - `match_quality_goes_before_route`: Text „Seeblick“: exakter Treffer mit 3 km Weg vor halbem Treffer mit 1 km Weg.
  - `hit_without_route_goes_after_routed_ones`: Router gibt `None` → `no_route == true`, steht hinter allen gerouteten derselben Güte.
  - `far_hits_are_not_routed`: Treffer 60 km von der Position (Position weit weg von der Kartenmitte): nicht im `MatrixRequest.to`, `no_route == false`, steht hinter Gruppe 1 und 2.
  - `target_outside_the_region_has_no_route`: zweite Region ohne Routing; Treffer dort: nicht im Matrix-Aufruf, `no_route == true`.
  - `failed_matrix_falls_back_to_air_distance`: Router gibt `Err(Engine)`; ebenso Profil `Manual` und Position ohne Routing-Region: alle `route_m == None`, `no_route == false`, sortiert nach `match_rank`, dann Luftlinie von der Position.
  - `far_places_keep_their_slots`: weit entfernter Ort außerhalb des Radius bleibt am Ende, nicht geroutet, `distance_m` von der Position.
  - `ascent_note_only_for_hiking`: `Hike` mit Weg → `ascent_ignored`; `Trekking` → nicht.
  - `only_shown_hits_are_routed`: `limit = 3` bei 5 Treffern → `to.len() == 3`, ein einziger Matrix-Aufruf.

- [ ] **Step 2: Tests scheitern**

Run: `cargo test -p core-app --test search`
Expected: FAIL (fehlende Funktionen/Felder)

- [ ] **Step 3: Umsetzen.** Ablauf in `search_from` (für Kategorie gleich):
  1. Treffer wie heute suchen, filtern und auf `limit` kürzen (Reihenfolge nach Kartenmitte wie heute), aber als `Hit` behalten.
  2. Zählt die Position: Luftlinie von ihr je Treffer; Region `routing_tiles(&[position])`, Version wie in `route_on` prüfen; Ziele = Treffer im Radius mit Luftlinie ≤ `ROUTE_LIMIT_M`, deren `routing_tiles(&[position, ziel])` dieselbe Region liefert; ein `matrix`-Aufruf.
  3. Ordnen innerhalb des Radius nach (`match_rank`, Gruppe, Schlüssel); ferne Orte bleiben dahinter wie bei `share_limit`. `share_limit` übernimmt die Reihenfolge, die es bekommt.
  4. `no_route` nur, wenn die Matrix lief und das Ziel `None` bekam oder außerhalb der Region liegt.

- [ ] **Step 4: Tests grün, Rest unverändert**

Run: `cargo test -p core-app` → PASS. `cargo clippy -p core-app --all-targets -- -D warnings` → keine Warnungen.

- [ ] **Step 5: Commit**

```bash
git add crates/core-app
git commit -m "Order search hits by the way from the own position"
```

---

### Task 3: `core-ffi` — `RouteFrom` und neue Felder für Swift

**Files:**
- Modify: `crates/core-ffi/src/search.rs`
- Test: `crates/core-ffi/src/search.rs` (Modul `tests`)
- Create: `docs/superpowers/plans/2026-11-27-uebergabe-suche-wegdistanz-mac.md`

**Interfaces:**
- Consumes: `App::search_from`, `App::search_category_from`, `core_app::RouteFrom` (Task 2); `KnownPosition` (FFI, schon da), `LegInput`, `leg_from` (`crates/core-ffi/src/plan.rs:285`).
- Produces (Swift sieht camelCase):
  - `#[derive(uniffi::Record)] pub struct RouteFrom { pub position: KnownPosition, pub leg: LegInput }`
  - FFI-`SearchHit` + `centre_distance_m: f64`, `route_m: Option<f64>`, `route_s: Option<f64>`, `no_route: bool`; FFI-`SearchResults` + `ascent_ignored: bool`.
  - `CoreApp::search(&self, text: String, near: LatLon, radius_m: f64, limit: u32, from: Option<RouteFrom>) -> SearchResults` und `CoreApp::search_category(…, from: Option<RouteFrom>) -> Result<SearchResults, CoreError>`. Ungültiges `LegInput` (unbekanntes Profil) → wie `None`.

- [ ] **Step 1: Test erweitern/hinzufügen**
  - `search_results_survive_the_bindings` um die neuen Felder (`route_m: Some(3200.0)`, `route_s: Some(3120.0)`, `no_route: false`, `centre_distance_m: 900.0`, `ascent_ignored: true`) ergänzen.
  - `unknown_profile_searches_without_routes`: `search` mit `LegInput { profile: "rocket", … }` liefert dasselbe wie mit `None`.

- [ ] **Step 2: scheitern lassen, umsetzen, grün**

Run: `cargo test -p core-ffi` → erst FAIL, nach Umsetzung PASS. `cargo clippy -p core-ffi --all-targets -- -D warnings`.

- [ ] **Step 3: Übergabe für den Mac schreiben**: Stand, Befehle (`build-core.sh`, `xcodegen generate`), Task 4 dieses Plans, Messwerte aus Task 1, offene Kleinigkeiten aus den Reviews.

- [ ] **Step 4: Commit**

```bash
git add crates/core-ffi docs/superpowers/plans/2026-11-27-uebergabe-suche-wegdistanz-mac.md
git commit -m "Pass the route start through the search bindings"
```

**Anhalten:** Kern fertig zeigen; Push für den Mac erst nach Rückfrage.

---

### Task 4: Trail — Umschalter, Anzeige, Verdrahtung (Mac)

**Files:**
- Modify: `apps/trail/ZeroGridTrail/Search/SearchController.swift` (Backend-Signatur, `routes`, Neu-Suche, `fit`, `details`, `notes`)
- Modify: `apps/trail/ZeroGridTrail/Search/SearchBridge.swift` (`RouteFrom` durchreichen)
- Modify: `apps/trail/ZeroGridTrail/Search/SearchBar.swift` (Umschalter)
- Modify: `apps/trail/ZeroGridTrail/Plan/PlanController.swift` (`formLeg` lesbar machen als `var newLeg: LegInput`)
- Modify: `apps/trail/ZeroGridTrail/ZeroGridTrailApp.swift` (Quelle für `RouteFrom`)
- Test: `apps/trail/ZeroGridTrailTests/SearchControllerTests.swift`

**Interfaces:**
- Consumes: FFI aus Task 3.
- Produces:
  - `struct SearchBackend { var text: @Sendable (String, LatLon, Double, UInt32, RouteFrom?) -> SearchResults; var category: @Sendable (String, LatLon, Double, UInt32, RouteFrom?) throws -> SearchResults }`
  - `SearchController.init(…, routeFrom: @escaping @MainActor () -> RouteFrom? = { nil }, defaults: UserDefaults = .standard)`; `@Published private(set) var routes: Bool` (Schlüssel `"search.routes"`, Standard `false`); `func toggleRoutes()`.
  - Live-Quelle in `ZeroGridTrailApp`: `location.knowsPosition`, `location.last` → `KnownPosition(lat, lon, ageS: -last.timestamp.timeIntervalSinceNow)`, `plan.newLeg`.

- [x] **Step 1: Tests schreiben** (`SearchControllerTests`; `hit(…)`/`answer(…)` um die neuen Felder ergänzen)
  - `routes_off_passes_no_route_start`: Umschalter aus → Backend bekommt `nil`, `routeFrom`-Quelle wird nicht gefragt.
  - `routes_on_passes_route_start`: an, Quelle liefert `RouteFrom` → Backend bekommt ihn.
  - `toggle_persists`: `toggleRoutes()` schreibt `"search.routes"`; neuer Controller mit denselben Defaults startet an.
  - `toggle_reruns_search`: mit offener Textsuche → zweiter Backend-Aufruf mit dem neuen Zustand.
  - `toggle_without_position_keeps_list`: an, Quelle liefert `nil` → Backend bekommt `nil`, Liste wie vorher; mit gewähltem Treffer bleibt `selected` erhalten.
  - `slow_routed_answer_does_not_replace_newer`: erste Antwort verzögert, zweite schnell → `results` ist die zweite.
  - `fit_uses_centre_distance`: Treffer mit `distanceM` 60 km (von Position), `centreDistanceM` 1 km, Radius 5 km → wird in `fit` einbezogen.
  - `details_show_route_and_air`: `routeM 3200, routeS 3120, distanceM 1100` → `"Unterkunft · 3,20 km · 0:52 h · Luftl. 1,10 km"` (Kategorie `lodging`) (Reihenfolge Art, Ort, Weg, Zeit, Luftlinie; Formatierer aus `Format`); `noRoute` → `"… · 800 m · Umweg oder kein Weg"`; ohne Weg wie heute.
  - `ascent_note`: `ascentIgnored` → `notes` enthält `"Zeiten ohne Höhenmeter."`.

- [x] **Step 2: Tests scheitern** (Xcode, Schema `ZeroGridTrail`, nur `SearchControllerTests`) → FAIL.

- [x] **Step 3: Umsetzen.** `run` holt `RouteFrom` auf dem Main-Actor (nur wenn `routes`) und reicht ihn in die Arbeit im selben Hintergrundjob. Umschalter in `SearchBar.field` zwischen Textfeld und Schließen-Knopf: Text „Weg“ mit Symbol `point.topleft.down.to.point.bottomright.curvepath`, an = Akzentfarbe, Trefferfläche ≥ 44 pt, `accessibilityIdentifier("search-routes")`, `.isSelected` wenn an. Sichtbar, solange die Suche offen ist. OLED-Dunkelmodus prüfen.

- [x] **Step 4: Tests grün**, ganze `ZeroGridTrailTests` grün.

- [x] **Step 5: Von Hand auf dem iPhone**
  - Ein bekannter Umweg-Fall (POI hinter Fluss oder Bahnlinie): Liste ordnet nach Weg, „Luftl.“ zeigt den Unterschied.
  - Stadtsuche zu Fuß (`hike`): Zeiten plausibel, Hinweis „Zeiten ohne Höhenmeter.“
  - Karte weit weg schieben (> 50 km): keine Wege, keine Verzögerung.
  - GPS aus: Liste wie bisher.
  - Gefühlte Wartezeit bei Kategorie „Camping“ im 50-km-Ausschnitt notieren.

- [x] **Step 6: Commit**

```bash
git add apps/trail
git commit -m "Show and sort search hits by the way there in Trail"
```

**Anhalten:** dem Nutzer zeigen; danach PR nach Rückfrage.
