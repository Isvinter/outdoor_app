# Trail: Aufzeichnung und Live-Routing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Trail zeichnet Touren auf, auch bei gesperrtem Bildschirm, und führt auf einer geladenen Route mit einem Weg zurück zum nächsten Wegpunkt, wenn der Nutzer abkommt.

**Architecture:** Die Führung (Fortschritt, Abweichung, Weg zurück, Restangaben) ist ein reines Rust-Modul in `core-app` und wird unter Linux mit abgespielten Tracks getestet. `App` verbindet es mit Positionen, geplanter Route und Aufzeichnung. `core-ffi` reicht Aufzeichnung, Tracks und Führung an Swift weiter. Trail bekommt einen gemeinsamen Ortungsdienst und zeigt nur an.

**Tech Stack:** Rust, UniFFI, Valhalla (valhalla-mobile), SwiftUI, MapLibre Native, CoreLocation, UserNotifications.

**Spec:** `docs/superpowers/specs/2026-11-24-aufzeichnung-live-routing-design.md`

## Global Constraints

- Branch `feature/recording-live-routing`. Am Ende PR, Merge lokal mit `--no-ff`. Push nur nach Rückfrage.
- **Anhalten und dem Nutzer zeigen:** nach Task 5 (Kern fertig, Linux), nach Task 8 (Meilenstein 1 auf dem Mac), nach Task 11 (Meilenstein 2).
- Die Führung verändert die geplante Route nie: Wegpunkte, Reihenfolge, Teilstücke, Speicherstand bleiben, wie sie sind.
- Keine Netzwerkaufrufe. Es gibt keine gerade Linie als Ersatz für einen gescheiterten Weg zurück.
- Eine Mitteilung je Abweichung, nicht mehr.
- GPS läuft nur auf Anforderung: Standortknopf, Aufzeichnung oder Führung. Hintergrund-Ortung nur bei Aufzeichnung oder Führung.
- Ortung „Standard“: `kCLLocationAccuracyBest`, `kCLDistanceFilterNone`, `pausesLocationUpdatesAutomatically = false`. Abstand der gespeicherten Punkte wie heute im Kern (Rad und MTB 1 s, Wandern und Skitour 3 s).
- Startwerte der Führung (in `docs/ALGORITHMEN.md` begründen, mit echten Tracks nachjustieren erlaubt): Abweichung ab 40 m, frühestens nach 3 Punkten **und** 10 s; zurück auf der Route unter 20 m; Punkte mit Genauigkeit über 50 m zählen nicht; erneuter Versuch nach gescheitertem Weg zurück frühestens nach 60 s oder 200 m Bewegung; Ziel erreicht innerhalb von 30 m vor dem Ende.
- Sichtbare Texte auf Deutsch, wörtlich:
  - Aktivitäten: „Rad“, „MTB“, „Wandern“, „Skitour“
  - „Aufzeichnen“, „Pause“, „Weiter“, „Beenden“, „Aufzeichnung beenden?“, „Abbrechen“
  - „Eine Aufzeichnung wurde unterbrochen.“, „Fortsetzen“, „Als Track speichern“
  - „Routen“, „Tracks“, „Umbenennen“, „Löschen“, „Als GPX teilen“
  - „Führen“, „Ab hier führen“, „Ziel erreicht“, „Kein Weg zurück gefunden“
  - Mitteilung: Titel „Abseits der Route“, Text „Weg zurück zu Wegpunkt {n}: {Distanz}.“ bzw. „Weg zurück zum Ziel: {Distanz}.“ (`{n}` ab 1 gezählt wie in der Wegpunktliste, Distanz mit `Format.distance`)
  - Schalter: „Signalton bei Abweichung“ (Standard an)
- Swift lässt sich unter Linux nicht bauen. Tasks 6–11 laufen auf dem Mac, vorher `apps/trail/scripts/build-core.sh` und `xcodegen generate`.
- Neue Leisten und Linien funktionieren auch im OLED-Dunkelmodus. Im Hintergrund zeichnet Trail die Karte nicht neu, nur der Kern rechnet.
- Echte Karten in `data/` nicht anfassen. Jeder Commit endet mit `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. **Hin- und Rückweg auf derselben Straße** (Stichstraße zu einem Gipfel und zurück): Der Fortschritt springt beim Hinweg nicht auf den Rückweg und umgekehrt. Test `out_and_back_keeps_direction` in Task 2.
2. **GPS-Sprung** um 150 m für einen einzelnen Punkt in der Stadt: keine Abweichung, keine Mitteilung. Test `single_jump_is_no_deviation` in Task 3.
3. **Führung einschalten ohne Position** (GPS sucht noch): Die Führung wartet und setzt beim ersten brauchbaren Punkt an, statt zum Start zu führen oder zu scheitern. Test `starts_on_first_usable_fix` in Task 4.
4. **Route wird geändert, während ein Weg zurück berechnet wird:** Der veraltete Weg zurück wird verworfen, die Führung nutzt die neue Route. Test `plan_change_drops_stale_way_back` in Task 4.
5. **App wird während Aufzeichnung mit ausgeschalteter Führung beendet:** Nach „Fortsetzen“ bleibt die Führung aus. Test `switched_off_survives_restart` in Task 4.

---

## Teil A: Kern (Linux)

### Task 1: Fahrtrichtung für den Weg zurück

**Files:**
- Modify: `crates/core-routing/src/model.rs` (`RouteRequest`)
- Modify: `crates/core-routing/src/valhalla_api.rs` (Aufbau von `locations`)
- Modify: alle Stellen mit `RouteRequest { … }` (u. a. `crates/core-app/src/plan.rs`), mit `heading_deg: None`
- Test: `crates/core-routing/src/valhalla_api.rs` (Modultests)

**Interfaces:**
- Produces: `RouteRequest.heading_deg: Option<f32>` (`#[serde(default)]`). Gilt nur für den ersten Punkt.

- [ ] **Step 1: Failing tests**

```rust
#[test]
fn heading_goes_to_the_first_location_only() {
    let json: Value = serde_json::from_str(&costing_json(&RouteRequest {
        waypoints: vec![a(), b()], profile: Profile::Trekking,
        options: RouteOptions::default(), heading_deg: Some(87.0),
    }).unwrap()).unwrap();
    assert_eq!(json["locations"][0]["heading"], 87);
    assert_eq!(json["locations"][0]["heading_tolerance"], 60);
    assert!(json["locations"][1].get("heading").is_none());
}

#[test]
fn no_heading_leaves_locations_as_before() { /* heading_deg: None → kein "heading"-Schlüssel */ }
```

(`costing_json` baut heute die Anfrage mit `locations`. Namen der Testhelfer `a()`, `b()` an die Datei anpassen.)

- [ ] **Step 2:** `cargo test -p core-routing heading` → FAIL (Feld fehlt).
- [ ] **Step 3:** Feld ergänzen, `heading` gerundet auf ganze Grad, `heading_tolerance: 60`. Alle Konstruktoren mit `heading_deg: None`.
- [ ] **Step 4:** `cargo test --workspace` → PASS.
- [ ] **Step 5:** Commit „Pass the direction of travel to Valhalla for the first point“.

### Task 2: Route als Linie mit Fortschritt

**Files:**
- Create: `crates/core-app/src/guide/mod.rs`, `crates/core-app/src/guide/line.rs`
- Modify: `crates/core-app/src/lib.rs` (`mod guide; pub use guide::…`)
- Test: Modultests in `line.rs`, Fixtures aus `fixtures/gpx/real/`

**Interfaces:**
- Consumes: `RoutePlan`, `Leg`, `Stage`, `LatLon`, `core_track::geo::haversine_m`
- Produces:

```rust
pub enum LegKind { Routed, Straight, Missing }   // Straight = Profile::Manual, Missing = Leg.error / leere Linie
pub struct LegSpan { pub from_m: f64, pub to_m: f64, pub kind: LegKind, pub spec: LegSpec, pub time_s: Option<f64> }
pub struct RouteLine { /* Punkte, kumulierte Meter */ pub legs: Vec<LegSpan>, pub waypoint_m: Vec<f64>, pub stage_end_m: Vec<f64>, pub length_m: f64 }
pub struct Projection { pub along_m: f64, pub offset_m: f64, pub leg: usize }
impl RouteLine {
    pub fn from_plan(plan: &RoutePlan) -> RouteLine;
    /// Nächster Punkt der Linie; `window` begrenzt auf `along_m` in [von, bis].
    pub fn project(&self, p: LatLon, window: Option<(f64, f64)>) -> Option<Projection>;
    pub fn point_at(&self, along_m: f64) -> LatLon;
    pub fn line_between(&self, from_m: f64, to_m: f64) -> Vec<LatLon>;
}
```

Ein gescheitertes Teilstück bekommt eine gerade Ersatzgeometrie zwischen seinen Wegpunkten, nur zum Einrasten (nie angezeigt), Art `Missing`. Abstände in einer lokalen Ebene (äquirektangulär um den Punkt), die Länge der Linie per `haversine_m`.

- [ ] **Step 1: Failing tests** in `line.rs`:
  - `from_plan_places_waypoints_and_stage_ends`: Plan mit 3 Teilstücken, Etappenende bei Wegpunkt 2 → `waypoint_m.len() == 4`, `waypoint_m[0] == 0.0`, `stage_end_m == [waypoint_m[2]]`, `length_m` gleich Summe der Teilstücklängen ±1 m.
  - `failed_leg_is_missing_with_straight_stand_in`: Teilstück mit `error` → `kind == Missing`, `project` eines Punkts auf der Verbindungsgeraden ergibt `offset_m < 1.0`.
  - `manual_leg_is_straight`.
  - `project_finds_offset_beside_the_line`: Punkt 35 m seitlich → `offset_m` 35 ± 2.
  - `out_and_back_keeps_direction`: Hinweg und Rückweg auf derselben Linie (Stichstraße, aus dem Track „von Rottach-Egern nach Obergries“ hin und zurück gebaut). Mit `window` um den Hinweg liefert `project` einen `along_m` im Hinweg, mit Fenster um den Rückweg einen im Rückweg.
- [ ] **Step 2:** `cargo test -p core-app guide::line` → FAIL.
- [ ] **Step 3:** Implementieren.
- [ ] **Step 4:** PASS, `cargo clippy -p core-app -- -D warnings` sauber.
- [ ] **Step 5:** Commit „Measure progress along a planned route“.

### Task 3: Führung als Zustandsautomat

**Files:**
- Create: `crates/core-app/src/guide/guide.rs`
- Test: Modultests mit falschem Router und abgespielten Fixtures (`core_location::GpxReplaySource` oder direkt aus `core_gpx::parse_gpx`)

**Interfaces:**
- Consumes: `RouteLine`, `Projection` (Task 2), `core_location::Fix`
- Produces:

```rust
pub struct GuideParams { pub off_route_m: f64, pub off_route_fixes: u32, pub off_route_min_s: f64,
    pub back_on_m: f64, pub max_accuracy_m: f64, pub retry_after_s: f64, pub retry_after_m: f64, pub arrived_m: f64 }
impl Default for GuideParams { /* Werte aus Global Constraints */ }

pub struct WayBack { pub line: Vec<LatLon>, pub distance_m: f64, pub time_s: Option<f64> }
pub trait WayBackRouter {
    fn way_back(&self, from: LatLon, heading_deg: Option<f32>, to: LatLon, leg: LegSpec) -> Result<WayBack, AppError>;
}
pub trait WayBackTarget {   // Austauschpunkt für die späteren Ansätze B und C
    fn target(&self, line: &RouteLine, along_m: f64) -> Option<usize>;   // Index des Wegpunkts
}
pub struct NextWaypoint;     // Ansatz A: erster Wegpunkt mit waypoint_m > along_m

pub enum GuidePhase { Waiting, OnRoute, OffRoute, NoWayBack, Arrived }
pub struct Remaining { pub distance_m: f64, pub time_s: Option<f64> }
pub struct GuideStatus {
    pub phase: GuidePhase, pub along_m: f64, pub offset_m: Option<f64>,
    pub next_waypoint: Option<usize>,
    /// Zählt jede neue Abweichung hoch; Trail meldet sich, wenn er wächst.
    pub deviation: u32,
    pub way_back: Option<WayBack>,
    pub to_stage_end: Option<Remaining>, pub to_destination: Remaining,
}

pub struct Guide { /* … */ }
impl Guide {
    pub fn new(line: RouteLine, params: GuideParams) -> Guide;          // Phase Waiting
    pub fn start_from_waypoint(&mut self, index: usize);               // setzt along_m = waypoint_m[index]
    pub fn update(&mut self, fix: &Fix, router: &dyn WayBackRouter) -> GuideStatus;
    pub fn status(&self) -> GuideStatus;
}
```

Regeln (aus der Spec, Schritte 2–9 „Ablauf“):
- Im Zustand `Waiting` setzt der erste Punkt mit ausreichender Genauigkeit per globalem `project` an (oder am Wegpunkt aus `start_from_waypoint`). Danach sucht `project` nur im Fenster `[along_m − 200 m, along_m + max(500 m, 3 × seit dem letzten Punkt zurücklegbare Strecke)]`. Das Fenster genau festzulegen bleibt dem Bauenden überlassen, er hält es in `ALGORITHMEN.md` fest.
- Auf `Straight`- und `Missing`-Teilstücken gibt es keine Abweichung.
- Profil für den Weg zurück: `spec` des aktuellen Teilstücks, wenn `Routed`, sonst das des letzten `Routed` davor, sonst danach. Ohne eines gibt es keinen Weg zurück, die Phase wird `NoWayBack`.
- Fahrtrichtung nur mitgeben bei `speed_mps ≥ 1.5` und vorhandenem `course_deg`.
- Bei aktivem Weg zurück misst die Abweichung gegen dessen Linie. Unter `back_on_m` zur geplanten Linie, im Fenster, fällt der Weg zurück weg.
- Restangaben: Weg zurück (falls aktiv) plus geplanter Rest ab Zielwegpunkt bzw. ab `along_m`. Die Zeit anteilig nach `LegSpan.time_s`. `to_stage_end` nur bei Routen mit Etappen, bis zum nächsten `stage_end_m` hinter dem Fortschritt, nach der letzten Etappe gleich `to_destination`.

- [ ] **Step 1: Failing tests** in `guide.rs` (falscher Router zählt Aufrufe und liefert eine Gerade oder einen Fehler):
  - `on_route_never_calls_router`: kompletter Track Sylvensteinsee als Route und als Fixes → `deviation == 0`, Router 0 Aufrufe, letzte Phase `Arrived`.
  - `deviation_after_three_fixes_and_ten_seconds`: 3 Fixes in 45 m Abstand innerhalb von 4 s → `OnRoute`. Weiter bis 10 s → `OffRoute`, `deviation == 1`, Router 1 Aufruf mit `to == waypoint_m`-Punkt von `next_waypoint`.
  - `single_jump_is_no_deviation`: ein Fix 150 m daneben zwischen Punkten auf der Route → `deviation == 0`.
  - `inaccurate_fixes_do_not_count`: `h_accuracy_m = Some(80.0)` 60 m daneben → keine Abweichung.
  - `back_on_route_before_waypoint_drops_way_back`: nach Abweichung Rückkehr auf die Linie 300 m vor dem Wegpunkt → `way_back == None`, `OnRoute`, `deviation` bleibt 1.
  - `leaving_the_way_back_is_a_new_deviation` → `deviation == 2`.
  - `no_inner_waypoints_routes_to_destination`: Route mit 2 Wegpunkten → `to` des Routers ist das Ziel.
  - `failed_way_back_waits_before_retry`: Router liefert Fehler → `NoWayBack`, nächster Fix 5 s später kein neuer Aufruf, nach 60 s oder 200 m einer.
  - `straight_and_missing_legs_never_deviate`.
  - `way_back_profile_from_routed_neighbour`: Abweichung auf `Straight`-Teilstück nach Fortschritt ins nächste `Routed` → Profil des letzten `Routed` davor.
  - `remaining_with_stages`: Route mit Etappenende → `to_stage_end.distance_m < to_destination.distance_m`. Hinter dem letzten Etappenende gleich.
  - `start_from_waypoint_skips_global_search`.
- [ ] **Step 2:** `cargo test -p core-app guide::guide` → FAIL.
- [ ] **Step 3:** Implementieren.
- [ ] **Step 4:** PASS, clippy sauber.
- [ ] **Step 5:** Commit „Guide along a planned route and back to the next waypoint“.

### Task 4: Führung in der App

**Files:**
- Create: `crates/core-app/src/guide/app.rs` (`impl App` für die Führung, `impl WayBackRouter` für die App)
- Modify: `crates/core-app/src/app.rs` (`Shared` bekommt `guide: Mutex<Option<ActiveGuide>>`, `handle_fix` füttert die Führung, Start/Fortsetzen der Aufzeichnung), `crates/core-app/src/routes.rs` (`finish_plan`, `set_planned` melden Planwechsel), `crates/core-app/src/plan.rs` (Teilstück mit Richtung ohne Cache)
- Test: `crates/core-app/tests/guide.rs` (mit `common::app_with_ramp_routing` bzw. `set_router_for_tests`)

**Interfaces:**
- Consumes: `Guide`, `GuideStatus`, `WayBackRouter` (Task 3), `RouteRequest.heading_deg` (Task 1)
- Produces:

```rust
impl App {
    pub fn start_guidance(&self) -> Result<GuideStatus, AppError>;   // AppError::NoRoutePlanned ohne Route
    pub fn guide_from_waypoint(&self, index: usize) -> Result<GuideStatus, AppError>;
    pub fn stop_guidance(&self);
    pub fn guidance_status(&self) -> Option<GuideStatus>;
}
pub trait AppListener { /* … */ fn on_guidance(&self, _status: Option<&GuideStatus>) {} }  // None = Führung beendet
```

Regeln:
- `handle_fix`: erst aufzeichnen wie heute, dann führen. **Nie Valhalla aufrufen, während `inner` gesperrt ist.** Die Führung hat ihr eigenes Mutex.
- Bei einem Wechsel der geplanten Route (`finish_plan`, `set_planned(Some)`, Laden): Läuft die Führung, wird sie mit neuer `RouteLine` und Phase `Waiting` neu angesetzt, mit der letzten Position. Ein Weg zurück, der zur alten Route berechnet wurde, wird verworfen (Generationszähler). `set_planned(None)` beendet die Führung.
- `Arrived` beendet die Führung nach dem Melden des Status.
- Kopplung an die Aufzeichnung über `Storage::setting`/`set_setting`, Schlüssel `guide_off_track` = Track-ID:
  - `start_recording`: Ist eine Route geplant, startet die Führung.
  - `stop_guidance` während einer Aufzeichnung setzt den Schlüssel auf die laufende Track-ID.
  - `continue_interrupted`: Die Führung startet, außer der Schlüssel nennt diesen Track.
  - Eine Route, die während einer Aufzeichnung geplant oder geladen wird, startet die Führung, außer der Schlüssel nennt die laufende Aufzeichnung.
  - `start_guidance` von Hand löscht den Schlüssel.
- Der Weg zurück nutzt dieselbe Regionswahl und Versionsprüfung wie `App::leg`, aber ohne Leg-Cache, und misst Zeit und Distanz wie `measure`.

- [ ] **Step 1: Failing tests** in `tests/guide.rs`:
  - `start_without_route_fails` → `Err(NoRoutePlanned)` (Code des vorhandenen Fehlers `no_route_planned`).
  - `starts_on_first_usable_fix`: `start_guidance` ohne Position → Phase `Waiting`. Erster Fix mit Genauigkeit 120 m bleibt `Waiting`, der nächste mit 8 m nahe Wegpunkt 2 → `OnRoute`, `next_waypoint == Some(2)`.
  - `recording_starts_guidance_with_route` und `recording_without_route_has_no_guidance`.
  - `switched_off_survives_restart`: Aufzeichnung mit Route, `stop_guidance`, App schließen und neu öffnen, `continue_interrupted` → `guidance_status() == None`.
  - `route_loaded_during_recording_starts_guidance`, `…_unless_switched_off`.
  - `plan_change_drops_stale_way_back`: Router blockiert den ersten Weg zurück, bis während der Berechnung ein neuer Plan kommt (Kanal im Testrouter). Der Status danach hat `way_back == None` und nutzt die neue Linie.
  - `clearing_the_route_ends_guidance` → Listener bekommt `on_guidance(None)`.
  - `guidance_never_changes_the_plan`: `planned_route()` vor und nach Abweichung und Weg zurück gleich.
- [ ] **Step 2:** `cargo test -p core-app --test guide` → FAIL.
- [ ] **Step 3:** Implementieren.
- [ ] **Step 4:** `cargo test --workspace` PASS, clippy sauber.
- [ ] **Step 5:** Commit „Run the guide on the app's positions, route and recording“.

### Task 5: Aufzeichnung, Tracks und Führung über FFI

**Files:**
- Create: `crates/core-ffi/src/track.rs`, `crates/core-ffi/src/guide.rs`
- Modify: `crates/core-ffi/src/lib.rs` (`CoreListener`, `Listener`, `mod`), `docs/ALGORITHMEN.md` (neuer Abschnitt „7. Live-Routing“ mit Werten und Begründung)
- Test: `crates/core-ffi/tests/ffi.rs` (vorhandene Testdatei erweitern)

**Interfaces:**
- Consumes: `App`-Methoden der Aufzeichnung (`start_recording` … `export_gpx`), Task 4
- Produces (alle `#[uniffi::export]` auf `CoreApp`, Records `uniffi::Record`, IDs als `String`, Aktivität als `"bike" | "mtb" | "hike" | "ski_tour"`):

```rust
pub struct FixInput { lat: f64, lon: f64, ele: Option<f64>, time_ms: i64, h_accuracy_m: Option<f32>, speed_mps: Option<f32>, course_deg: Option<f32> }
pub struct RecordingInfo { phase: String /* "idle"|"recording"|"paused" */, track_id: Option<String>, activity: Option<String>,
    distance_m: f64, moving_s: f64, elapsed_s: f64, ascent_m: Option<f64>, appended: Vec<LineVertex> }
pub struct LineVertex { segment: u32, lat: f64, lon: f64 }
pub struct TrackSummary { id: String, name: Option<String>, activity: String, started_ms: i64, distance_m: f64, moving_s: f64, ascent_m: Option<f64> }
pub struct TrackView { summary: TrackSummary, segments: Vec<Vec<LatLon>>, descent_m: Option<f64>, max_ele_m: Option<f64>, elapsed_s: f64 }
pub struct GuidanceInfo { phase: String /* "waiting"|"on_route"|"off_route"|"no_way_back"|"arrived" */, along_m: f64,
    next_waypoint: Option<u32>, deviation: u32, way_back: Vec<LatLon>, way_back_m: Option<f64>,
    to_stage_end_m: Option<f64>, to_stage_end_s: Option<f64>, to_destination_m: f64, to_destination_s: Option<f64> }

fn handle_fix(&self, fix: FixInput);
fn start_recording(&self, activity: String) -> Result<RecordingInfo, CoreError>;
fn pause_recording(&self) -> Result<RecordingInfo, CoreError>;  fn resume_recording(&self) -> Result<RecordingInfo, CoreError>;
fn stop_recording(&self) -> Result<Option<TrackSummary>, CoreError>;
fn recording_status(&self) -> RecordingInfo;  fn recording_line(&self) -> Vec<Vec<LatLon>>;
fn interrupted_recording(&self) -> Result<Option<TrackSummary>, CoreError>;
fn continue_interrupted(&self) -> Result<RecordingInfo, CoreError>;  fn finish_interrupted(&self) -> Result<Option<TrackSummary>, CoreError>;
fn list_tracks(&self) -> Result<Vec<TrackSummary>, CoreError>;  fn track_detail(&self, id: String) -> Result<TrackView, CoreError>;
fn rename_track(&self, id: String, name: String) -> Result<(), CoreError>;  fn delete_track(&self, id: String) -> Result<(), CoreError>;
fn export_track_gpx(&self, id: String) -> Result<String, CoreError>;
fn start_guidance(&self) -> Result<GuidanceInfo, CoreError>;  fn guide_from_waypoint(&self, index: u32) -> Result<GuidanceInfo, CoreError>;
fn stop_guidance(&self);  fn guidance_status(&self) -> Option<GuidanceInfo>;

// CoreListener (with_foreign) bekommt:
fn on_recording(&self, status: RecordingInfo);
fn on_guidance(&self, status: Option<GuidanceInfo>);
```

Statistikfelder aus `TrackStats` bzw. `dem_stats`, falls vorhanden. Feldnamen dort nachsehen, die Abbildung ist Sache des Bauenden.

- [ ] **Step 1: Failing tests** in `tests/ffi.rs`: `recording_round_trip_over_ffi` (Start „hike“, 20 Fixes über `handle_fix`, Stop → `list_tracks().len() == 1`, `export_track_gpx` enthält `<trkpt`), `unknown_activity_is_an_error`, `listener_gets_recording_and_guidance` (Testlistener zählt Aufrufe, `on_guidance(None)` nach `stop_guidance`).
- [ ] **Step 2:** `cargo test -p core-ffi` → FAIL.
- [ ] **Step 3:** Implementieren, `ALGORITHMEN.md` ergänzen.
- [ ] **Step 4:** `cargo test --workspace`, `cargo clippy --workspace -- -D warnings` PASS.
- [ ] **Step 5:** Commit „Offer recording, tracks and guidance to the apps“.
- [ ] **Step 6: Anhalten.** Dem Nutzer zeigen, Push nach Rückfrage, Übergabe für den Mac schreiben (`docs/superpowers/plans/…-uebergabe-trail-mac.md` wie beim letzten Mal).

## Teil B: Trail (Mac)

### Task 6: Gemeinsamer Ortungsdienst

**Files:**
- Create: `apps/trail/ZeroGridTrail/Location/LocationService.swift`, `apps/trail/ZeroGridTrail/Location/GPXReplay.swift`
- Modify: `apps/trail/project.yml` (`UIBackgroundModes: [location]`), `apps/trail/ZeroGridTrail/Core/CoreHost.swift` (neue Listener-Methoden, auf den Main-Thread weiterreichen), `apps/trail/ZeroGridTrail/Map/MapView.swift` (Standortknopf über den Dienst, MapLibre-Punkt aus dem Dienst)
- Test: `apps/trail/ZeroGridTrailTests/LocationServiceTests.swift`

**Interfaces:**
- Consumes: `CoreApp.handleFix(fix:)` (Task 5)
- Produces:

```swift
enum LocationDemand: Hashable { case map, recording, guidance }
struct LocationProfile { let accuracy: CLLocationAccuracy; let distanceFilter: CLLocationDistance; static let standard: LocationProfile }
protocol LocationManaging: AnyObject { /* die genutzten CLLocationManager-Eigenschaften und -Methoden */ }
@MainActor final class LocationService: NSObject, ObservableObject {
    init(core: CoreApp, manager: LocationManaging = CLLocationManager())
    @Published private(set) var last: CLLocation?
    var activity: String          // "bike"…, setzt activityType: Rad/MTB .fitness, Wandern/Skitour .fitness, sonst .other
    func require(_ demand: LocationDemand)
    func release(_ demand: LocationDemand)
    var running: Bool { get }
}
```

- Hintergrund (`allowsBackgroundLocationUpdates`, `showsBackgroundLocationIndicator`) genau dann, wenn `.recording` oder `.guidance` angefordert ist.
- Der Positionspunkt der Karte kommt aus dem Dienst: ein eigener `MLNLocationManager`, der `last` weitergibt, damit nur ein GPS läuft. Die einmalige Abfrage in `SearchBridge.PositionRequest` bleibt.
- Startargument `-replayGPX <Pfad>`: `GPXReplay` ersetzt den Manager und spielt die Punkte zehnmal so schnell ab, für UI-Tests der Tasks 7–10. Fixture-GPX wird mit ins Test-Bundle genommen.

- [ ] **Step 1: Failing tests:** `demands_union_and_release` (map+recording, release recording → läuft weiter, Hintergrund aus; release map → gestoppt), `background_only_for_recording_or_guidance`, `standard_profile_values` (`kCLLocationAccuracyBest`, `kCLDistanceFilterNone`, `pausesLocationUpdatesAutomatically == false`), `fixes_reach_the_core` (falscher Manager liefert eine Position, `core.lastPosition`-Gegenstück bzw. eine Aufzeichnung enthält sie).
- [ ] **Step 2:** `apps/trail/scripts/test.sh -only-testing:ZeroGridTrailTests/LocationServiceTests` → FAIL.
- [ ] **Step 3:** Implementieren. Der vorhandene `LocationUITests` muss weiter grün sein (GPS beim Start aus).
- [ ] **Step 4:** PASS, dazu `-only-testing:ZeroGridTrailUITests/LocationUITests`.
- [ ] **Step 5:** Commit „Share one location service between map, recording and guidance“.

### Task 7: Aufzeichnen auf der Karte

**Files:**
- Create: `apps/trail/ZeroGridTrail/Recording/RecordingController.swift`, `RecordingBar.swift`, `ActivityPicker.swift`, `TrackLayers.swift`
- Modify: `apps/trail/ZeroGridTrail/Map/MapScreen.swift`, `MapToolbar.swift` (Knopf „Aufzeichnen“), `apps/trail/ZeroGridTrail/AppStart.swift` (Frage nach unterbrochener Aufzeichnung)
- Test: `apps/trail/ZeroGridTrailTests/RecordingControllerTests.swift`, `apps/trail/ZeroGridTrailUITests/RecordingUITests.swift`

**Interfaces:**
- Consumes: `LocationService.require/release(.recording)`, `activity` (Task 6). FFI-Aufzeichnung und `on_recording` (Task 5).
- Produces: `@MainActor final class RecordingController: ObservableObject` mit `@Published info: RecordingInfo`, `start(activity:)`, `pause()`, `resume()`, `stop() -> TrackSummary?`, `interrupted: TrackSummary?`, `continueInterrupted()`, `finishInterrupted()`. Zuletzt gewählte Aktivität in `UserDefaults` unter `lastActivity`.

Leiste wie in der Spec skizziert (Zeit, Strecke, Höhenmeter, „Pause“/„Weiter“, „Beenden“ mit Rückfrage „Aufzeichnung beenden?“). Die Linie live über `appended`, beim Öffnen der Karte einmal über `recordingLine()`.

- [ ] **Step 1: Failing tests:** `start_requires_location_and_stop_releases_it`, `last_activity_is_preselected`, `interrupted_offers_continue_and_save`. UI: `test_record_pause_resume_stop` (mit `-replayGPX`: Aufzeichnen → „Wandern“ → Leiste zeigt Strecke > 0 → „Pause“ → „Weiter“ → „Beenden“ → „Beenden“ bestätigen → Leiste weg).
- [ ] **Step 2:** FAIL.
- [ ] **Step 3:** Implementieren.
- [ ] **Step 4:** PASS.
- [ ] **Step 5:** Commit „Record tours in Trail“.

### Task 8: Tracks ansehen, umbenennen, löschen, teilen

**Files:**
- Create: `apps/trail/ZeroGridTrail/Recording/TracksList.swift`, `TrackDetailView.swift`
- Modify: `apps/trail/ZeroGridTrail/Plan/RoutesList.swift` (Umschalter „Routen“ / „Tracks“ oben in der Liste)
- Test: `apps/trail/ZeroGridTrailUITests/TracksUITests.swift`

**Interfaces:**
- Consumes: `listTracks`, `trackDetail`, `renameTrack`, `deleteTrack`, `exportTrackGpx` (Task 5), `TrackLayers` (Task 7)

Detail zeigt den Track auf der Karte (eingepasst) und die Statistik. „Als GPX teilen“ schreibt `<Name oder Datum>.gpx` in ein temporäres Verzeichnis und öffnet `ShareLink`. Löschen fragt nach.

- [ ] **Step 1: Failing UI test** `test_track_list_rename_share_delete` (nach einer Aufzeichnung wie in Task 7).
- [ ] **Step 2:** FAIL. **Step 3:** Implementieren. **Step 4:** PASS, ganzer `scripts/test.sh` grün.
- [ ] **Step 5:** Commit „Show, rename, share and delete recorded tracks“.
- [ ] **Step 6: Anhalten, Meilenstein 1.** Der Nutzer prüft auf dem iPhone: Runde mit gesperrtem Bildschirm, kein Punkt fehlt, die Frage nach „Immer“ wird festgehalten, ebenso der Akku pro Stunde. Ergebnis in `docs/MESSPROTOKOLL.md`.

### Task 9: Führen auf der Karte

**Files:**
- Create: `apps/trail/ZeroGridTrail/Guide/GuideController.swift`, `GuideBar.swift`, `GuideLayers.swift`
- Modify: `apps/trail/ZeroGridTrail/Plan/PlanBar.swift` (Schalter „Führen“, nur mit geladener Route), `apps/trail/ZeroGridTrail/Plan/WaypointViews.swift` (Menüpunkt „Ab hier führen“), `apps/trail/ZeroGridTrail/Map/MapView.swift` (Folgen-Modus)
- Test: `apps/trail/ZeroGridTrailTests/GuideControllerTests.swift`, `apps/trail/ZeroGridTrailUITests/GuideUITests.swift`

**Interfaces:**
- Consumes: FFI-Führung und `on_guidance` (Task 5), `LocationService.require/release(.guidance)` (Task 6)
- Produces: `@MainActor final class GuideController: ObservableObject` mit `@Published info: GuidanceInfo?`, `toggle()`, `startFromWaypoint(_ index: Int)`, `@Published following: Bool`, und `var newDeviation: AnyPublisher<GuidanceInfo, Never>` (feuert, wenn `deviation` wächst) für Task 10.

- Gefahrener Teil der Route blasser (`along_m`), Weg zurück durchgezogen in eigener Farbe, Farbe nicht wie `failed_leg_color()`.
- Leiste: bei Etappen zwei Zeilen „Etappe: 12,3 km · 0:48 h“ und „Ziel: 84,0 km · 5:10 h“, sonst eine. Bei `no_way_back` „Kein Weg zurück gefunden“, bei `arrived` kurz „Ziel erreicht“.
- Folgen: an beim Einschalten. Ein Verschieben der Karte beendet es, ein Knopf in der Leiste holt es zurück.
- Kommt `on_guidance(None)` (Route geschlossen), verschwindet die Leiste, und `.guidance` wird freigegeben.

- [ ] **Step 1: Failing tests:** `toggle_requires_and_releases_guidance_location`, `deviation_publisher_fires_once_per_deviation` (zweimal derselbe `deviation`-Wert → ein Ereignis). UI `test_guide_shows_way_back_after_leaving_route` (Route laden, `-replayGPX` mit Abweichung, „Führen“ → Leiste sichtbar, nach der Abweichung Element `guide-way-back` vorhanden).
- [ ] **Step 2:** FAIL. **Step 3:** Implementieren. **Step 4:** PASS.
- [ ] **Step 5:** Commit „Guide along the route in Trail“.

### Task 10: Hinweis bei Abweichung

**Files:**
- Create: `apps/trail/ZeroGridTrail/Guide/DeviationAlert.swift`
- Modify: `apps/trail/ZeroGridTrail/Map/MapToolbar.swift` (Schalter „Signalton bei Abweichung“ im vorhandenen Menü, `UserDefaults` `deviationSound`, Standard `true`)
- Test: `apps/trail/ZeroGridTrailTests/DeviationAlertTests.swift`

**Interfaces:**
- Consumes: `GuideController.newDeviation` (Task 9)
- Produces: `final class DeviationAlert` mit `init(center: NotificationScheduling, haptics: Haptics, defaults: UserDefaults)` und `func handle(_ info: GuidanceInfo, appActive: Bool)`. `NotificationScheduling` und `Haptics` sind Protokolle für die Tests.

- Bei jeder neuen Abweichung vibriert es. Im Hintergrund oder bei gesperrtem Bildschirm kommt dazu eine lokale Mitteilung mit Titel und Text aus den Global Constraints, Ton nach Schalter. Im Vordergrund gibt es keine Mitteilung.
- Die Erlaubnis für Mitteilungen fragt Trail beim ersten „Führen“ ab. Ohne Erlaubnis bleibt es bei Vibration und Anzeige.
- Ob die Vibration im Hintergrund ohne Ton ankommt, prüft der Handtest in Task 11. Falls nicht, im Protokoll festhalten und den Nutzer fragen.

- [ ] **Step 1: Failing tests:** `background_deviation_schedules_one_notification` (Text „Weg zurück zu Wegpunkt 4: 650 m.“ für `nextWaypoint = 3`, `wayBackM = 650`), `destination_text_without_inner_waypoint`, `foreground_vibrates_without_notification`, `sound_switch_off_sends_silent_notification`.
- [ ] **Step 2:** FAIL. **Step 3:** Implementieren. **Step 4:** PASS, ganzer `scripts/test.sh` grün.
- [ ] **Step 5:** Commit „Tell the user when they leave the route“.

### Task 11: Test draußen und Abschluss

**Files:**
- Modify: `docs/MESSPROTOKOLL.md`, `docs/ALGORITHMEN.md` (falls Werte nachjustiert)

- [ ] **Step 1:** Release-Build auf das iPhone, Flugmodus. Der Nutzer fährt je eine Runde mit eingeschaltetem und mit gesperrtem Bildschirm und biegt absichtlich ab: einmal mit Rückkehr vor dem Wegpunkt, einmal bis zum Wegpunkt. Dazu eine Route nur mit Start und Ziel.
- [ ] **Step 2:** Festhalten: Punkte lückenlos? Genau eine Mitteilung je Abweichung? Vibration im Hintergrund? Weg zurück sinnvoll? Akku in %/h mit Bildschirm an und aus.
- [ ] **Step 3:** Commit „Record the outdoor test of recording and guidance“.
- [ ] **Step 4: Anhalten, Meilenstein 2.** Danach PR und Merge `--no-ff` nach Rückfrage.
