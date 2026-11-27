# Tour Schritt 3: Export als Paket Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** In der Detailansicht eines Tracks „Tour exportieren“: Trail lässt den Kern eine ZIP mit `track.gpx`, Fotos, Sprachnotizen, `tour.json` und `notizen.md` bauen und gibt sie über „Teilen“ weiter.

**Architecture:** `core-app` bekommt das Modul `package`: reine Funktionen bauen aus `TrackDetail` und den Segment-IDs das Manifest (`TourManifest`, Serialize + Deserialize, damit Base es später ohne Nachbau einliest) und die lesbare `notizen.md`; `App::export_tour` sammelt die Daten unter der Sperre, gibt sie frei und schreibt dann die ZIP streamend (Fotos/Audio gespeichert, Text komprimiert) über eine `.part`-Datei an den Zielpfad. `core-ffi` reicht `exportTour` durch. Trail baut die ZIP im Hintergrund in einen eigenen Ordner im Temp-Verzeichnis, öffnet das Teilen-Blatt und räumt danach auf.

**Tech Stack:** Rust, `zip`-Crate (neu), serde_json, UniFFI, SwiftUI, `UIActivityViewController`, XCUITest.

**Spec:** `docs/superpowers/specs/2026-11-26-tour-fotos-notizen-export-design.md` („Schritt 3“, „Feste IDs“, Anti-Ziele, „Dem Bauenden überlassen“: Kompression).

## Global Constraints

- Branch `feature/tour-export` (von `main`, 99d5976). Am Ende PR, Merge lokal mit `--no-ff`. Push nur nach Rückfrage.
- **Anhalten und dem Nutzer zeigen:** nach Task 2 (Kern fertig, Linux; danach Push für den Mac nach Rückfrage) und nach Task 3 (iPhone-Test).
- Inhalt der ZIP, flach ohne Wurzelordner: `track.gpx` (wie `export_gpx`, also jeder Abschnitt ein `<trkseg>` mit `zg:section`), `fotos/<media-id>.<ext>`, `notizen/<media-id>.m4a`, `tour.json`, `notizen.md`. Gelöschte Medien fehlen. Textnotizen haben keine Datei.
- **Entscheidung von Claude:** Bilder behalten ihr Format und ihre Endung (`.jpg`, `.heic`, `.png`); der Kern wandelt nichts um und schreibt kein EXIF nach. Kamerafotos haben Zeit und Ort schon im EXIF (Schritt 2); bei Bildern aus der Mediathek steht beides in `tour.json`.
- **Entscheidung von Claude:** Fotos und Audio als `Stored` (schon komprimiert, spart Akku), `track.gpx`, `tour.json`, `notizen.md` als `Deflated`. ZIP64 muss gehen (eine Woche ≈ 1,5 GB, mehrere Wochen > 4 GB).
- Feste IDs: Tour = Track-UUID, Abschnitt = Segment-UUID (`Storage::segment_ids`), Foto/Notiz = Medien-UUID. Zweimal exportiert → dieselben IDs.
- Die laufende Aufnahme wird nicht exportiert (`RecordingActive`); andere Tracks lassen sich während einer Aufnahme exportieren. Der Export hält die Sperre der App nur beim Einsammeln, nie beim Schreiben (die Aufnahme darf nicht stocken).
- Keine Netzwerkaufrufe. Logik im Kern. Die GPX-Weitergabe („Als GPX teilen“) bleibt unverändert.
- Texte wörtlich (Deutsch): Knopf „Tour exportieren“; währenddessen „Paket wird gebaut …“; Fehler „Nicht genug freier Speicher für das Paket.“ bzw. „Das Paket konnte nicht gebaut werden.“; Sportarten in `notizen.md` „Rad“, „MTB“, „Wandern“, „Skitour“, „Keine Angabe“, Rast „Rast“, ohne Abschnitt „Ganze Tour“.
- Swift lässt sich unter Linux nicht bauen. Task 3 auf dem Mac, vorher `apps/trail/scripts/build-core.sh` und `xcodegen generate` in `apps/trail`. Neue Ansichten im OLED-Dunkelmodus prüfen; Knopf ≥ 56 pt hoch. Echte Karten in `data/` nicht anfassen. Jeder Commit endet mit `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. Große Tour (Hunderte Fotos, GB): Speicher bleibt klein (Dateien mit `std::io::copy` streamen, nichts ganz einlesen), die Sperre ist beim Schreiben frei. → Streaming-Prüfung im Review; Test `export_while_recording_other_track` in Task 2.
2. Datei eines Mediums fehlt auf der Platte (liegen gebliebene Einträge, Schritt-2-Lücken): Export gelingt, Eintrag bleibt in `tour.json` mit `file: null`, `notizen.md` sagt „(Datei fehlt)“. → Test in Task 2.
3. Export scheitert mittendrin (Speicher voll, Ziel nicht schreibbar): keine halbe Datei am Ziel, keine `.part` übrig, Code `export_no_space` bzw. `export_failed`; in Trail wird der Temp-Ordner auch nach Abbruch des Teilens gelöscht. → Tests in Task 2 und 3.
4. Track ohne Zeiten (GPX-Import ohne `<time>`), ohne Namen, ohne Punkte aber mit Medien, oder alt (ein Abschnitt, keine Medien): Manifest mit `null`-Zeiten, `notizen.md` mit „–“ und Datum als Titel, ZIP ohne leere Ordner. → Tests in Task 1 und 2.
5. Notiztext mit Zeilenumbrüchen oder Markdown-Zeichen (`#`, `*`, `[`): `notizen.md` bleibt gegliedert (Folgezeilen eingerückt, Text sonst unverändert); `tour.json` gibt den Text exakt zurück. → Test in Task 1.

---

### Task 1: `core-app` — Manifest `tour.json` und `notizen.md`

**Files:**
- Create: `crates/core-app/src/package.rs` (Typen und reine Funktionen; Schreiben der ZIP kommt in Task 2 dazu)
- Modify: `crates/core-app/src/lib.rs` (Modul, Exporte)
- Test: `crates/core-app/tests/package.rs` (neu)

**Interfaces:**
- Consumes: `TrackDetail`, `SectionDetail`, `MediaDetail` (`app.rs`, `media.rs`), `TrackRecord`, `MediaRecord`, `TrackStats`, `Activity`, `SegmentReason`.
- Produces (alle `Serialize, Deserialize, Debug, Clone, PartialEq`, serde snake_case):
  - `pub const PACKAGE_FORMAT: &str = "zerogrid-tour"; pub const PACKAGE_VERSION: u32 = 1;`
  - `pub struct TourManifest { pub format: String, pub version: u32, pub exported_at_ms: i64, pub utc_offset_min: i32, pub tour: TourInfo, pub sections: Vec<SectionInfo>, pub media: Vec<MediaInfo> }`
  - `pub struct TourInfo { pub id: Uuid, pub name: Option<String>, pub activity: Activity, pub activities: Vec<Activity>, pub started_at_ms: Option<i64>, pub ended_at_ms: Option<i64>, pub created_at_ms: i64, pub updated_at_ms: i64, pub rev: i64, pub stats: Option<TrackStats>, pub elevation: Elevation }` — `activities`: verschiedene Sportarten der Abschnitte ohne Rast, in Reihenfolge des ersten Auftretens; `stats` = `dem_stats` falls vorhanden, sonst `record.stats`.
  - `pub enum Elevation { Terrain, Receiver }` (`terrain` wenn `dem_stats` vorhanden).
  - `pub struct SectionInfo { pub id: Uuid, pub index: usize, pub activity: String, pub reason: SegmentReason, pub start_ms: Option<i64>, pub end_ms: Option<i64>, pub stats: TrackStats }` — `activity` = `Activity::as_str()` bzw. `"rest"` bei `reason == Rest`; `start_ms`/`end_ms` = `None` statt 0 ohne Zeiten.
  - `pub struct MediaInfo { pub id: Uuid, pub kind: MediaKind, pub source: MediaSource, pub section_id: Option<Uuid>, pub time_ms: Option<i64>, pub lat: Option<f64>, pub lon: Option<f64>, pub file: Option<String>, pub text: Option<String>, pub transcribed: bool, pub created_at_ms: i64, pub updated_at_ms: i64, pub rev: i64 }` — `file` = Pfad **in der ZIP** (`fotos/<id>.<ext>`, `notizen/<id>.m4a`) oder `None` (Textnotiz, fehlende Datei).
  - `pub fn package_path(media: &MediaRecord) -> Option<String>` (Ziel in der ZIP aus Art, ID und Endung der gespeicherten Datei; Endung klein; ohne Datei `None`).
  - `pub fn tour_manifest(detail: &TrackDetail, segment_ids: &[Uuid], present: &dyn Fn(&MediaRecord) -> bool, exported_at_ms: i64, utc_offset_min: i32) -> TourManifest` — `present` sagt, ob die Datei des Mediums da ist (sonst `file: None`).
  - `pub fn notes_markdown(manifest: &TourManifest) -> String`.

`notizen.md` sieht so aus (Zeiten lokal über `utc_offset_min`, Zahlen mit Dezimalkomma, eine Nachkommastelle bei km, Höhenmeter gerundet; ohne Zeit „–“):

```markdown
# 2026-11-24 10:05 Rad

24.11.2026, 10:05–16:40 · 54,2 km · ↑ 1230 m · Rad, Wandern

## Ganze Tour

- 09:58 · Notiz: Nebel am Start.

## Abschnitt 1 · Rad · 10:05–12:10 · 31,4 km · ↑ 620 m

- 10:42 · Foto: ![Foto 10:42](fotos/0193….jpg)
- 11:03 · Sprachnotiz: „Abzweig verpasst.“ ([Audio](notizen/0193….m4a))
- 11:20 · Sprachnotiz: [Audio](notizen/0193….m4a)

## Rast · 12:10–13:05
```

Titel = Name, sonst Startdatum `dd.MM.yyyy, HH:mm`, sonst „Tour“. „Abschnitt n“ zählt `index + 1` (Rasten mitgezählt, damit n zu `index` passt). Jeder Abschnitt hat eine Überschrift, auch ohne Medien; „Ganze Tour“ nur mit Medien. Medien in Manifest-Reihenfolge (nach Zeit). Textnotiz „Notiz: <Text>“; Folgezeilen eines Textes mit zwei Leerzeichen eingerückt. Fehlende Datei: „Foto (Datei fehlt)“ bzw. „Sprachnotiz (Datei fehlt)“.

- [x] **Step 1: Tests schreiben** (`tests/package.rs`; Tracks über die vorhandenen Fix-Helfer aus `tests/common` aufnehmen, Dateien in `app.media_dir()` anlegen, `TrackDetail` mit `app.track_detail`)
  - `manifest_has_stable_ids` (Tour-ID = Track-ID, `sections[i].id == segment_ids[i]`, Medien-IDs gleich; zweiter Aufruf → gleiche IDs).
  - `manifest_round_trips_json` (`serde_json::to_string` → `from_str` → gleich; `format == "zerogrid-tour"`, `version == 1`).
  - `rest_section_is_rest` (Rast-Abschnitt → `activity == "rest"`, `reason == Rest`; `tour.activities` ohne Rast, `[Bike, Hike]` bei Wechsel Rad → Wandern).
  - `media_points_to_package_files` (Kamerafoto `media/x.jpg` → `fotos/<id>.jpg`; Bild `media/y.HEIC` → `fotos/<id>.heic`; Sprachnotiz → `notizen/<id>.m4a`; Textnotiz → `None`; `section_id` = Segment-UUID bzw. `None` für ganze Tour).
  - `missing_file_is_null` (`present` liefert `false` → `file == None`, Rest des Eintrags da).
  - `untimed_track_has_null_times` (GPX-Import ohne `<time>` → `start_ms`/`end_ms`/`started_at_ms` `None`).
  - `markdown_layout` (Track mit zwei Abschnitten, Rast, Foto, Sprachnotiz mit Umwandlung, Textnotiz ohne Abschnitt, `utc_offset_min = 60` → Ausgabe enthält wörtlich die Zeilen `## Ganze Tour`, `## Abschnitt 1 · Rad · `, `## Rast · `, `- HH:MM · Foto: ![Foto HH:MM](fotos/<id>.jpg)`, `Sprachnotiz: „<Text>“ ([Audio](notizen/<id>.m4a))`; Uhrzeit = UTC + 1 h).
  - `markdown_without_name_or_times` (kein Name, keine Zeiten → Titel „Tour“, Zeiten „–“).
  - `markdown_keeps_multiline_text` (Text `"Zeile 1\n# Zeile 2 *x*"` → `- – · Notiz: Zeile 1\n  # Zeile 2 *x*`; im Manifest exakt der Ausgangstext).
  - `markdown_missing_file` (→ „Foto (Datei fehlt)“).
- [x] **Step 2: Laufen lassen, schlagen fehl** — Run: `cargo test -p core-app --test package` · Expected: Kompilierfehler für die neuen Namen.
- [x] **Step 3: Implementieren** in `package.rs`. Lokale Zeit: `time::UtcOffset::from_whole_seconds(offset * 60)` (`time` als Abhängigkeit von `core-app` aus dem Workspace ergänzen). Sportartnamen als `fn activity_label(Activity) -> &'static str` mit den Texten der Global Constraints.
- [x] **Step 4: Tests laufen lassen** — Run: `cargo test -p core-app` · Expected: alle PASS.
- [x] **Step 5: Commit** — `git commit -m "Describe a tour as manifest and readable notes"`.

---

### Task 2: `core-app` + `core-ffi` — ZIP schreiben

**Files:**
- Modify: `Cargo.toml` (Workspace: `zip`), `crates/core-app/Cargo.toml`, `crates/core-app/src/package.rs` (Schreiben), `crates/core-app/src/error.rs`, `crates/core-ffi/src/track.rs` (`export_tour`)
- Test: `crates/core-app/tests/package.rs`, `crates/core-ffi/tests/ffi.rs`

**Interfaces:**
- Consumes: Task 1; `App::track_detail`, `App::export_gpx`, `Storage::segment_ids`, `Storage::list_media`.
- Produces:
  - `App::export_tour(&self, id: Uuid, dest: &Path, utc_offset_min: i32) -> Result<(), AppError>` — schreibt nach `<dest>.part`, dann `rename` auf `dest` (vorhandene Datei wird ersetzt); bei jedem Fehler `.part` entfernen.
  - `AppError::ExportNoSpace` (Code `export_no_space`, aus `io::ErrorKind::StorageFull`), `AppError::Export(String)` (Code `export_failed`, alle anderen I/O- und ZIP-Fehler). Laufende Aufnahme → vorhandenes `RecordingActive`; unbekannter Track → vorhandenes `NotFound`.
  - FFI (Swift-Name): `exportTour(trackId: String, path: String, utcOffsetMin: Int32) throws`.

- [x] **Step 1: Tests schreiben** (ZIP in Tests mit dem `zip`-Crate lesen; `zip` auch als dev-dependency)
  - `zip_has_expected_entries` (Track mit Kamerafoto, Bild aus der Mediathek, Sprachnotiz, Textnotiz → Einträge genau `track.gpx`, `tour.json`, `notizen.md`, `fotos/<a>.jpg`, `fotos/<b>.heic`, `notizen/<c>.m4a`; Fotos/Audio `CompressionMethod::Stored`, Text `Deflated`).
  - `zip_content_matches_sources` (`track.gpx` == `export_gpx(id)`; Fotobytes gleich der Datei in `media/`; `tour.json` parst als `TourManifest`; `notizen.md` == `notes_markdown(&manifest)`).
  - `deleted_media_is_left_out` (gelöschtes Foto → weder Eintrag noch Datei).
  - `missing_file_still_exports` (Datei in `media/` von Hand gelöscht → Export gelingt, kein `fotos/`-Eintrag, Manifest `file == None`).
  - `plain_track_has_no_media_folders` (alter Track ohne Medien → nur die drei Textdateien).
  - `track_without_points_but_notes_exports` (Aufnahme ohne Fix mit Textnotiz, beendet → Export gelingt).
  - `recording_is_not_exported` (laufende Aufnahme → `recording_active`).
  - `export_while_recording_other_track` (fertiger Track A, dann Aufnahme B läuft, Fixe kommen → `export_tour(A)` gelingt, Aufnahme nimmt danach weiter Fixe an).
  - `failed_export_leaves_nothing` (Ziel in einem nicht vorhandenen Ordner → `export_failed`, weder Ziel noch `.part` vorhanden).
  - `existing_dest_is_replaced` (zweimal auf denselben Pfad → gültige ZIP, keine `.part`).
  - `storage_full_maps_to_no_space` (Umwandlung `io::Error` mit `StorageFull` → `AppError::ExportNoSpace`, Code `export_no_space`; als Unit-Test der Umwandlungsfunktion in `package.rs`).
  - FFI: `export_tour_through_ffi` (Track mit Foto, `export_tour(id, path, 60)` → Datei ist eine ZIP mit `tour.json`); `export_bad_id_has_code`.
- [x] **Step 2: Laufen lassen, schlagen fehl** — Run: `cargo test -p core-app --test package && cargo test -p core-ffi` · Expected: Kompilierfehler für die neuen Namen.
- [x] **Step 3: Implementieren.** `zip` in `[workspace.dependencies]` (aktuelle Version, `default-features = false`, nur das Deflate-Feature über `flate2`). Unter der Sperre nur einsammeln: `track_detail`, `segment_ids`, `list_media`, GPX-Text; dann Sperre frei. Schreiben über `ZipWriter<BufWriter<File>>`, Medien mit `std::io::copy` aus der geöffneten Datei, `large_file(true)` für Einträge über 4 GiB − 1. Ob eine Datei da ist, entscheidet ein Versuch, sie zu öffnen (`present` in Task 1 bekommt dieses Ergebnis). `exported_at_ms` = jetzt.
- [x] **Step 4: Alles prüfen** — Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check` · Expected: alle grün.
- [x] **Step 5: Commit** — `git commit -m "Pack a tour with photos and notes into a ZIP"`.
- [x] **Step 6: Übergabe schreiben** — `docs/superpowers/plans/2026-11-27-uebergabe-tour-schritt3-mac.md` (erledigte Commits, Testzahl, Hinweise für Swift: Zielpfad, Codes, Dauer bei großen Touren, Aufräumen), committen. **Anhalten**, dem Nutzer zeigen; Push nach Rückfrage für die Mac-Sitzung.

---

### Task 3: Trail — „Tour exportieren“ und Teilen

**Files:**
- Modify: `apps/trail/ZeroGridTrail/Recording/TracksList.swift` (`TracksController`: Export, Aufräumen), `Recording/TrackDetailView.swift` (Knopf, Fortschritt, Fehler), `ZeroGridTrailApp.swift` (beim Start `tmp/tours` löschen)
- Create: `apps/trail/ZeroGridTrail/Recording/ShareSheet.swift` (`UIViewControllerRepresentable` um `UIActivityViewController`, meldet das Schließen)
- Test: `apps/trail/ZeroGridTrailTests/TourExportTests.swift` (neu), `apps/trail/ZeroGridTrailUITests/TracksUITests.swift`

**Interfaces:**
- Consumes: `exportTour(trackId:path:utcOffsetMin:)`, Codes `export_no_space`, `export_failed`, `recording_active` (Task 2); `TracksController.fileName(_:)`.
- Produces: `TracksController.exportTour() async` (setzt `exporting: Bool`, danach `exportFile: URL?` oder `message`), `TracksController.finishExport()` (löscht `tmp/tours/<track-id>/`, setzt `exportFile = nil`).

- [x] **Step 1: Tests schreiben**
  - Unit (`TourExportTests`, Ersatz-Kern): `exportWritesIntoOwnTempFolder` (Pfad `tmp/tours/<id>/<Name>.zip`, Name aus `fileName` mit `.zip` statt `.gpx`; `utcOffsetMin` = `TimeZone.current.secondsFromGMT(for: Start) / 60`); `finishExportRemovesFolder`; `noSpaceShowsMessage` (Kern wirft `export_no_space` → „Nicht genug freier Speicher für das Paket.“, Ordner weg); `otherErrorShowsMessage` (→ „Das Paket konnte nicht gebaut werden.“).
  - UI (`TracksUITests`): `testExportTourOpensShareSheet` (Track mit Foto öffnen, „Tour exportieren“ ≥ 56 pt, Tipp → Teilen-Blatt erscheint; schließen → Datei im Temp-Ordner weg, sichtbar über den Teststatus `export=none`); `testNoExportWhileRecording` (Detailansicht der laufenden Aufnahme zeigt den Knopf nicht); `testGpxShareStillThere` („Als GPX teilen“ weiter vorhanden).
- [x] **Step 2: Laufen lassen, schlagen fehl** — Run: `apps/trail/scripts/test.sh -only-testing:ZeroGridTrailTests/TourExportTests -only-testing:ZeroGridTrailUITests/TracksUITests` · Expected: neue FAIL.
- [x] **Step 3: Implementieren.** Knopf „Tour exportieren“ über „Als GPX teilen“, nicht für die laufende Aufnahme. Tipp → Knopf gesperrt, `ProgressView` mit „Paket wird gebaut …“; `exportTour` in `Task.detached` (nie auf dem Main-Thread, große Touren brauchen Sekunden); danach `ShareSheet` als Sheet mit der Datei. Schließen des Blatts (geteilt oder abgebrochen) → `finishExport()`. Beim App-Start `tmp/tours` ganz löschen (falls die App während des Teilens endete). Teststatus um `export=<none|file>` ergänzen.
- [x] **Step 4: Alle Trail-Tests** — Run: `apps/trail/scripts/test.sh` · Expected: alle PASS; Hell/Dunkel ansehen.
- [x] **Step 5: Commit** — `git commit -m "Export a tour as ZIP from the track details"`.
- [ ] **Step 6: Anhalten**, dem Nutzer zum Test auf dem iPhone geben (Tour mit vielen Fotos exportieren, in „Dateien“ sichern und per Kabel/Syncthing auf Linux entpacken, `notizen.md` und `tour.json` ansehen, Fotos mit EXIF); danach Abschlussprüfung des Branchs und PR.
