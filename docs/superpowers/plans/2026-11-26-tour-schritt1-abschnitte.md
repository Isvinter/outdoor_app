# Tour Schritt 1: Aufnahme mit Abschnitten, Rast und Drosselung Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eine Aufnahme in Trail beginnt mit einer Sportart, gliedert sich in Abschnitte je Sportart, erkennt Rasten als eigene Abschnitte und drosselt dabei das GPS in drei wählbaren Stufen.

**Architecture:** `core-track` bekommt Sportart und Beginngrund je `Segment`, eine reine Rasterkennung (`rest.rs`) im `Recorder` und eine Statistik je Abschnitt mit Summe. `core-storage` speichert beides in einer neuen Migration, `core-gpx` schreibt/liest es als Segment-Erweiterung. `core-app`/`core-ffi` bieten Sportwechsel, Rast-Einstellung, Rast-Status und Abschnitte eines Tracks an. Trail zeigt Startauswahl, Sportwechsel, Rasthinweis, Drosselstufe und die Abschnitte in der Detailansicht und drosselt `LocationService` während einer Rast.

**Tech Stack:** Rust, rusqlite, UniFFI, SwiftUI, CoreLocation, XCUITest.

**Spec:** `docs/superpowers/specs/2026-11-26-tour-fotos-notizen-export-design.md` (nur „Schritt 1“ und die zugehörigen Teile von „Entwurf“)

## Global Constraints

- Branch `feature/tour-media` (besteht, Entwurf committet). Am Ende PR, Merge lokal mit `--no-ff`. Push nur nach Rückfrage.
- **Anhalten und dem Nutzer zeigen:** nach Task 3 (Kern fertig, Linux) und nach Task 5 (Trail auf dem Mac).
- Rast: **5 min** innerhalb von **50 m** (Voreinstellung, änderbar). Drosselstufen **Stark** (`kCLLocationAccuracyHundredMeters`), **Mäßig** (`kCLLocationAccuracyNearestTenMeters`, seltener Punkte), **Aus**; Voreinstellung **Stark**; während der Aufnahme umstellbar. Nie drosseln, solange die Führung läuft.
- Rasterkennung ist im Kern **standardmäßig aus**; nur Trail schaltet sie ein. Base bleibt, wie es ist.
- Gespeicherte Punkte werden nie umgeschrieben; der Schnitt fällt beim Erkennen.
- Statistik der Tour = Summe der Abschnitte, jeder mit `StatsParams::for_activity` seiner Sportart. Rast zählt zur Gesamtzeit, nicht zur Bewegungszeit, ohne Länge und Höhenmeter; die Luftlinie vom Rastpunkt zum ersten Punkt danach zählt zur Länge.
- Bestehende Tracks öffnen und exportieren weiter (Abschnitte ohne Sportart = Sportart des Tracks, Grund unbekannt = `start`).
- Keine Netzwerkaufrufe. Logik im Kern, nicht in Swift.
- Texte wörtlich (Deutsch): Sportarten wie heute (`Activity.name`: Rad, MTB, Wandern, Skitour, Keine Angabe); Startauswahl Titel „Aufnahme starten“; Rasthinweis „Rast – GPS gedrosselt“ (bei Stufe Aus: „Rast“); Abschnitt Rast in Listen „Rast {hh:mm}–{hh:mm}“; Einstellungen „Rast nach“ (3, 5, 10, 15 min), „Rastumkreis“ (30, 50, 100 m), „Drosseln bei Rast“ mit „Stark“, „Mäßig“, „Aus“.
- Swift lässt sich unter Linux nicht bauen. Tasks 4–5 auf dem Mac, vorher `apps/trail/scripts/build-core.sh` und `xcodegen generate` in `apps/trail`.
- Neue Ansichten funktionieren im OLED-Dunkelmodus. UI-Tests mit echten Eingaben. Echte Karten in `data/` nicht anfassen. Jeder Commit endet mit `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. Ein einzelner GPS-Sprung während einer Rast beendet sie nicht (zwei aufeinanderfolgende Fixe außerhalb nötig). → Test in Task 1.
2. Fixe mit schlechter Genauigkeit (> Rastumkreis) beim Stehen starten keine Rast und setzen den Rastpunkt nicht zurück. → Test in Task 1.
3. Pause, Sportwechsel oder Weiter während einer Rast: Rast endet sauber, neuer Abschnitt mit dem richtigen Grund, kein doppelter leerer Abschnitt. → Test in Task 1.
4. Eine nach einem Absturz fortgesetzte Aufnahme (`continue_interrupted`) behält Sportart je Abschnitt und die Rast-Einstellung. → Test in Task 3.
5. Alte Datenbank (Schema 3) und alte GPX ohne Erweiterung öffnen mit sinnvollen Abschnitten. → Tests in Task 2.

---

### Task 1: `core-track` — Abschnitte mit Sportart, Rasterkennung, Statistik je Abschnitt

**Files:**
- Modify: `crates/core-track/src/model.rs` (Segment), `crates/core-track/src/recorder.rs`, `crates/core-track/src/stats.rs`, `crates/core-track/src/lib.rs`
- Create: `crates/core-track/src/rest.rs`
- Modify: alle `Segment { points }`-Literale auf `Segment { points, ..Default::default() }` (u. a. `core-app/src/routes.rs`, `core-storage/src/storage.rs`, `sensor-sim/src/synthetic.rs`, Tests in `core-app`, `core-gpx`, `core-storage`, `core-track`)
- Test: `crates/core-track/tests/recorder.rs`, `crates/core-track/tests/rest.rs` (neu), `crates/core-track/tests/stats.rs`

**Interfaces:**
- Produces:
  - `pub enum SegmentReason { Start, ActivityChange, Resume, Gap, Rest, RestEnd, Continue }` (`Default = Start`, serde snake_case, `as_str()`/`FromStr` wie `Activity`).
  - `Segment { points: Vec<TrackPoint>, activity: Option<Activity>, reason: SegmentReason }`; `Segment::is_rest(&self) -> bool` (= `reason == Rest`); `Track::segment_activity(&self, i: usize) -> Activity` (Segment-Sportart, sonst Track-Sportart, sonst `Other`).
  - `pub struct RestParams { pub dwell_ms: i64, pub radius_m: f64 }`, `RestParams::DEFAULT` = 300 000 ms, 50 m.
  - `pub struct RestDetector` mit `new(RestParams)`, `offer(&mut self, p: &TrackPoint) -> Option<RestEvent>`, `reset(&mut self)`, `resting(&self) -> bool`; `pub enum RestEvent { Started, Ended }`.
  - `Recorder::set_rest_detection(&mut self, params: Option<RestParams>)`, `Recorder::switch_activity(&mut self, a: Activity)`, `Recorder::resting(&self) -> bool`, `Recorder::activity(&self) -> Activity` (des laufenden Abschnitts). `break_segment` bekommt den Grund: `break_segment(&mut self, reason: SegmentReason)`.
  - `pub fn compute_section_stats(track: &Track, params_of: impl Fn(Activity) -> StatsParams) -> (TrackStats, Vec<TrackStats>)` — Gesamt und je Abschnitt. `compute_stats` bleibt (für Aufrufer mit einer Sportart).

- [ ] **Step 1: Tests schreiben**
  - `rest.rs`: `starts_after_dwell_within_radius` (Fixe alle 10 s innerhalb 20 m; bei t = 300 s `Started`, vorher `None`); `moving_resets_anchor` (bei t = 200 s 80 m weg → danach erst 300 s später `Started`); `inaccurate_fixes_neither_start_nor_reset` (`h_accuracy_m = 120` → nie `Started`, Rastpunkt bleibt); `ends_after_two_fixes_outside_radius_plus_accuracy` (Rast; ein Fix 200 m weg mit Genauigkeit 100 → `None`; Fix bei 60 m mit Genauigkeit 100 zählt nicht als weg; zwei aufeinanderfolgende Fixe 200 m weg mit Genauigkeit 30 → beim zweiten `Ended`); `single_jump_does_not_end_rest` (weg, zurück, weg → kein `Ended`).
  - `recorder.rs`: `switch_activity_starts_segment` (Punkte, `switch_activity(Mtb)`, Punkte → 2 Segmente, Sportarten `[Bike, Mtb]`, Grund `[Start, ActivityChange]`); `rest_becomes_own_segment` (mit `RestParams::DEFAULT`: 6 min Stehen, dann Weiterfahren → Gründe `[Start, Rest, RestEnd]`, Rast-Segment beginnt mit dem Fix, bei dem erkannt wurde, Sportart nach Rast = davor); `rest_off_by_default` (gleiche Fixe ohne `set_rest_detection` → 1 Segment); `pause_during_rest_ends_it` (Rast, `pause`, `resume`, Fix → `resting() == false`, letzter Grund `Resume`); `switch_during_rest_ends_it` (Grund `ActivityChange`, `resting() == false`); `switch_before_first_point_sets_first_segment` (kein leeres Segment, erstes Segment hat neue Sportart).
  - `stats.rs`: `sections_use_their_own_params` (ein Hike- und ein Bike-Abschnitt mit identischen Punkten → Bewegungszeiten gleich denen von `compute_stats` mit dem jeweiligen Profil, Gesamt = Summe); `rest_counts_total_time_only` (Rast-Abschnitt 60 min: Gesamtzeit +3600 s, Bewegungszeit, Länge, Aufstieg unverändert); `rest_end_adds_straight_line` (Rastpunkt → erster Punkt nach Rast 120 m → Länge +120 m ±1).
- [ ] **Step 2: Laufen lassen, schlagen fehl** — Run: `cargo test -p core-track` · Expected: Kompilierfehler für die neuen Namen.
- [ ] **Step 3: Implementieren.** Rasterkennung: Rastpunkt = erster brauchbarer Fix (Genauigkeit ≤ Radius oder unbekannt) nach Zurücksetzen; Fix weiter als Radius → neuer Rastpunkt; `Started`, sobald ein brauchbarer Fix ≥ `dwell_ms` nach dem Rastpunkt innerhalb liegt. In Rast: „weg“ = Abstand > Radius + Genauigkeit (fehlt sie: + 0); zwei „weg“ hintereinander → `Ended`, ein Fix dazwischen innerhalb setzt den Zähler zurück. Der `Recorder` fragt den Detektor in `add_point` vor dem Anhängen: `Started` → neues Segment `Rest` mit diesem Fix; `Ended` → neues Segment `RestEnd` mit diesem Fix. `pause`, `resume`, `switch_activity`, `break_segment` rufen `reset()`. Statistik: Rast-Segment liefert nur `total_time_s`; für `RestEnd` die Luftlinie vom letzten Punkt des Rast-Segments zum ersten eigenen Punkt zur Länge addieren; Summe: Summen, `max_speed` als Maximum, min/max Höhe, Mittelwerte neu.
- [ ] **Step 4: Tests laufen lassen** — Run: `cargo test -p core-track && cargo test --workspace --no-run` · Expected: alle PASS, Workspace kompiliert (Literale angepasst).
- [ ] **Step 5: Commit** — `git commit -m "Record sections with their own activity and detect rests"`.

---

### Task 2: `core-storage` und `core-gpx` — Abschnitte speichern und als GPX schreiben

**Files:**
- Modify: `crates/core-storage/src/migrations.rs` (Migration 4), `crates/core-storage/src/storage.rs`
- Modify: `crates/core-gpx/src/write.rs`, `crates/core-gpx/src/read.rs`
- Test: `crates/core-storage/tests/storage.rs`, `crates/core-gpx/tests/gpx.rs`

**Interfaces:**
- Consumes: `Segment`, `SegmentReason` (Task 1).
- Produces: `Storage::begin_segment(&mut self, track: Uuid, activity: Activity, reason: SegmentReason, now_ms: i64) -> Result<Uuid, StorageError>`; `Storage::set_segment_activity(&mut self, track: Uuid, seq: usize, activity: Activity, now_ms: i64) -> Result<(), StorageError>` (erhöht `tracks.rev`, setzt `updated_at`); `load_track` füllt `activity`/`reason`.
- GPX: je `<trkseg>` ein `<extensions><zg:section activity="bike" reason="rest"/></extensions>`, Namensraum `xmlns:zg="urn:zerogrid:gpx:1"` im Wurzelelement; Lesen ohne Erweiterung → `activity: None`, `reason: Start`.

- [ ] **Step 1: Tests schreiben** — `migration_4_keeps_old_segments` (Datenbank mit Schema 3 anlegen wie vorhandene Migrationstests, migrieren → `load_track` liefert Segmente mit `activity: None`, `reason: Start`); `segments_keep_activity_and_reason` (Rundlauf); `set_segment_activity_changes_one_and_bumps_rev`; GPX: `sections_round_trip` (Schreiben → Lesen gibt Sportart und Grund je Segment zurück; Ausgabe enthält `xmlns:zg="urn:zerogrid:gpx:1"`); `plain_gpx_has_no_sections` (vorhandene Fixture ohne Erweiterung).
- [ ] **Step 2: Laufen lassen, schlagen fehl** — Run: `cargo test -p core-storage -p core-gpx` · Expected: FAIL.
- [ ] **Step 3: Implementieren.** Migration 4: `ALTER TABLE track_segments ADD COLUMN activity TEXT; ALTER TABLE track_segments ADD COLUMN reason TEXT;` (NULL bei alten). Unbekannte Werte beim Lesen wie NULL behandeln. `import_track` speichert die Segmentwerte mit. Bestehende Aufrufer von `begin_segment` (in `core-app`) vorerst mit Track-Sportart und `Start` bedienen; Task 3 setzt die echten Werte.
- [ ] **Step 4: Tests laufen lassen** — Run: `cargo test -p core-storage -p core-gpx -p core-app` · Expected: alle PASS.
- [ ] **Step 5: Commit** — `git commit -m "Store and export the activity and reason of each section"`.

---

### Task 3: `core-app` und `core-ffi` — Aufnahme mit Sportwechsel und Rast, Abschnitte eines Tracks

**Files:**
- Modify: `crates/core-app/src/app.rs`, `crates/core-ffi/src/track.rs`, `crates/core-ffi/src/lib.rs`, `docs/ALGORITHMEN.md` (Abschnitt 5/6b ergänzen: Rasterkennung, Statistik je Abschnitt)
- Test: `crates/core-app/tests/app.rs`, `crates/core-ffi/tests/ffi.rs`

**Interfaces:**
- Consumes: Task 1 und 2.
- Produces (Rust, `App`): `switch_recording_activity(&self, a: Activity) -> Result<RecordingStatus, AppError>`; `set_rest_detection(&self, params: Option<RestParams>)` (gilt sofort, auch für eine laufende und eine fortgesetzte Aufnahme; standardmäßig `None`); `set_section_activity(&self, id: Uuid, index: usize, a: Activity) -> Result<TrackDetail, AppError>` (für einen Rast-Abschnitt oder einen Index außerhalb: neue Variante `AppError::InvalidSection`, Code `invalid_section`); `RecordingStatus` + `resting: bool`, `activity` = Sportart des laufenden Abschnitts; `TrackDetail` + `sections: Vec<SectionDetail>` mit `index, activity, reason, start_ms, end_ms, stats: TrackStats`. `set_track_activity` setzt alle Nicht-Rast-Abschnitte und `tracks.activity`. Statistik überall über `compute_section_stats`; `tracks.activity` = Sportart des ersten Abschnitts.
- Produces (FFI, Swift-Namen): `switchRecordingActivity(activity: String) -> RecordingInfo`; `setRestDetection(settings: RestSettings?)` mit `RestSettings { dwellS: UInt32, radiusM: Double }`; `setSectionActivity(id: String, index: UInt32, activity: String) -> TrackView`; `RecordingInfo.resting: Bool`; `TrackView.sections: [TrackSection]` mit `index: UInt32, activity: String, rest: Bool, startMs: Int64, endMs: Int64, distanceM: Double, movingS: Double, ascentM: Double?`.

- [ ] **Step 1: Tests schreiben** (`core-app/tests/app.rs`, mit den vorhandenen Fix-Helfern)
  - `switch_activity_during_recording` → Status-Sportart wechselt, gespeicherter Track hat zwei Abschnitte mit beiden Sportarten; Punktabstand folgt der neuen Sportart (`location_interval_ms`).
  - `rest_detection_off_unless_set` und `rest_detection_marks_rest` (`set_rest_detection(Some(DEFAULT))`, 6 min Stehen → `resting == true` im Status; Weiterfahren → `false`; `track_detail.sections` mit Rast in der Mitte).
  - `continued_recording_keeps_sections_and_rest_setting` (Aufnahme abbrechen wie in den vorhandenen Absturz-Tests, `continue_interrupted` → neue Punkte in Abschnitt `Continue` mit der letzten Sportart; Rasterkennung weiter aktiv).
  - `set_track_activity_skips_rest` und `set_section_activity_changes_one` (Rast-Abschnitt → Fehler).
  - FFI: `recording_info_has_resting` und `track_view_has_sections` (Werte werden durchgereicht).
- [ ] **Step 2: Laufen lassen, schlagen fehl** — Run: `cargo test -p core-app --test app && cargo test -p core-ffi` · Expected: FAIL.
- [ ] **Step 3: Implementieren.** `ActiveRecording.activity`/`params` folgen dem Sportwechsel; `record()` legt Datenbank-Segmente mit Sportart und Grund des Recorder-Segments an; die GPS-Lücke über `GAP_MS` bricht mit `SegmentReason::Gap`; `continue_interrupted` mit `Continue` und der Sportart des letzten Abschnitts. Status sendet bei Rastbeginn/-ende sofort (nicht erst nach `UPDATE_INTERVAL`), damit Trail rechtzeitig drosselt.
- [ ] **Step 4: Tests, Lints** — Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check` · Expected: alle grün.
- [ ] **Step 5: Commit** — `git commit -m "Switch activities and mark rests while recording"`.
- [ ] **Step 6: Anhalten**, dem Nutzer zeigen; Push nach Rückfrage für die Mac-Sitzung.

---

### Task 4: Trail — Start mit Sportart, Sportwechsel, Rast und Drosselung

**Files:**
- Modify: `apps/trail/ZeroGridTrail/Recording/RecordingController.swift`, `RecordingBar.swift`, `apps/trail/ZeroGridTrail/Map/MapScreen.swift` (Startauswahl), `apps/trail/ZeroGridTrail/Map/MapToolbar.swift` (Einstellungen), `apps/trail/ZeroGridTrail/Location/LocationService.swift`, App-Verdrahtung in `ZeroGridTrailApp.swift`
- Test: `apps/trail/ZeroGridTrailUITests/RecordingUITests.swift`, `apps/trail/ZeroGridTrailTests/RecordingControllerTests.swift`, `LocationServiceTests.swift`

**Interfaces:**
- Consumes: FFI aus Task 3.
- Produces: `RecordingController.start(activity: String)`, `switchActivity(_:)`, `@Published throttle: RestThrottle` (`enum RestThrottle: String { strong, moderate, off }`, `@AppStorage`-Schlüssel `restThrottle`, Voreinstellung `strong`); `LocationProfile.restStrong` (`kCLLocationAccuracyHundredMeters`, `distanceFilter` 25 m) und `.restModerate` (`kCLLocationAccuracyNearestTenMeters`, `distanceFilter` 15 m); `LocationService.restProfile: LocationProfile?` (gesetzt → gilt statt `standard`, solange keine `.guidance`-Nachfrage besteht). Schlüssel `restDwellMin` (Voreinstellung 5), `restRadiusM` (50), `lastRecordingActivity`.

- [ ] **Step 1: Tests schreiben**
  - Unit: `restProfileOnlyWithoutGuidance` (LocationService mit Ersatz-Manager: `restProfile = .restStrong` → `desiredAccuracy == kCLLocationAccuracyHundredMeters`; `require(.guidance)` → `kCLLocationAccuracyBest`); `throttleFollowsRestingAndLevel` (RecordingController: `resting` an + `strong` → `.restStrong`; `off` → `nil`; `resting` aus → `nil`).
  - UI (`RecordingUITests`, mit GPX-Ersatz wie die vorhandenen): `testStartAsksForActivity` (Aufnahmeknopf → „Aufnahme starten“ mit Sportarten, zuletzt genutzte gewählt; Tipp auf „MTB“ startet, Leiste zeigt „MTB“); `testSwitchActivityInBar` (Menü „MTB ▾“ → „Wandern“ → Status zeigt zwei Abschnitte); `testSaveWithoutActivityChoice` („Beenden“ → nur „Speichern“/„Verwerfen“, keine Sportart-Chips; Name mit der Startsportart); `testRestHintShows` (Ersatz-GPX mit 6 min Stillstand, beschleunigt → „Rast – GPS gedrosselt“, danach verschwunden); `testThrottleLevelInBar` (Menü → „Aus“ → Hinweis „Rast“).
- [ ] **Step 2: Laufen lassen, schlagen fehl** — Run: `apps/trail/scripts/test.sh -only-testing:ZeroGridTrailTests/RecordingControllerTests -only-testing:ZeroGridTrailTests/LocationServiceTests -only-testing:ZeroGridTrailUITests/RecordingUITests` · Expected: neue FAIL.
- [ ] **Step 3: Implementieren.** Startauswahl als Panel über der Karte wie `SaveTrackPanel` (Chips, Tipp startet; Titel „Aufnahme starten“, Abbrechen). `SaveTrackPanel` ohne Chips. Leiste: Sportart-Menü rechts oben, Rasthinweis-Zeile, Drosselstufe als Menü (gleicher `@AppStorage`-Schlüssel wie in den Einstellungen). Einstellungen im Menü `map-settings`: Picker „Rast nach“, „Rastumkreis“, „Drosseln bei Rast“. Trail ruft beim Start der App und bei Änderung `setRestDetection` mit den Werten. `location.activity` folgt der laufenden Sportart.
- [ ] **Step 4: Tests laufen lassen** — gleicher Befehl · Expected: alle PASS.
- [ ] **Step 5: Commit** — `git commit -m "Start recordings with an activity and throttle GPS during rests"`.

---

### Task 5: Trail — Abschnitte in der Detailansicht

**Files:**
- Modify: `apps/trail/ZeroGridTrail/Recording/TrackDetailView.swift`, `TracksList.swift` (Controller), ggf. `TrackLayers.swift` (Rast-Abschnitte gestrichelt oder blasser zeichnen – dem Bauenden überlassen)
- Test: `apps/trail/ZeroGridTrailUITests/TracksUITests.swift`

**Interfaces:**
- Consumes: `TrackView.sections`, `setSectionActivity` (Task 3).
- Produces: `TracksController.setSectionActivity(index: Int, activity: String)`.

- [ ] **Step 1: UI-Tests schreiben** — `testDetailListsSections` (Track mit zwei Sportarten und einer Rast aus einer Fixture-Aufnahme → drei Zeilen, Rastzeile „Rast hh:mm–hh:mm“); `testChangeSectionActivity` (Menü der ersten Zeile → „Wandern“ → Zeile und Gesamtwerte aktualisiert); `testRestSectionHasNoActivityMenu`.
- [ ] **Step 2: Laufen lassen, schlagen fehl** — Run: `apps/trail/scripts/test.sh -only-testing:ZeroGridTrailUITests/TracksUITests` · Expected: neue FAIL.
- [ ] **Step 3: Implementieren.** Zeile je Abschnitt: Farbpunkt/Sportart als Menü (nicht bei Rast), Uhrzeit von–bis, Länge, Bewegungszeit, Aufstieg. Mit nur einem Abschnitt keine Liste.
- [ ] **Step 4: Alle Trail-Tests** — Run: `apps/trail/scripts/test.sh` · Expected: alle PASS; Hell/Dunkel kurz ansehen.
- [ ] **Step 5: Commit** — `git commit -m "Show and edit the sections of a track"`.
- [ ] **Step 6: Anhalten**, dem Nutzer zum Test auf dem iPhone geben (Rast auf einer echten Tour, Akku beobachten).
