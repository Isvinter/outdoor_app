# iOS I3: Routenplanung offline auf dem iPhone – Bauplan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Auf dem iPhone ohne Netz Routen planen und umplanen, wie am Desktop. Dazu gehören
Wegpunkte, Teilstücke mit eigenem Profil, Etappen, Höhenprofil und Statistik, außerdem
Speichern, Laden, Umbenennen und Löschen.

**Architecture:**
- Der Rust-Kern bekommt eine **Router-Fabrik von außen**. Auf iOS reicht Swift über einen
  UniFFI-Callback (`ValhallaEngine`) den JSON-Text an valhalla-mobile durch. Der Kern baut die
  Anfrage (`costing_json`) und wertet die Antwort aus (`parse_valhalla_route`), über den neuen
  `core_routing::JsonRouter`.
- Die Bearbeitung einer Planung liegt als reine Funktionen auf `PlanRequest` im Kern:
  Punkt setzen, verschieben und entfernen, Etappenende umschalten, letztes Teilstück ändern.
  Die Profilfarben liegen dort ebenfalls, damit Swift keine Planungslogik nachbaut.
- „Speichern“ überschreibt die geladene Route, „Als neue Route speichern“ legt eine neue an.
  Das gilt im Kern, also auf beiden Plattformen.
- Die Swift-App hält nur den aktuellen `PlanRequest` als Oberflächenzustand. Sie rechnet im
  Hintergrund und verwirft veraltete Antworten.

**Tech Stack:**
- Rust 2024: `core-routing`, `core-app`, `core-storage`, `core-ffi` mit UniFFI 0.32.2.
- Tauri-Desktop (TypeScript) für den neuen Knopf.
- Swift 5.9 mit SwiftUI, MapLibre Native 7.0.0, valhalla-mobile **0.6.4** und Swift Charts.
  Mindestversion iOS 16.4.

**Spec:** `docs/superpowers/specs/2026-11-15-ios-app-offline-planung-design.md`. Maßgeblich
sind Ziele 2–3, „Rust-Kern für iOS“ (Router von außen), „Karte auf iOS“ (Route zeichnen,
Profilfarben), „Bedienung auf dem iPhone“ und Meilenstein I3. Baut auf I1 und I2 auf (in
`main`, fc5f979).

**Wo:**
- Task 1–4 laufen auf dem **Linux-Rechner**, auf dem Branch `feature/ios-i3` von `main`.
- Nach Task 4 wird gepusht, nach einmaliger Rückfrage beim Nutzer.
- Task 5–9 laufen auf dem **Mac** auf demselben Branch, nach `git fetch && git switch feature/ios-i3`.
- Vor Task 5 auf dem Mac `apps/ios/scripts/build-core.sh` ausführen, weil sich die
  UniFFI-API ändert.

## Stand nach Task 1–4 (Linux, 2026-11-15) – für die Mac-Sitzung

Task 1–4 sind fertig (Branch `feature/ios-i3`). Vor Task 5 `apps/ios/scripts/build-core.sh`
ausführen, die UniFFI-API ist neu. Bitte so übernehmen:

- **Swift-API** (aus den erzeugten Bindings, zur Probe auf Linux erzeugt):
  - `CoreApp`:
    - `setValhallaEngine(engine:)` und `routeProfiles() -> [ProfileChoice]`
    - `planRoute(request:) throws -> PlanResult` (abseits des Main-Threads aufrufen)
    - `clearPlannedRoute()`: bei weniger als zwei Punkten. Die geladene Route bleibt, „Speichern“ überschreibt sie weiter.
    - `startNewRoute()`: nur für „Leeren“. Danach legt „Speichern“ eine neue Route an.
    - `saveRoute(name:)`, `saveRouteAsNew(name:)`, `savedRoutes()`, `loadRoute(id:)`, `renameRoute(id:name:)` und `deleteRoute(id:)`
    - `loadedRouteId() -> String?`
  - Freie Funktionen:
    - `planEmpty()`
    - `planAddWaypoint(request:at:leg:)` und `planMoveWaypoint(request:index:to:)`
    - `planRemoveWaypoint(request:index:)` und `planToggleStageEnd(request:index:)`
    - `planSetLastLeg(request:leg:)` und `failedLegColor()`
- **`ValhallaEngine`** ist in Swift `protocol ValhallaEngine: AnyObject, Sendable`, also eine
  Klasse.
  - Methoden: `version() -> String` und `route(tilesPath:requestJson:) throws -> String`.
  - Bei einem Fehler `throw EngineFailure.Failed(message:)`. Valhallas Meldungen „No path could
    be found“, „No suitable edges“ und „unreachable“ werden im Kern zu `route_not_found`, alles andere zu
    `routing`.
- **Versionsprüfung:**
  - `version()` liefert `"3.9.1"`. Regionen mit `routing.valhalla` ≠ Engine-Version: das
    Teilstück hat `errorCode == "routing_incompatible"`, die Engine wird gar nicht erst gefragt.
  - Regionen ohne `routing` im Manifest werden nicht geprüft.
- **Fehlgeschlagene Teilstücke** in `failedLegColor()` (`#d93025`) zeichnen, wie der Desktop.
  `PlanLeg.color` ist immer die Profilfarbe. Sie gilt für das Höhenprofil und für
  erfolgreiche Teilstücke.
- **Entfernen eines Punkts** (in Task 2 gegen `route.ts` korrigiert): Das Teilstück davor
  verbindet die Nachbarn. Bei `[hike, mtb, car]` ergibt Entfernen von Punkt 2 `[hike, mtb]`,
  von Punkt 1 `[hike, car]`.
- **Optionen** kommen in fester Reihenfolge zurück (`use_hills`, `use_roads`,
  `avoid_bad_surfaces`, `max_hiking_difficulty`). Es sind nur die gesetzten. Ein unbekannter
  Schlüssel oder ein unbekanntes Profil ergibt bei `planRoute` den Code `route_plan`. Die
  `plan…`-Funktionen geben eine Anfrage, die sie nicht lesen können, unverändert zurück.
- **`PlanResult.elevation`** ist auf rund 1000 Proben ausgedünnt (`for_display`), genug für
  das Diagramm. Der Cursor nimmt `lat`/`lon` der Probe.
- **Speichern:**
  - `saveRoute` überschreibt die zuletzt geladene oder gespeicherte Route (gleiche `id`,
    `rev + 1`).
  - Nach `startNewRoute()` („Leeren“), nach dem Löschen der geladenen Route und beim ersten
    Speichern entsteht eine neue Route. `clearPlannedRoute()` (Rückgängig bis unter zwei Punkte,
    Startpunkt neu setzen) löst die Verbindung **nicht**.
  - Der Kern merkt sich nur die **neueste** Planung: Endet eine ältere `planRoute` nach einer
    neueren oder nach `clearPlannedRoute`/`startNewRoute`/`loadRoute`, gilt sie nicht als
    geplante Route. Ihr Ergebnis bekommt der Aufrufer trotzdem. Swift verwirft es ohnehin
    über seinen eigenen Zähler.
  - `saveRouteAsNew` legt immer eine neue an, und `saveRoute` überschreibt danach diese.
- **Desktop:** Der Knopf „Als neue Route speichern“ ist neu (Command `route_save_new`).
  `RouteRecord` hat jetzt `rev`.
- Valhalla-Engine und Router: Der Kern öffnet je Region einmal einen Router und ruft
  `route(tilesPath:…)` mit dem Pfad der `routing.tar` dieser Region. Die Engine hält je Pfad
  eine valhalla-mobile-Instanz (Task 5).
- `cargo check --target aarch64-apple-ios` geht auf Linux nicht (SQLite braucht `xcrun`). Die
  iOS-Übersetzung prüft erst `build-core.sh` auf dem Mac.

## Stand nach Task 5–9 (Mac, 2026-11-16)

Task 5–9 sind gebaut und im Simulator grün (25 Unit-Tests, 26 UI-Tests). Abweichungen und
Rulings:

- **`ISARWINKEL_PACKAGE`** setzt `apps/ios/scripts/test.sh`, nicht das Schema (kein
  Rechnerpfad im Repo). Auf dem Mac liegt nur die `routing.tar`. Das Skript macht daraus ein
  Paket nur mit Routing. Die Abnahme läuft damit auf leerer Karte und ohne Höhenprofil.
- **Valhalla-Logging** lässt sich über `ValhallaConfig` nicht abstellen (valhalla-mobile gibt
  `logging` nicht weiter). Die `traffic.tar`-Warnung ist abgestellt.
- Die Engine-Klasse heißt `ValhallaMobileEngine`, weil `ValhallaEngine` das Protokoll aus
  den Bindings ist.
- **Tippen:** Ein Tipp gilt nach 0,3 s, wenn kein Doppeltipp folgt. Den Erkenner auf den
  Doppeltipp der Karte warten zu lassen, verschluckte jeden zweiten von schnellen Tipps.
- **Ziehen** eines Wegpunkts beginnt sofort (eigene Geste), nicht erst nach langem Drücken.
- **Wegpunktmenü:** iOS 27 zeigt in diesem Dialog keinen „Abbrechen“-Knopf, Tippen daneben
  schließt ihn.
- **Profil-Cursor:** bleibt nach dem Loslassen stehen, bis sich die Route ändert. Auf dem
  Handy gibt es kein Überfahren, und so lässt sich die Stelle bei eingeklapptem Sheet ansehen.
- **Zahlen:** `Format.distance(12353)` ergibt „12,35 km“ wie `format.ts`, nicht „12,4 km“.
- **Speichern** fragt immer nach dem Namen und schlägt den der geladenen Route oder
  „Profil Distanz“ vor. Nach dem Laden klappt das Sheet ein, und die Karte zeigt die Route.
- `testUndoAndClear` prüft die Zustände; dass „Leeren“ eine neue Route beginnt und
  Rückgängig nicht, prüft `RoutesUITests.testUndoKeepsRouteAndClearStartsNew` über das
  Speichern. `testLoadReplanSave` setzt nach dem Laden einen Punkt dazu; das Verschieben
  nach dem Laden prüft die Abnahme.
- **Abnahme:** Bei Zoom 12 (alle Punkte auf einem Bildschirm) rastete Valhalla an anderen
  Wegen ein (13 001 m). Der Test tippt bei Zoom 14 und schiebt die Karte per Wischgeste von
  Punkt zu Punkt, die Bildschirmstellen liefert `project=` im Status. Ergebnis 12 352 m.
- **Offen: Lauf auf dem iPhone und `plan_ms` dort.** Der Test-Runner lässt sich nicht
  installieren, das kostenlose Profil erlaubt drei Apps (Outdoor, Valhalla-Test, Vario liegen
  darauf). Siehe `docs/MESSPROTOKOLL.md`, „iOS I3: Planung“.

## Global Constraints

- **Kein Schritt der Planung braucht ein Netz.** Kein Rückgriff auf Online-Routing.
- **Keine Planungslogik in Swift.** Punkte setzen, verschieben und entfernen, Etappenenden,
  Statistik, Farben und Speichern kommen aus dem Kern. Swift zeichnet, nimmt Eingaben an und
  reicht den Valhalla-JSON-Text durch.
- valhalla-mobile **0.6.4** (= Valhalla 3.9.1).
  - Die Fabrik meldet `engine_version() = Some("3.9.1")`.
  - Der Kern vergleicht das mit `routing.valhalla` aus dem Manifest. Weichen die Versionen
    ab, schlägt das Teilstück mit dem Code `routing_incompatible` fehl, und die Karte bleibt
    nutzbar.
  - Ein Manifest ohne `routing` wird nicht geprüft (ältere Pakete).
- Bearbeitungsregeln **genau wie** `apps/desktop/ui/src/route.ts`:
  - Ein neuer Punkt hängt ein Teilstück mit dem Profil aus dem Formular an.
  - Beim Entfernen eines Punkts verbindet das Teilstück davor die Nachbarn. Der erste Punkt
    nimmt sein Teilstück mit.
  - Ein Etappenende am entfernten Punkt entfällt. Die übrigen Indizes rücken nach.
  - Etappenenden sind nur innere Punkte, aufsteigend.
  - Das Formular gilt für das letzte Teilstück und für alle neuen.
  - „Rückgängig“ entfernt den letzten Punkt.
- Profilfarben wie in `apps/desktop/ui/src/profile-colors.ts`:
  - `road_bike #1565c0`, `trekking #00897b`, `mtb #e07b00`, `hike #c62828`, `car #546e7a`,
    `manual #6a1b9a`.
  - Für fehlgeschlagene Teilstücke `#d93025`.
- Route zeichnen wie `apps/desktop/ui/src/map.ts` (`addRouteLayers`):
  - Rand weiß, Breite 8, Deckkraft 0,95. Linie in Profilfarbe, Breite 4,5, `round` für Ende
    und Ecken.
  - Ein fehlgeschlagenes Teilstück ist eine gerade Linie zwischen seinen Wegpunkten:
    Farbe `failedLegColor()` (`#d93025`, wie `route.ts:255`), Breite 3, gestrichelt `[2, 2]`.
    *(Korrigiert nach Task 4: vorher stand hier „Profilfarbe“.)*
  - Wegpunkte sind nummerierte Kreise mit 24 pt Durchmesser, Farbe `#0b63ce`, weißer Rand
    2 pt. Ein Etappenende ist ein abgerundetes Quadrat in `#1d1d1f`.
  - Der Profil-Cursor ist ein weißer Kreis von 12 pt mit Rand `#0b63ce` von 3 pt.
- Texte auf Deutsch. Die Fehlercodes des Kerns werden auf iOS übersetzt, mindestens
  `route_not_found`, `routing_unavailable`, `routing_incompatible`, `route_too_few_waypoints`
  und `routing`.
- Selbsttests treiben **echte Eingaben** (Tippen und Ziehen an Bildschirmstellen) und lesen den
  Zustand aus dem unsichtbaren Element `map-status` (Muster aus I1). Keine internen Aufrufe.
- Push erst nach dem Meilenstein (und nach Task 4 für die Übergabe), jeweils einmal fragen.

## Review Focus

1. **Stale Antwort:** Der Nutzer setzt schnell hintereinander Punkte. Eine langsamere, ältere
   Berechnung darf die neuere nicht überschreiben. → Task 6 (`testFastTapsKeepNewestPlan`).
2. **Versionskonflikt:** Die Region wurde mit einer anderen Valhalla-Version gebaut. Die Karte
   geht, das Teilstück schlägt mit `routing_incompatible` fehl, kein Absturz. → Task 1
   (`incompatible_engine_fails_leg`).
3. **Gelöschte geladene Route:** Erst laden, dann löschen, dann „Speichern“. Das legt eine
   neue Route an und schlägt nicht fehl. → Task 3 (`save_after_deleting_loaded_creates_new`).
4. **Punkt außerhalb der Routingdaten:** Das Teilstück ist gestrichelt mit einer Meldung, die
   anderen Teilstücke bleiben, und die Statistik zählt es nicht mit. → Task 4
   (`leg_outside_region_fails_alone`) und Task 6 (UI-Test mit `failed_legs=1`).
5. **Gleichzeitige Valhalla-Aufrufe:** Zwei Berechnungen laufen zugleich, etwa beim Laden
   während einer Planung. Die Swift-Engine serialisiert je Instanz und stürzt nicht ab.
   → Task 5 (`testConcurrentRoutesAreSerialised`).

---

### Task 1 (Linux): Router-Fabrik von außen, `JsonRouter`, Versionsprüfung

**Files:**
- Modify: `crates/core-routing/src/valhalla_api.rs` (`engine_error`, öffentlich), `crates/core-routing/src/valhalla_router.rs` (nutzt `engine_error`), `crates/core-routing/src/model.rs` (`RoutingError::Incompatible`), `crates/core-routing/src/lib.rs`
- Create: `crates/core-routing/src/json_router.rs`
- Modify: `crates/core-app/src/routes.rs` (`RouterFactory`, `Routing`), `crates/core-app/src/plan.rs` (`leg`), `crates/core-app/src/error.rs` (Code), `crates/core-app/src/lib.rs`
- Test: `crates/core-routing/tests/json_router.rs`, `crates/core-app/tests/router_factory.rs`

**Interfaces:**
- Produces (core-routing):
  - `pub trait JsonEngine: Send + Sync { fn route_json(&self, request: &str) -> Result<String, String>; }`
  - `pub struct JsonRouter` mit `JsonRouter::new(engine: Arc<dyn JsonEngine>) -> Self`, implementiert `Router`. Die Profile sind dieselben wie bei `ValhallaRouter`.
  - `pub fn engine_error(message: &str) -> RoutingError`: liefert `NoRoute`, wenn `is_no_route`, sonst `Engine`.
  - `RoutingError::Incompatible { data: String, engine: String }` mit der Meldung `routing data built with Valhalla {data}, this app has {engine}`.
- Produces (core-app):
  - `pub trait RouterFactory: Send + Sync { fn open(&self, tiles: &Path) -> Result<Arc<dyn Router>, RoutingError>; fn engine_version(&self) -> Option<String>; }`
  - `App::set_router_factory(&self, factory: Arc<dyn RouterFactory>)`: schließt alle offenen Router und leert die zwischengespeicherten Teilstücke.
  - Ohne gesetzte Fabrik gilt mit Feature `valhalla` die eingebaute (`ValhallaRouter::open`, `engine_version() = None`), ohne Feature keine.
  - `route_profiles()[..].available` ist `profile == Manual || eine Fabrik vorhanden`.
  - `AppError::code()` liefert für `Incompatible` den Code `routing_incompatible`.

- [ ] **Step 1: Tests schreiben.**
  - `json_router.rs`:
    - `routes_through_engine`: Ein Fake-Engine merkt sich die Anfrage und antwortet mit `trip`-JSON wie in `tests/routing.rs:244`. Dann gilt Anfrage == `costing_json(&request)`, die Distanz stimmt, die Linie ist dekodiert.
    - `engine_error_maps_no_route`: `Err("No path could be found")` ergibt `RoutingError::NoRoute`, `Err("boom")` ergibt `Engine`.
  - `router_factory.rs`, mit der Region `ramp` samt `routing.tar`-Attrappe (`common::app_with_ramp_routing`). Für die Versionsprüfung bekommt `ramp_region` einen Parameter für `routing` im Manifest:
    - `factory_is_used_for_routed_legs`: Die Fabrik zählt `open`, Wandern über zwei Punkte geht, `open` == 1 auch nach zwei Plänen.
    - `incompatible_engine_fails_leg`: Manifest `routing.valhalla = "3.9.1"`, Fabrik `Some("3.8.0")`. Das Teilstück hat `error.code == "routing_incompatible"`, `plan_route` ist `Ok`.
    - `no_factory_no_routing`: `#[cfg(not(feature = "valhalla"))]`, `core-app` hat das Feature standardmäßig nicht. Wandern ergibt `routing_unavailable`, `available == false` außer bei `manual`.
- [ ] **Step 2:** `cargo test -p core-routing --test json_router` und `cargo test -p core-app --test router_factory` ausführen. Erwartet: FAIL (Typen fehlen).
- [ ] **Step 3:** Umsetzen. Die Versionsprüfung kommt in `App::leg`, vor dem Öffnen des Routers, mit `tiles.info.as_ref().map(|i| &i.valhalla)` gegen `factory.engine_version()`. `Routing::router` bekommt die Fabrik statt `open_router`. `set_router_for_tests` bleibt.
- [ ] **Step 4:** `cargo test --workspace` und `cargo clippy --workspace --all-targets` ausführen. Erwartet: grün. Zusätzlich `cargo check -p core-app --target aarch64-apple-ios --no-default-features`, falls das Target installiert ist, sonst über CI.
- [ ] **Step 5:** Commit `core: router factory from outside, JsonRouter, Valhalla version check`.

### Task 2 (Linux): Bearbeitung einer Planung und Profilfarben im Kern

**Files:**
- Modify: `crates/core-app/src/plan.rs` (`impl PlanRequest`), `crates/core-routing/src/model.rs` (`Profile::color`, `FAILED_LEG_COLOR`)
- Test: `crates/core-app/tests/plan_edit.rs`, `crates/core-routing/tests/routing.rs` (Farben)

**Interfaces:**
- Produces, alle Funktionen geben `PlanRequest` zurück und ändern `self` nicht. Die Regeln stehen in den Global Constraints.
  - `PlanRequest::empty() -> Self`
  - `fn add_waypoint(&self, at: LatLon, leg: LegSpec) -> Self`
  - `fn move_waypoint(&self, index: usize, to: LatLon) -> Self`
  - `fn remove_waypoint(&self, index: usize) -> Self`, außerhalb des Bereichs unverändert
  - `fn toggle_stage_end(&self, index: usize) -> Self`, nur innere Punkte
  - `fn set_last_leg(&self, leg: LegSpec) -> Self`, ohne Teilstücke unverändert
- `Profile::color(self) -> &'static str`, `pub const FAILED_LEG_COLOR: &str = "#d93025"`.

- [ ] **Step 1: Tests schreiben** (`plan_edit.rs`):
  - `add_appends_leg_from_form`: Der 1. Punkt bringt kein Teilstück, der 2. und 3. Punkt bringen je eines.
  - `remove_inner_joins_with_leg_before`: 4 Punkte mit Profilen `[hike, mtb, car]`. Entfernen von Index 2 ergibt `[hike, mtb]` (das Teilstück davor verbindet die Nachbarn), Index 1 ergibt `[hike, car]`, Index 3 (der letzte) `[hike, mtb]`, Index 0 `[mtb, car]`. *(Korrigiert in Task 2, nach `route.ts`.)*
  - `remove_shifts_stage_ends`: Etappenenden `[1, 3]` bei 5 Punkten. Entfernen von 1 ergibt `[2]`, Entfernen von 2 ergibt `[1, 2]`.
  - `stage_end_only_inner`: `toggle_stage_end(0)` und `toggle_stage_end(n-1)` ändern nichts. Zweimal umschalten stellt den alten Zustand her. Die Liste bleibt sortiert.
  - `stage_ends_dropped_when_point_becomes_last`: Punkte 0..=2, Etappenende `[1]`. Entfernen von 2 ergibt `[]`.
  - `set_last_leg_changes_only_last`.
  - Jedes Ergebnis mit ≥ 2 Punkten besteht `PlanRequest::validate()`.
  - In `routing.rs`: `colors_match_desktop` liest `apps/desktop/ui/src/profile-colors.ts` per `include_str!` und prüft jede `Profile::ALL`-Farbe und `FAILED_LEG_COLOR` darin.
- [ ] **Step 2:** `cargo test -p core-app --test plan_edit` ausführen. Erwartet: FAIL.
- [ ] **Step 3:** Umsetzen. Das private `check` in `plan.rs` wird zu `pub fn PlanRequest::validate(&self) -> Result<(), AppError>`, und `plan_route` ruft es auf. Den Index für das Entfernen genau wie `route.ts:137-143` bestimmen.
- [ ] **Step 4:** `cargo test --workspace` ausführen. Erwartet: grün.
- [ ] **Step 5:** Commit `core: editing a route plan, profile colours in the core`.

Ruling, in den Plan übernommen: Der Desktop behält seine TypeScript-Fassung derselben Regeln
(`route.ts`). Ein Umbau auf den Kern brächte bei jedem Klick einen Tauri-Aufruf, ohne Gewinn
für das Ziel. Die Tests in `plan_edit.rs` legen die Regeln fest, an denen sich beide messen.

### Task 3 (Linux): „Speichern“ überschreibt, „Als neue Route speichern“, Desktop-Knopf

**Files:**
- Modify: `crates/core-storage/src/storage.rs` (`replace_route`, `RouteRecord.rev`), `crates/core-app/src/routes.rs`, `apps/desktop/src-tauri/src/commands.rs` und `lib.rs` (Command registrieren), `apps/desktop/ui/src/api.ts`, `apps/desktop/ui/src/route.ts`, `apps/desktop/ui/index.html`, `apps/desktop/ui/src/i18n/de.ts`
- Test: `crates/core-storage/tests/` (bestehende Routen-Tests ergänzen), `crates/core-app/tests/routes.rs`

**Interfaces:**
- `Storage::replace_route(&mut self, id: Uuid, name: &str, profile: &str, distance_m: f64, ascent_m: Option<f64>, plan_json: &str, now_ms: i64) -> Result<(), StorageError>`: Sie setzt `updated_at` und `rev = rev + 1`. Bei einer gelöschten oder fehlenden Route liefert sie `NotFound`.
- `RouteRecord` bekommt `pub rev: i64`, also auch `list_routes`.
- `core-app`:
  - `Routing` merkt sich `loaded: Option<Uuid>`.
  - `save_planned_route(name)`: Ist eine Route geladen und noch vorhanden, wird sie ersetzt, sonst entsteht eine neue, die danach als geladen gilt. Ergebnis `RouteRecord`.
  - `save_planned_route_as_new(name) -> Result<RouteRecord, AppError>`: legt immer eine neue Route an, die danach als geladen gilt.
  - `saved_route(id)` setzt `loaded = id`.
  - `clear_planned_route()` setzt `loaded = None`.
  - `delete_route(id)` setzt `loaded = None`, wenn `id` geladen war.
  - `loaded_route(&self) -> Option<Uuid>`.
- Tauri: `route_save_new(name)` neben `route_save`. Der Knopf `route-save-new` mit dem Text „Als neue Route speichern“ steht neben „Speichern“. Nach dem Speichern zeigt der Desktop „Route gespeichert“ und aktualisiert die Liste.

- [ ] **Step 1: Tests schreiben** (`core-app/tests/routes.rs`):
  - `save_twice_overwrites`: Planen, speichern, umplanen, speichern ergibt eine Route mit derselben `id`, `rev` um 1 höher und neuer Distanz.
  - `save_loaded_overwrites`: Laden, umplanen und speichern ersetzt diese `id`.
  - `save_as_new_keeps_old`: Danach gibt es zwei Routen, die alte ist unverändert, und `loaded_route()` ist die neue.
  - `clear_then_save_creates_new`.
  - `save_after_deleting_loaded_creates_new`.
  - In Storage: `replace_route_bumps_rev` und `replace_deleted_route_is_not_found`.
- [ ] **Step 2:** `cargo test -p core-app --test routes` ausführen. Erwartet: FAIL.
- [ ] **Step 3:** Kern umsetzen, dann den Desktop. Aufräumen durch `route-clear` ruft schon `route_clear` auf, die geladene Route wird also vergessen.
- [ ] **Step 4:** `cargo test --workspace`, `pnpm -C apps/desktop/ui build` und `cargo build -p outdoor-desktop` ausführen. Erwartet: grün. Kurz von Hand prüfen: zweimal speichern ergibt einen Eintrag, „Als neue“ ergibt zwei.
- [ ] **Step 5:** Commit `routes: save overwrites the loaded route, save as new`.

### Task 4 (Linux): Planung in `core-ffi`

**Files:**
- Modify: `crates/core-ffi/src/lib.rs` (oder aufteilen: `src/plan.rs` für Typen und Funktionen)
- Test: `crates/core-ffi/tests/plan.rs`

**Interfaces** (UniFFI, Swift-Namen in camelCase):
- Foreign Trait `ValhallaEngine: Send + Sync`:
  - `fn version(&self) -> String`
  - `fn route(&self, tiles_path: String, request_json: String) -> Result<String, EngineFailure>`
  - Dazu `#[derive(uniffi::Error)] enum EngineFailure { Failed { message: String } }`.
- `CoreApp::set_valhalla_engine(engine: Arc<dyn ValhallaEngine>)`: Ein Adapter `RouterFactory` legt je `routing.tar` einen `JsonRouter` an, dessen `JsonEngine` `engine.route(path, json)` aufruft. `engine_version = Some(engine.version())`.
- Records:
  - `LatLon { lat, lon }`
  - `OptionValue { key: String, value: f64 }`
  - `LegInput { profile: String, options: Vec<OptionValue> }`
  - `PlanInput { waypoints: Vec<LatLon>, legs: Vec<LegInput>, stage_ends: Vec<u32> }`
  - `OptionInfo { key, min, max, step, default }`
  - `ProfileChoice { profile: String, color: String, available: bool, options: Vec<OptionInfo> }`
  - `PlanLeg { profile, color, line: Vec<LatLon>, distance_m, ascent_m: Option<f64>, descent_m: Option<f64>, time_s: Option<f64>, error_code: Option<String>, error_message: Option<String> }`
  - `ElevationSample { distance_m, lat, lon, ele: Option<f32>, leg: u32 }`
  - `StageSummary { from: u32, to: u32, distance_m, ascent_m, descent_m, time_s, colors: Vec<String> }`
  - `PlanTotals { distance_m, ascent_m, descent_m, time_s, min_ele_m, max_ele_m, failed_legs: u32 }`
  - `PlanResult { request: PlanInput, legs: Vec<PlanLeg>, elevation: Vec<ElevationSample>, stages: Vec<StageSummary>, total: PlanTotals }`
  - `SavedRoute { id: String, name, profile, distance_m, ascent_m: Option<f64>, updated_at_ms: i64, rev: i64 }`
- `CoreApp`-Methoden:
  - `route_profiles() -> Vec<ProfileChoice>`
  - `plan_route(request: PlanInput) -> Result<PlanResult, CoreError>`, mit `for_display(1000)`
  - `clear_planned_route()`
  - `save_route(name) -> Result<SavedRoute, CoreError>`
  - `save_route_as_new(name) -> Result<SavedRoute, CoreError>`
  - `saved_routes() -> Result<Vec<SavedRoute>, CoreError>`
  - `load_route(id) -> Result<PlanResult, CoreError>`
  - `rename_route(id, name) -> Result<(), CoreError>`
  - `delete_route(id) -> Result<(), CoreError>`
  - `loaded_route_id() -> Option<String>`
- Freie Funktionen, die je eine Methode aus Task 2 aufrufen:
  - `plan_empty() -> PlanInput`
  - `plan_add_waypoint(request, at, leg)`
  - `plan_move_waypoint(request, index: u32, to)`
  - `plan_remove_waypoint(request, index: u32)`
  - `plan_toggle_stage_end(request, index: u32)`
  - `plan_set_last_leg(request, leg)`
  - `failed_leg_color() -> String`
- Ein unbekanntes Profil oder ein unbekannter Optionsschlüssel liefert `CoreError` mit dem Code `route_plan`. Eine falsche UUID liefert den Code `route_id`.

- [ ] **Step 1: Tests schreiben** (`core-ffi/tests/plan.rs`, Fixture `mini`, Fake-Engine in Rust):
  - `manual_plan_without_engine`: Drei Punkte in `mini` mit `manual` ergeben zwei Teilstücke, Distanz > 0 und Höhenproben mit `ele`.
  - `engine_receives_costing_json`: Kopie von `mini` mit einer `routing.tar`-Attrappe und `routing.valhalla = "3.9.1"` im Manifest, Fake-Engine `version "3.9.1"`. Die Engine bekommt den Pfad der `routing.tar` und ein JSON mit `"costing":"pedestrian"`. Ihre `trip`-Antwort landet als Linie im `PlanLeg`.
  - `leg_outside_region_fails_alone`: Punkt 3 liegt weit außerhalb. Teilstück 2 hat `error_code == "routing_unavailable"` (oder `route_not_found`), `failed_legs == 1`, und Teilstück 1 ist unverändert.
  - `edit_functions_roundtrip`: `plan_add_waypoint` dreimal, `plan_toggle_stage_end(1)` und `plan_remove_waypoint(1)` ergeben `stage_ends == []` und 2 Punkte.
  - `save_load_overwrite_via_ffi`: `save_route`, `load_route` und erneut `save_route` ergeben gleiche `id` und `rev + 1`.
  - `profiles_have_colors`: `hike` ist `#c62828`.
- [ ] **Step 2:** `cargo test -p core-ffi --test plan` ausführen. Erwartet: FAIL.
- [ ] **Step 3:** Umsetzen. Nur Typumwandlung, keine Logik. Die Optionen gehen über `RouteOptions` per serde (`OptionValue`-Liste → JSON-Objekt → `RouteOptions`).
- [ ] **Step 4:**
  - `cargo test --workspace` und clippy ausführen. Erwartet: grün.
  - `cargo build -p core-ffi --release`, dann `uniffi-bindgen generate` wie im I1-Plan mit der `.so` oder `.a` von Linux, nur zur Probe. Erwartet: Swift-Datei mit `protocol ValhallaEngine`.
- [ ] **Step 5:**
  - Commit `core-ffi: route planning for the apps`.
  - Den Abschnitt „Stand nach Task 1–4“ in diesen Plan schreiben, mit Rulings und Abweichungen für die Mac-Sitzung, und committen.
  - **Den Nutzer fragen, ob `feature/ios-i3` gepusht werden darf.**

### Task 5 (Mac): valhalla-mobile als `ValhallaEngine`

**Files:**
- Modify: `apps/ios/project.yml` (Paket `ValhallaMobile`, `exactVersion: 0.6.4`, Produkt `Valhalla`), `apps/ios/Outdoor/AppStart.swift` (Engine setzen)
- Create: `apps/ios/Outdoor/Core/ValhallaEngine.swift`
- Test: `apps/ios/OutdoorTests/ValhallaEngineTests.swift`

- [x] **Step 1: Tests schreiben.**
  - `testRoutesReferenceHike`: Läuft nur, wenn `ISARWINKEL_PACKAGE` (Umgebungsvariable des Test-Schemas, Pfad zu `isarwinkel-achensee` auf dem Mac) gesetzt ist, sonst `XCTSkip`.
    - Region importieren, Engine setzen, `planRoute` über die drei Referenzpunkte (siehe Spike: 47.6803/11.5741, 47.6625/11.5233, 47.645/11.556) mit `hike` und `max_hiking_difficulty 3`, `use_hills 0.8`.
    - Die Distanz ist 12 353 m ± 1 %, `failedLegs == 0`.
  - `testConcurrentRoutesAreSerialised`: 8 parallele `planRoute`-Aufrufe derselben Region ergeben 8 gleiche Distanzen, ohne Absturz.
  - `testEngineFailureIsReported`: Ein Pfad ohne Datei ergibt `CoreError` beziehungsweise ein Teilstück mit `errorCode`, kein Absturz.
- [x] **Step 2:** Tests im Simulator ausführen. Erwartet: FAIL, weil es die Engine noch nicht gibt.
- [x] **Step 3:** `ValhallaEngine` umsetzen:
  - Eine Instanz von `Valhalla(ValhallaConfig(tileExtractTar:))` je `tilesPath`, einmal angelegt und wiederverwendet.
  - Je Instanz eine `NSLock` um `route(rawRequest:)`.
  - `version()` liefert `"3.9.1"`.
  - Logging und `traffic.tar` in der Konfiguration abstellen. Geht das mit `ValhallaConfig` nicht, als Ruling festhalten.
  - In `AppStart` nach `CoreApp.open` `setValhallaEngine` aufrufen.
- [x] **Step 4:** Tests ausführen. Erwartet: grün, der Referenztest mit gesetzter Variable ebenfalls.
- [x] **Step 5:** Commit `ios: route with valhalla-mobile through the core`.

### Task 6 (Mac): Wegpunkte und Route auf der Karte

**Files:**
- Create: `apps/ios/Outdoor/Plan/PlanController.swift` (ObservableObject: `request: PlanInput`, `plan: PlanResult?`, `profile`, `options`, `picking`), `apps/ios/Outdoor/Plan/RouteLayers.swift` (Quellen und Ebenen in MapLibre), `apps/ios/Outdoor/Plan/WaypointViews.swift`
- Modify: `apps/ios/Outdoor/Map/MapView.swift`, `MapScreen.swift` (Status um Planungswerte), `MapToolbar.swift`
- Test: `apps/ios/OutdoorUITests/PlanUITests.swift`

Bedienung wie im Entwurf:
- „+ Punkt“ schaltet den Planmodus um, nur dann setzt Tippen einen Punkt.
- Ziehen verschiebt einen Wegpunkt.
- Tippen auf einen Wegpunkt öffnet ein Menü mit „Etappenende hier“ (nur innere Punkte) und „Entfernen“.
- Es gibt die Knöpfe „Rückgängig“ (↶) und „Leeren“ (⌫).
- Rechnen geschieht in einer `Task` abseits des Main-Threads, mit Zähler. Nur die neueste Antwort wird übernommen.
- Bei einem Stilwechsel bleiben die Ebenen der Route erhalten (neu anlegen nach `didFinishLoading`).

Der Status `map-status` bekommt diese Felder:
- `waypoints=<n>` und `legs=<n>`
- `failed_legs=<n>` und `distance_m=<n>`
- `stage_ends=<i,j>`
- `planning=<0|1>` (eine Berechnung läuft)
- `route_colors=<#..,#..>` (gezeichnete Teilstücke)

- [x] **Step 1: UI-Tests schreiben.** Fixture `mini`, Profil `manual`, damit kein Routing nötig ist, Start mit `-startZoom 14`.
  - `testTapsSetWaypoints`: Planmodus an, drei Tipps an verschiedenen Bildschirmstellen ergeben `waypoints=3 legs=2`, `distance_m>0` und `route_colors` zweimal `#6a1b9a`.
  - `testTapOutsidePlanModeSetsNothing`.
  - `testDragMovesWaypoint`: Zieht man Punkt 2 um 80 pt, ändert sich `distance_m`, `waypoints` bleibt 3.
  - `testWaypointMenu`: Tippen auf Punkt 2 und „Etappenende hier“ ergibt `stage_ends=1`. „Entfernen“ ergibt `waypoints=2`.
  - `testUndoAndClear`: Nach „Leeren“ ruft die App `startNewRoute()` auf. Bei Rückgängig unter zwei Punkte ruft sie nur `clearPlannedRoute()` auf.
  - `testFastTapsKeepNewestPlan`: fünf Tipps ohne Warten. Wenn `planning=0`, gilt `waypoints=5` und `legs=4`.
  - Mit Profil `hike` und `mini` ohne Routingdaten: `failed_legs=1`, das Teilstück ist gestrichelt, und die Meldung „Keine Routingdaten …“ ist sichtbar.
- [x] **Step 2:** Ausführen. Erwartet: FAIL.
- [x] **Step 3:** Umsetzen:
  - Wegpunkte als `MLNPointAnnotation`-Views oder als Symbolebene. Für Ziehen und Menü sind Annotation-Views naheliegend, das ist die Wahl des Bauenden.
  - Linien als `MLNShapeSource` mit den Eigenschaften `color` und `failed`, Ebenen wie in den Global Constraints.
  - Farben aus `PlanLeg.color` und `failedLegColor()`.
- [x] **Step 4:** Ausführen. Erwartet: grün.
- [x] **Step 5:** Commit `ios: waypoints and route legs on the map`.

### Task 7 (Mac): Sheet mit Profilwahl, Einstellungen, Höhenprofil und Etappen

**Files:**
- Create: `apps/ios/Outdoor/Plan/PlanSheet.swift`, `ProfileChart.swift`, `ProfilePicker.swift`, `Format.swift` (Zahlen wie `apps/desktop/ui/src/format.ts`, deutsch)
- Modify: `PlanController.swift`, `MapScreen.swift`, `RouteLayers.swift` (Cursor)
- Test: `apps/ios/OutdoorUITests/PlanUITests.swift`, `apps/ios/OutdoorTests/FormatTests.swift`

Das Sheet hat drei Stufen, wie im Entwurf skizziert. Mindesthöhe iOS 16.4, also
`.presentationDetents`.
- Eingeklappt: Distanz, Aufstieg und Zeit.
- Halb:
  - Sportart-Menü in der Reihenfolge `hike, mtb, trekking, road_bike, car, manual`. Nicht
    verfügbare Profile sind ausgegraut.
  - Regler aus `ProfileChoice.options`.
  - Das Höhenprofil und die Etappenliste, sobald es mehrere Etappen gibt.
- Ganz: das Höhenprofil groß.

Das Profil ist nach Teilstück gefärbt, mit senkrechten Linien an den Etappenenden. Ein Finger
auf dem Profil setzt den Cursor auf der Karte.

- [x] **Step 1: Tests schreiben.**
  - `FormatTests`: `12353 m` ergibt „12,4 km“, `850 m` ergibt „850 m“, `3,6 h` ergibt „3:36 h“. Die genauen Werte kommen aus `format.ts`, dort nachsehen und übernehmen.
  - UI-Tests:
    - `testProfileAppliesToLastLeg`: Drei Punkte mit `manual`, dann „Wandern“ wählen. Mit `mini` (ohne Routingdaten) schlägt das Wander-Teilstück fehl: `failed_legs=1`, `route_colors` ergibt `#6a1b9a,#d93025`. Das Profil steht trotzdem im Request (`legs` zeigt `manual,hike`, eigenes Status-Feld `leg_profiles=`).
    - `testSheetShowsTotals`: Die eingeklappte Stufe zeigt die Distanz aus `distance_m`, formatiert.
    - `testChartCursor`: Ziehen auf dem Profil ergibt den Status `cursor=<lat>,<lon>` innerhalb der Grenzen von `mini`.
- [x] **Step 2:** Ausführen. Erwartet: FAIL.
- [x] **Step 3:** Umsetzen. Das Profil wahlweise mit Swift Charts (`LineMark` je Teilstück, `RuleMark` für Etappen) zeichnen.
- [x] **Step 4:** Ausführen. Erwartet: grün.
- [x] **Step 5:** Commit `ios: plan sheet with profile choice, elevation chart and stages`.

### Task 8 (Mac): Routen speichern, laden, umbenennen, löschen

**Files:**
- Create: `apps/ios/Outdoor/Plan/RoutesList.swift`
- Modify: `PlanSheet.swift`, `PlanController.swift`, `MapScreen.swift` (Status `route_id=`, `route_rev=`, `routes=<n>`)
- Test: `apps/ios/OutdoorUITests/RoutesUITests.swift`

Die Liste „Routen“ steht im ganz ausgezogenen Sheet. Ein Eintrag zeigt Name, Distanz und
Aufstieg. Tippen lädt die Route, Wischen bietet „Umbenennen“ und „Löschen“. Die Knöpfe heißen
„Speichern“ und „Als neue Route speichern“. Ohne eigenen Namen schlägt der Dialog einen vor,
wie am Desktop (`route.default_name`, Profilnamen und Distanz).

- [x] **Step 1: UI-Tests schreiben** (Fixture `mini`, `manual`):
  - `testSaveTwiceKeepsOneRoute`: Zwei Punkte setzen und speichern, dann einen dritten setzen und speichern. Ergebnis: `routes=1`, `route_rev` um 1 höher.
  - `testSaveAsNew`: Ergebnis `routes=2`.
  - `testLoadReplanSave`: Leeren, Route 1 laden (`waypoints` wie gespeichert), Punkt verschieben und speichern. Ergebnis: dieselbe `route_id`, `routes` unverändert.
  - `testRenameAndDelete`: Die geladene Route löschen und dann speichern ergibt eine neue Route.
- [x] **Step 2:** Ausführen. Erwartet: FAIL.
- [x] **Step 3:** Umsetzen.
- [x] **Step 4:** Ausführen. Erwartet: grün.
- [x] **Step 5:** Commit `ios: save, load, rename and delete routes`.

### Task 9 (Mac): Abnahme mit Referenzroute, Messwerte, Doku

**Files:**
- Test: `apps/ios/OutdoorUITests/AcceptanceUITests.swift`
- Modify: `docs/MESSPROTOKOLL.md` (Abschnitt „iOS I3: Planung“), `PLAN.md` (8.1, Stand I3), `README.md` (Bedienung auf dem iPhone)

- [x] **Step 1: Abnahmetest schreiben.** `testReferenceHikeByTapping` braucht `ISARWINKEL_PACKAGE`, sonst `XCTSkip`. `-importFixture` zeigt auf dieses Paket.
  - Profil „Wandern“ wählen, mit Standardeinstellungen wie im Spike: `max_hiking_difficulty 3`, `use_hills 0.8`. Weichen die Standardwerte ab, die Regler im Test setzen.
  - Den Planmodus einschalten und die drei Referenzpunkte an ihrer **Bildschirmstelle** antippen. Die Umrechnung von Koordinate in Bildschirmpunkt liefert der Status (`project=<lat>,<lon>→<x>,<y>` für die drei Punkte, oder Start mit `-startCenter` und `-startZoom 13`, dann rechnet der Test Web Mercator selbst). Die Wahl des Bauenden.
  - Ergebnis: `distance_m` liegt in 12 353 ± 124, `failed_legs=0`.
  - Danach speichern, leeren, laden, Punkt 2 verschieben und speichern. Ergebnis: `routes=1`, dieselbe `route_id`.
- [ ] **Step 2:** Im Simulator und auf dem iPhone ausführen (`ISARWINKEL_PACKAGE` im Schema setzen). Erwartet: grün. *(Simulator grün; iPhone offen, siehe „Stand nach Task 5–9“.)*
- [x] **Step 3:** Messwerte in `docs/MESSPROTOKOLL.md`:
  - Dauer je neues Teilstück auf dem iPhone, vom Tippen bis zur gezeichneten Route. Mit Median über 10 Teilstücke als Status-Feld `plan_ms` aus dem Selbsttest.
  - App-Größe mit valhalla-mobile.
  - PLAN.md 8.1 und README nachziehen.
- [x] **Step 4:** Commit `ios: I3 done – route planning on the iPhone`. **Anhalten**, dem Nutzer zeigen und den Push erfragen. Er plant im Flugmodus von Hand.

## Danach

- Gesamtprüfung des Branches durch einen frischen Gutachter, auf Linux oder dem Mac, je nachdem
  wo die Sitzung läuft. Danach Merge nach `main` (nach Rückfrage).
- **I4** (Freigabe im Heimnetz, Übernahme per QR) bekommt einen eigenen Plan. Dafür sind
  wichtig: `RouteRecord.rev` (aus Task 3) und `synced_rev`. Für die Regionen-Übersicht braucht
  es einen Weg ohne Katalogabfrage, weil `App::regions()` den Katalog abfragt, sobald eine
  Quelle gesetzt ist.
