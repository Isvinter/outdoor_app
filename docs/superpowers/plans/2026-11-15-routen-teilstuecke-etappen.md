# Routen aus Teilstücken, Etappen und Aktivitätsfarben – Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eine Route besteht aus Teilstücken mit eigenem Profil, gegliedert in vom Nutzer
gesetzte Etappen; der Kern rechnet nur geänderte Teilstücke neu, die Oberfläche zeigt Farben
je Profil auf Karte und Höhenprofil.

**Architecture:** Neues Modul `crates/core-app/src/plan.rs` mit Anfrage-/Ergebnistypen,
Teilstück-Speicher und Statistik; `App::plan_route` nimmt `PlanRequest` statt `RouteRequest`
und ruft den Router je Teilstück (`core-routing` bleibt unverändert). Speichern und GPX bauen
auf dem neuen `RoutePlan` auf; alte gespeicherte Pläne werden beim Laden neu gerechnet.
Die Oberfläche hält Wegpunkte, Profile je Teilstück und Etappenenden und schickt immer den
ganzen Zustand.

**Tech Stack:** Rust (core-app, core-gpx, core-storage), Tauri 2, TypeScript + MapLibre.

**Spec:** `docs/superpowers/specs/2026-11-15-routen-teilstuecke-etappen-design.md`

## Global Constraints

- Ein Fehler in einem Teilstück löscht nie bereits berechnete Teilstücke (`plan_route` gibt dann trotzdem `Ok`).
- Bestehende gespeicherte Routen öffnen weiter.
- Logik im Rust-Kern; die Oberfläche rechnet keine Statistik.
- Keine neue Abhängigkeit im Kern (mobile `cargo check` in der CI).
- Profilwechsel im Formular gilt für das letzte Teilstück und alle neuen.
- Entfernen eines mittleren Punkts: Nachbarn mit dem Profil des Teilstücks **davor** verbinden; seine Etappenmarke entfällt.
- Zeit je Teilstück: zu Fuß (`hike`, `manual`) DIN 33466 aus Distanz/Auf-/Abstieg des Teilstücks, sonst Routerzeit; Etappe und gesamt sind Summen.
- Oberflächentexte deutsch in `i18n/de.ts`; Code und Kommentare englisch.
- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `pnpm exec tsc --noEmit` grün nach jeder Aufgabe.

## Review Focus

1. Etappenenden beim Entfernen/Verschieben von Punkten: Indizes müssen mitwandern, eine Marke auf dem entfernten Punkt verschwindet, keine Marke auf erstem/letztem Punkt.
2. Ein fehlerhaftes Teilstück mitten in der Route, danach weitere Punkte: das fehlerhafte bleibt markiert, die Gesamtwerte sagen, dass sie unvollständig sind; „Rückgängig“ bis davor stellt alles wieder her.
3. Gespeicherte Routen im alten Format (`plan_json` ohne `format`) laden ohne Fehler, auch wenn die Region inzwischen gelöscht ist.
4. Lange Routen (300 Punkte, ~1 000 km): ein neuer Punkt darf nicht alles neu rechnen; die Antwortgröße bleibt handhabbar.
5. Teilstück ohne Höhendaten: dessen Auf-/Abstieg ist `None`, Etappe und gesamt dann ebenfalls `None` statt zu wenig.

---

### Task 1: Planmodell, Teilstück-Speicher und Statistik im Kern

**Files:**
- Create: `crates/core-app/src/plan.rs`
- Modify: `crates/core-app/src/routes.rs` (`plan_route`, `Routing`, `forget_region`, alte `RoutePlan` entfernen), `crates/core-app/src/lib.rs` (Exporte), `crates/core-app/src/error.rs` (falls neue Codes)
- Test: `crates/core-app/tests/routes.rs` (bestehende Tests auf das neue Modell umstellen, neue ergänzen)

**Interfaces:**
- Produces (alle `Serialize + Deserialize + Clone + Debug + PartialEq`, `serde(rename_all = "snake_case")` wo Enums):
  ```rust
  pub struct LegSpec { pub profile: Profile, #[serde(default)] pub options: RouteOptions }
  pub struct PlanRequest { pub waypoints: Vec<LatLon>, pub legs: Vec<LegSpec>, #[serde(default)] pub stage_ends: Vec<usize> }
  pub struct LegError { pub code: String, pub message: String }   // code wie AppError::code()
  pub struct Leg {
      pub profile: Profile, pub options: RouteOptions,
      pub line: Vec<LatLon>,            // leer bei Fehler
      pub distance_m: f64,              // 0 bei Fehler
      pub duration_s: Option<f64>,      // Router
      pub ascent_m: Option<f64>, pub descent_m: Option<f64>,
      pub time_s: Option<f64>,          // DIN zu Fuß, sonst duration_s
      pub region: Option<String>, pub max_mtb_scale: Option<u8>,
      pub error: Option<LegError>,
  }
  pub struct RoutePoint { pub distance_m: f64, pub lat: f64, pub lon: f64, pub ele: Option<f32>, pub leg: usize }
  pub struct Stage { pub from: usize, pub to: usize, pub distance_m: f64, pub ascent_m: Option<f64>, pub descent_m: Option<f64>, pub time_s: Option<f64>, pub profiles: Vec<Profile> }
  pub struct Totals { pub distance_m: f64, pub ascent_m: Option<f64>, pub descent_m: Option<f64>, pub time_s: Option<f64>, pub min_ele_m: Option<f64>, pub max_ele_m: Option<f64>, pub failed_legs: usize }
  pub struct RoutePlan { pub format: u32 /* = 2 */, pub request: PlanRequest, pub legs: Vec<Leg>, pub elevation: Vec<RoutePoint>, pub stages: Vec<Stage>, pub total: Totals }
  impl App { pub fn plan_route(&self, request: &PlanRequest) -> Result<RoutePlan, AppError> }
  ```
  `Stage.from/to` sind Wegpunktindizes. `RoutePoint.distance_m` läuft über die ganze Route.
  Fehlerhafte Teilstücke tragen 0 m bei; Summen über `Option` sind `None`, sobald ein beteiligtes Teilstück `None` hat.
- Consumes: `RegionStore::routing_tiles(&[(f64, f64)])`, `Router::route(&RouteRequest)`, `manual_route`, `din_time_s`, `elevation_gain`, `RegionStore::elevation_profile`.

- [ ] **Step 1: Tests schreiben** in `crates/core-app/tests/routes.rs`. Bestehende Tests auf `PlanRequest` umstellen (Hilfsfunktion `manual(waypoints)` baut Teilstücke mit `Profile::Manual`; Werte bleiben: Rampe 4 090 m ± 60, Aufstieg 80 ± 1, DIN-Zeit gleich `din_time_s`). Neu:
  - `legs_must_match_the_waypoints` – 3 Punkte, 1 Teilstück → `Err`, Code `route_invalid_option`.
  - `stage_ends_must_be_inner_ascending_waypoints` – Etappenende 0, letzter Index oder absteigend → `Err`.
  - `each_stage_has_its_statistics_and_the_total_is_their_sum` – Rampe, 3 Punkte, Etappenende 1: zwei Etappen, `stages[0].from == 0 && to == 1`, Summe der Distanzen/Aufstiege/Zeiten == `total` (± 1e-6).
  - `a_failing_leg_keeps_the_others` – Rampe mit `Profile::Hike` auf mittlerem Teilstück ohne Routingdaten, Rest `Manual`: `Ok`, `legs[1].error.is_some()`, `legs[0].line` und `legs[2].line` nicht leer, `total.failed_legs == 1`.
  - `only_changed_legs_are_computed` – Router-Attrappe (`Router`-Impl im Test, zählt Aufrufe, liefert gerade Linie) über einen Test-Einstieg (siehe Step 3): Plan mit 3 Punkten → 2 Aufrufe; gleicher Plan + 1 Punkt → 1 weiterer; letzter Punkt verschoben → 1 weiterer; Profil des letzten Teilstücks geändert → 1 weiterer.
  - `elevation_points_know_their_leg` – `elevation` beginnt mit `leg == 0`, endet mit dem letzten Teilstück, `distance_m` monoton.
  - Valhalla-Modul: `hike_is_routed_on_the_tiles_of_the_region` auf `PlanRequest` umstellen; neu `mixed_hike_and_mtb_legs_on_the_fixture` (Bahnhof → Neureuth wandern, zurück MTB): beide Teilstücke ohne Fehler, `stages[0].profiles == [Hike, Mtb]`.

- [ ] **Step 2: Tests laufen lassen**
  Run: `cargo test -p core-app --test routes`
  Expected: FAIL beim Kompilieren (`PlanRequest` unbekannt).

- [ ] **Step 3: `plan.rs` umsetzen**
  - Prüfung der Anfrage: `waypoints.len() >= 2` sonst `TooFewWaypoints`; `legs.len() == waypoints.len() - 1`, Etappenenden in `1..waypoints.len()-1` streng aufsteigend, sonst `RoutingError::InvalidOption`.
  - Je Teilstück: `Manual` → `manual_route(&[from, to])`; sonst `routing_tiles(&[from, to])` → Router aus `Routing::routers` (wie heute geöffnet) → `route`. Kein Gebiet → Teilstück-Fehler `route_unavailable`. Höhenprofil des Teilstücks mit `elevation_profile(line, PROFILE_STEP_M)`, Auf-/Abstieg mit `elevation_gain` wie heute (nur bei vollständigen Höhen).
  - Teilstück-Speicher in `Routing`: `HashMap<String, CachedLeg>` mit Schlüssel aus Region, Start, Ziel (auf 1e-7° formatiert), Profil, `serde_json` der Optionen; `CachedLeg` = `Leg` + Höhenpunkte des Teilstücks. Fehler nicht speichern. Über 5 000 Einträge: leeren. `forget_region` leert den Speicher.
  - Für den Attrappen-Test: `#[doc(hidden)] pub fn set_router_for_tests(&self, region: &str, router: Arc<dyn Router>)` auf `App` (setzt den Eintrag in `routers`); die Testregion braucht ein Manifest mit `routing.tar` wie `app_with_routing`.
  - Zusammensetzen: `elevation` mit fortlaufender Distanz und `leg`, `stages` an den Etappenenden, `total` als Summe, Min/Max-Höhe über alle bekannten Punkte. Der Plan wird wie heute als geplante Route gemerkt.

- [ ] **Step 4: Tests laufen lassen**
  Run: `cargo test -p core-app --features valhalla --test routes` (bzw. Feature-Name wie in `core-app/Cargo.toml`)
  Expected: PASS

- [ ] **Step 5: Commit**
  `git commit -m "core-app: routes from legs with stages, only changed legs are computed"`

### Task 2: Speichern, Laden alter Pläne, GPX mit Etappen

**Files:**
- Modify: `crates/core-app/src/routes.rs` (`save_planned_route`, `load_plan`, `plan_gpx`), `crates/core-gpx/src/lib.rs`, `crates/core-gpx/src/write.rs`
- Test: `crates/core-app/tests/routes.rs`, `crates/core-gpx/tests/*` (bestehende Schreibtests)

**Interfaces:**
- Consumes: `RoutePlan`, `PlanRequest` aus Task 1.
- Produces: `GpxDocument { name, tracks, waypoints: Vec<GpxWaypoint> }` mit `pub struct GpxWaypoint { pub lat: f64, pub lon: f64, pub ele: Option<f64>, pub name: Option<String> }`, geschrieben als `<wpt>` vor den `<trk>`. Die Liste der Profile eines Plans für die Spalte `profile`: `fn profile_label(plan: &RoutePlan) -> String` (verschiedene Profile in Reihenfolge des ersten Auftretens, mit `+` verbunden).

- [ ] **Step 1: Tests schreiben**
  - `planned_route_is_saved_listed_loaded_renamed_and_deleted` umstellen; dazu `mixed_route_is_listed_with_all_profiles` – Profil-Spalte `"manual+hike"` o. ä. bei gemischtem Plan (Hike-Teilstück darf fehlschlagen; die Spalte zählt die Anfrage).
  - `old_saved_plans_are_loaded_and_recomputed` – schreibt über `Storage` direkt einen alten `plan_json` (aus dem heutigen Format, Profil `manual`, 2 Wegpunkte auf der Rampe) und lädt ihn: `request.legs.len() == 1`, Profil `Manual`, Distanz wie neu gerechnet. Zweiter Fall: altes Profil `hike` ohne Routingdaten → lädt trotzdem, `legs[0].error.is_some()`.
  - `routes_are_exported_as_gpx_with_stages` – Rampe, 3 Punkte, Etappenende 1: zwei `<trk>` „Etappe 1“/„Etappe 2“, ein `<wpt>` am Etappenende, Höhen vorhanden. Eine Etappe → ein `<trk>` mit dem Routennamen, kein `<wpt>`.
  - In `core-gpx`: `waypoints_are_written_before_tracks`.

- [ ] **Step 2: Tests laufen lassen** – Run: `cargo test -p core-app --test routes && cargo test -p core-gpx` – Expected: FAIL.

- [ ] **Step 3: Umsetzen**
  - Speichern: `plan_json` = `RoutePlan` (mit `format: 2`), Distanz/Aufstieg aus `total`.
  - Laden: JSON mit `"format": 2` → direkt; sonst private Struktur des alten Formats (`route.profile`, `route.waypoints`, `options`) lesen und `plan_route` mit allen Teilstücken in diesem Profil aufrufen.
  - GPX: je Etappe ein `Track` (Name „Etappe n“ bzw. Routenname bei einer Etappe), je Folge gleicher Aktivität ein `Segment` mit den Höhenpunkten dieser Teilstücke; `activity` gesetzt, wenn in der Etappe einheitlich. Etappenenden als `GpxWaypoint` mit Name „Etappe n Ende“. Bestehende `GpxDocument { .. }`-Konstruktionen um `waypoints: Vec::new()` ergänzen (6 Stellen).

- [ ] **Step 4: Tests laufen lassen** – Expected: PASS, dazu `cargo test --workspace`.

- [ ] **Step 5: Commit** – `git commit -m "core-app: save mixed routes, load old plans, GPX with stages"`

### Task 3: Tauri-Befehl und API-Typen

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands.rs` (`route_plan` nimmt `PlanRequest`), `apps/desktop/ui/src/api.ts`

**Interfaces:**
- Produces in `api.ts`: `LegSpec`, `PlanRequest`, `LegError`, `Leg`, `RoutePoint`, `Stage`, `Totals`, `RoutePlan` exakt nach den Rust-Typen (snake_case-Felder); `api.routePlan(request: PlanRequest)`.

- [ ] **Step 1:** Typen und Befehl ändern; `route.ts` und `diagnostics.ts` vorerst nur so anpassen, dass sie kompilieren (eine Route = alle Teilstücke im gewählten Profil, Anzeige aus `total` und `legs`).
- [ ] **Step 2:** Run: `cargo clippy -p outdoor-desktop --all-targets -- -D warnings && (cd apps/desktop/ui && pnpm exec tsc --noEmit)` – Expected: keine Ausgabe/Fehler.
- [ ] **Step 3:** Bestehenden Routen-Selbsttest laufen lassen (`OUTDOOR_SELFTEST_ROUTE` mit eigenem `OUTDOOR_DATA_DIR`, Region per Symlink oder Kopie) – Expected: Route gezeigt, Statistik wie vorher.
- [ ] **Step 4: Commit** – `git commit -m "desktop: route planning with legs"`

### Task 4: Oberfläche – Teilstücke, Profilwechsel, Etappenmenü

**Files:**
- Modify: `apps/desktop/ui/src/route.ts`, `apps/desktop/ui/src/map.ts` (`setWaypoints`, `WaypointHandlers`), `apps/desktop/ui/src/i18n/de.ts`, `apps/desktop/ui/src/style.css`

**Interfaces:**
- `WaypointHandlers` bekommt `stageToggled(index: number): void`; `setWaypoints(points, stageEnds: number[], handlers)`.
- `RoutePanel` hält `waypoints: LatLon[]`, `legs: LegSpec[]`, `stageEnds: number[]`.

- [ ] **Step 1:** Zustand umbauen:
  - Neuer Punkt → neues Teilstück mit aktuellem Formularprofil und -optionen.
  - Profil-/Reglerwechsel → letztes Teilstück und künftige.
  - „Rückgängig“ → letzter Punkt und letztes Teilstück weg, Etappenende auf dem neuen letzten Punkt entfällt.
  - Mittleren Punkt entfernen → Teilstücke `i-1` und `i` werden eines mit dem Profil von `i-1`, Etappenindizes > i um 1 kleiner, Marke auf i entfällt.
  - Verschieben → nur Position.
  - Laden einer gespeicherten Route → Zustand aus `plan.request`, Formular zeigt das Profil des letzten Teilstücks.
- [ ] **Step 2:** Klick auf einen Wegpunkt öffnet ein kleines MapLibre-Popup mit „Etappenende“ (Umschalter, nicht bei erstem/letztem Punkt) und „Entfernen“. Etappenenden bekommen die Klasse `stage-end` (eigene Form/Farbe).
- [ ] **Step 3:** Fehlerhafte Teilstücke: Meldung „Teilstück n: …“ (Fehlertext wie bisher übersetzt); die übrige Route bleibt.
- [ ] **Step 4:** Run: `cd apps/desktop/ui && pnpm exec tsc --noEmit && pnpm build` – Expected: ok.
- [ ] **Step 5: Commit** – `git commit -m "desktop UI: legs with their own profile, stage ends from the waypoint menu"`

### Task 5: Oberfläche – Farben auf Karte und Höhenprofil, Etappenstatistik

**Files:**
- Create: `apps/desktop/ui/src/profile-colors.ts`
- Modify: `apps/desktop/ui/src/map.ts` (`showRoute`), `apps/desktop/ui/src/profile-chart.ts`, `apps/desktop/ui/src/route.ts` (`renderPlan`), `apps/desktop/ui/src/style.css`, `apps/desktop/ui/src/i18n/de.ts`

**Interfaces:**
- `PROFILE_COLORS: Record<Profile, string>` – Startwerte: `road_bike #1565c0`, `trekking #00897b`, `mtb #e07b00`, `hike #c62828`, `car #546e7a`, `manual #6a1b9a`; Fehler `#d93025` gestrichelt.
- `MapView.showRoute(legs: { line: LatLon[]; color: string; failed: boolean; from: LatLon; to: LatLon }[], fit: boolean)`: GeoJSON mit einem Feature je Teilstück, Farbe aus Eigenschaft; fehlerhafte als gerade Linie von `from` nach `to`, gestrichelt (eigene Ebene mit Filter).
- `renderProfileChart(target, points, onHover, options?: { colorOf(leg: number): string; stageAt: number[] })`: Fläche je zusammenhängendem Bereich gleicher Farbe; senkrechte Linien an den Etappengrenzen (Distanzen).

- [ ] **Step 1:** Farben und Kartenlinie umsetzen.
- [ ] **Step 2:** Höhenprofil farbig, Etappenlinien, darunter eine Legende nur der vorkommenden Profile (Farbfeld + Name, eine Zeile, `#route-legend`).
- [ ] **Step 3:** Statistik: Gesamtwerte aus `total` wie bisher (Zeit-Zeile: „Zeit“ aus `total.time_s`; bei `failed_legs > 0` Hinweis „ohne n fehlerhafte Teilstücke“); bei mehr als einer Etappe darunter je Etappe eine Zeile „Etappe n · 84 km · ↑1 250 m ↓900 m · 6:10 h“ mit kleinen Farbpunkten der Profile.
- [ ] **Step 4:** Run: `pnpm exec tsc --noEmit && pnpm build` – Expected: ok.
- [ ] **Step 5: Commit** – `git commit -m "desktop UI: route and elevation profile coloured by profile, statistics per stage"`

### Task 6: Selbsttest, echte Messung, Doku

**Files:**
- Modify: `apps/desktop/ui/src/diagnostics.ts` (`routeSelfTest`), `docs/MESSPROTOKOLL.md`, `README.md` (Abschnitt Routenplanung), `PLAN.md`

- [ ] **Step 1:** `routeSelfTest` mit echten Mausereignissen erweitern:
  - 3 Punkte in Wandern setzen, Profil auf MTB, 4. Punkt setzen.
  - Wegpunkt 2 anklicken, im Menü „Etappenende“, Popup zu.
  - „Rückgängig“.
  - Berichten: Anzahl Teilstücke je Profil, `stage-end`-Marker, Etappenzeilen, Legendeneinträge, Zahl der gezeichneten Teilstücke auf der Karte, Layoutprobleme.
  - Snapshot des Kartenbilds.
- [ ] **Step 2:** Selbsttest auf der Fixture-Region laufen lassen. Expected: Bericht wie oben; Bild zeigt zwei Farben.
- [ ] **Step 3:** Messung auf `alpenraum-ost`, gelesen aus dem Nutzerordner, mit Kopie oder Symlink in einem eigenen Datenordner, nichts schreiben: ~300 Punkte über ~1 000 km per `examples`-Programm in `core-app` (Zeit für den ganzen Plan kalt, Zeit für +1 Punkt warm, JSON-Größe). Ergebnis ins Messprotokoll; ist die Antwort größer als ~5 MB, Höhenprofil für die Übertragung ausdünnen (eigener Schritt mit Test).
- [ ] **Step 4:** README/PLAN.md: Teilstücke, Etappen, Farben, Limits.
- [ ] **Step 5: Commit** – `git commit -m "Routes with legs and stages: self-test, measurement, docs"`
