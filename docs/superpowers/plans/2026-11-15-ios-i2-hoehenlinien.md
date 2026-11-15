# iOS I2: Höhenlinien vorberechnet im Regionspaket – Bauplan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Regionspakete bringen Höhenlinien als `contours.pmtiles` (Vektorkacheln) mit, und
der Kern setzt sie in den Style ein, damit MapLibre Native auf iOS sie zeigt.

**Architecture:** Das neue Werkzeug `tools/contour-tiles` (neben `tools/slope-tiles`) liest
die Terrarium-Kacheln aus `dem.pmtiles`, zieht Isolinien (Marching Squares), kodiert sie als
Mapbox Vector Tiles und schreibt sie mit `core_maps::write_archive`. `core-maps` kennt die
optionale Ebene `Layer::Contours`. `offline_style` bekommt `Contours::Tiles`, und `core-ffi`
wählt das, sobald eine installierte Region Höhenlinien hat. Der Desktop zeichnet weiter selbst
mit `maplibre-contour` und bleibt unverändert.

**Tech Stack:** Rust 2024 (Workspace, `rayon`, `flate2`, `clap` wie `slope-tiles`), Bash
(`scripts/build-region.sh`). MVT-Kodierung von Hand, ohne neue Abhängigkeit.

**Spec:** `docs/superpowers/specs/2026-11-15-ios-app-offline-planung-design.md`, Abschnitt
„Höhenlinien im Paket“. Baut auf I1 Task 1–2 auf (`core_maps::offline_style`,
`crates/core-ffi`, Branch `feature/ios-i1`).

**Wo:** Task 1–5 auf dem **Linux-Rechner**, auf einem eigenen Branch `feature/ios-i2`, der von
`feature/ios-i1` abzweigt. Task 6 auf dem **Mac**, nach I1 Task 8.

## Global Constraints

- Quell-Ebene `contours`, Felder `ele` (ganze Meter, Integer) und `level` (`0` = Nebenlinie, `1` = Hauptlinie). Die Style-Ebenen filtern schon so (`contour-minor`: `level != 1`, `contour-major`/`contour-label`: `level == 1`).
- Zoomstufen **11 bis 15** (der Style hat `"maxzoom": 15`). Abstände wie am Desktop (`apps/desktop/ui/src/map.ts`, `CONTOUR_TILES`): z11 100/500 m, z12 50/250 m, z13 20/100 m, z14 20/100 m, z15 10/50 m.
- `contours.pmtiles` ist **optional** wie `slope.pmtiles`. Ältere Pakete ohne die Datei funktionieren weiter, auf iOS dann ohne Höhenlinien.
- Der Desktop bleibt bei `maplibre-contour`. Keine Änderung an `apps/desktop/ui`.
- Workspace-Lints gelten. Keine neue Abhängigkeit, nur die aus `[workspace.dependencies]`.
- Ein vorhandenes Paket wird mit `--steps contours,manifest` ergänzt, ohne Neubau. Das Manifest behält dabei die Angabe `routing` (Valhalla-Version), auch wenn `routing.json` fehlt.
- Größengrenze: Wird `contours.pmtiles` von `isarwinkel-achensee` größer als **50 % von dessen `dem.pmtiles`** (42,5 MB), dann anhalten und den Nutzer fragen, bevor die großen Pakete ergänzt werden (Kartendaten bis 10 GB auf dem Handy sind sein Rahmen).
- Nach I2 anhalten und dem Nutzer zeigen. Push nur nach einmaliger Rückfrage.

## Review Focus

1. **Linien an Kachelgrenzen:** Eine Linie, die über die Grenze zweier Kacheln läuft, muss nahtlos weitergehen, ohne Lücke oder Versatz. → Task 3 (`lines_continue_across_tile_edges`).
2. **Kachel ohne DEM-Nachbarn** (Rand der Region): Die Linien enden am Rand, es gibt keinen Absturz und keine Linien aus erfundenen Höhen. → Task 3 (`edge_of_region_has_no_invented_lines`).
3. **Flaches Gelände oder Wasser:** keine Linien, keine leeren Kacheln im Archiv. → Task 2 (`flat_grid_has_no_lines`) und Task 3 (`empty_tiles_are_not_written`).
4. **Sattelpunkt** (Mehrdeutigkeit bei Marching Squares): Die Linien bleiben geschlossen bzw. laufen bis zum Rand, keine sich kreuzenden Linien gleicher Höhe. → Task 2 (`saddle_is_resolved_consistently`).
5. **Installiertes Paket ohne `routing.json` ergänzen:** Das Manifest behält `routing`, und die Prüfsummen aller Dateien stimmen. → Task 4 (Prüfung mit `isarwinkel-achensee` und einer Kopie ohne `routing.json`).

---

### Task 1: Ebene `Contours` im Kern und im Style

**Files:**
- Modify: `crates/core-maps/src/manifest.rs` (`Layer`), `crates/core-maps/src/store.rs` (`has_layer`), `crates/core-maps/src/style.rs` (`Contours::Tiles`), `crates/core-app/src/regions.rs` (`map_has_layer`), `crates/core-ffi/src/lib.rs` (`offline_style`)
- Test: `crates/core-maps/tests/style.rs`, `crates/core-maps/tests/store.rs`, `crates/core-ffi/tests/ffi.rs`

**Interfaces:**
- Produces:
  - `Layer::Contours` (`as_str() == "contours"`, `file_name() == "contours.pmtiles"`), in `Layer::ALL` als letzte.
  - `Contours::Tiles`: Die Quelle `contours` bekommt `tiles = ["{tiles_base}contours/{z}/{x}/{y}"]` und `bounds` wie die anderen Quellen.
  - `RegionStore::has_layer(&self, layer: Layer) -> bool`: Mindestens eine installierte Region hat die Datei dieser Ebene.
  - `App::map_has_layer(&self, layer: Layer) -> bool`.
  - `CoreApp::offline_style` nimmt `Contours::Tiles`, wenn `map_has_layer(Layer::Contours)`, sonst `Contours::Omit`. Die Swift-Signatur bleibt gleich.

- [ ] **Step 1: Tests schreiben:**
  - `style.rs` `tiles_contours_point_to_core`: Mit `Contours::Tiles` ist `sources.contours.tiles[0] == "outdoor-tiles://localhost/contours/{z}/{x}/{y}"`, `bounds == BOUNDS`, die drei Contour-Ebenen sind da.
  - `store.rs` `contours_layer_name`: `"contours".parse::<Layer>() == Ok(Layer::Contours)`, `file_name() == "contours.pmtiles"`.
  - `store.rs` `has_layer_follows_installed_files`: Mit der Fixture `mini` (ohne `slope`/`contours`) ist `has_layer(Layer::Basemap)` wahr und `has_layer(Layer::Contours)` falsch. Nach dem Installieren eines Pakets mit einer `contours.pmtiles` (Testarchiv mit einer MVT-Kachel über `write_archive`, im Manifest mit Größe und SHA-256) ist es wahr.
  - `ffi.rs` `style_uses_contour_tiles_when_installed`: dasselbe über `CoreApp`. Ohne Höhenlinien gibt es keine Quelle `contours`, mit ihnen zeigt sie auf `…/contours/{z}/{x}/{y}`.
- [ ] **Step 2:** `cargo test -p core-maps -p core-ffi` → schlägt fehl.
- [ ] **Step 3:** Umsetzen. Kachelanfragen `/contours/z/x/y` laufen danach ohne weitere Änderung über `App::map_tile` (Desktop-Schema und `core-ffi`).
- [ ] **Step 4:** `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings` → grün.
- [ ] **Step 5:** Commit `core: optional contour layer in packages and styles`.

### Task 2: Isolinien und MVT-Kodierung (`tools/contour-tiles`, reine Funktionen)

**Files:**
- Create: `tools/contour-tiles/Cargo.toml` (wie `tools/slope-tiles`, plus `flate2`), `tools/contour-tiles/src/lib.rs`, `tools/contour-tiles/src/isolines.rs`, `tools/contour-tiles/src/mvt.rs`
- Test: `tools/contour-tiles/tests/isolines.rs`, `tools/contour-tiles/tests/mvt.rs`, `tools/contour-tiles/tests/common/mod.rs` (MVT-Decoder nur für Tests)

**Interfaces:**
- Produces:
  ```rust
  /// Minor and major interval in metres at a zoom level (11..=15), see Global Constraints.
  pub fn intervals(z: u8) -> Option<(u32, u32)>;
  /// Elevation samples on a square grid, row-major, `side × side`, NaN = no data.
  pub struct Grid { pub side: usize, pub values: Vec<f32> }
  /// One contour line in grid coordinates (0.0 ..= side-1).
  pub struct Isoline { pub ele: i32, pub points: Vec<[f32; 2]> }
  /// All lines at multiples of `interval`, joined into polylines (closed rings end on their start).
  pub fn isolines(grid: &Grid, interval: u32) -> Vec<Isoline>;
  /// Douglas–Peucker with `tolerance` in the same units as the points.
  pub fn simplify(points: &[[f32; 2]], tolerance: f32) -> Vec<[f32; 2]>;
  /// One MVT layer "contours", extent 4096; `lines` in tile coordinates (0..4096, may exceed by the buffer).
  pub fn encode_tile(lines: &[(i32 /*ele*/, u8 /*level*/, Vec<[i32; 2]>)]) -> Vec<u8>;
  ```
  Marching Squares mit linearer Interpolation an den Kanten. Sattelpunkte werden mit dem
  Mittelwert der vier Ecken entschieden. Zellen mit NaN an einer Ecke liefern nichts. Ein
  Höhenwert genau auf der Schwelle zählt als „darüber“ (`>=`), damit keine doppelten Punkte
  entstehen. MVT: Version 2, Geometrie als `MoveTo`/`LineTo` mit Zickzack-Deltas, Schlüssel
  `ele` und `level` als `sint`/`uint`-Werte. Kodiert nach der Mapbox-Vector-Tile-Spezifikation
  2.1.

- [ ] **Step 1: Tests schreiben:**
  - `intervals_match_desktop`: `intervals(11) == Some((100, 500))`, `12 → (50, 250)`, `13 → (20, 100)`, `14 → (20, 100)`, `15 → (10, 50)`, `10 → None`, `16 → None`.
  - `cone_gives_concentric_rings`: Kegel 33 × 33, Spitze 1000 m in der Mitte, Fuß 0 m am Rand. Mit `interval = 100` gibt es genau je eine geschlossene Linie für 100 … 900 (9 Stück), jeweils `points.first() == points.last()`, und alle Punkte einer Linie liegen im Abstand ±0,6 Zellen vom erwarteten Radius.
  - `flat_grid_has_no_lines`: konstante 500,0 → leer, auch bei `interval = 100` (500 liegt genau auf der Schwelle).
  - `saddle_is_resolved_consistently`: 2 × 2-Zelle mit Ecken 0/10/0/10 diagonal, `interval = 5` → genau zwei Segmente, die sich nicht schneiden.
  - `nan_cells_give_nothing`: Kegel mit einer NaN-Ecke → Linien enden an dieser Zelle, keine Punkte darin.
  - `simplify_keeps_ends_and_drops_collinear`: `[[0,0],[1,0.001],[2,0]]`, Toleranz 0,1 → `[[0,0],[2,0]]`.
  - `mvt.rs` `roundtrip`: `encode_tile` von zwei Linien (`ele` 1200/`level` 1 und 1220/0) wird mit dem Test-Decoder wieder gelesen: Layer `contours`, Version 2, Extent 4096, zwei Features vom Typ LineString, gleiche Punkte und Werte.
- [ ] **Step 2:** `cargo test -p contour-tiles` → schlägt fehl.
- [ ] **Step 3:** Umsetzen.
- [ ] **Step 4:** `cargo test -p contour-tiles && cargo clippy -p contour-tiles --all-targets -- -D warnings` → grün.
- [ ] **Step 5:** Commit `contour-tiles: isolines and vector tile encoding`.

### Task 3: Archiv bauen (`run`, Kommandozeile)

**Files:**
- Modify: `tools/contour-tiles/src/lib.rs`
- Create: `tools/contour-tiles/src/main.rs`
- Test: `tools/contour-tiles/tests/run.rs` (mit `fixtures/regions/mini/dem.pmtiles`)

**Interfaces:**
- Consumes: Task 2. `core_maps::{Archive, decode_dem_tile, write_archive, ArchiveInfo, TileType::Mvt}`.
- Produces: `pub fn run(input: &Path, output: &Path, options: &Options) -> Result<Summary, ContourError>`, mit `Options { min_zoom: u8 /*11*/, max_zoom: u8 /*15*/, threads: Option<usize> }` und `Summary { tiles_written: u64, bytes: u64, max_dem_zoom: u8 }`. Kommandozeile wie `slope-tiles`: `contour-tiles <dem.pmtiles> <contours.pmtiles> [--min-zoom 11] [--max-zoom 15] [--threads N]`, Ausgabe in einer Zeile mit Kacheln, MB und Sekunden.

Je Ausgabekachel `(z, x, y)`: ein Raster von 257 × 257 Höhen über die Kachel plus je 8 Zellen
Rand an allen Seiten (für Linien über die Grenze). Die Höhen kommen bilinear aus den DEM-Kacheln
auf Zoom `min(z, max_dem_zoom)`. Für Zoom über `max_dem_zoom` wird ausgeschnitten und
interpoliert, und Nachbarkacheln werden für den Rand gelesen (wie `slope-tiles`, mit
DEM-Zwischenspeicher). Wo eine Nachbarkachel fehlt, ist der Rand NaN. Linien je Abstand aus
`intervals(z)`. `level = 1`, wenn `ele % major == 0`. Umrechnen auf Extent 4096, dann
`simplify` mit Toleranz 2 (Extent-Einheiten). Kacheln ohne Linien werden nicht geschrieben.
Ausgabe: `write_archive` mit `TileType::Mvt` und den `bounds` des DEM-Archivs.
`write_archive` speichert Kacheln unkomprimiert (Header `tile_compression = none`,
`crates/core-maps/src/writer.rs:185`). Dabei bleibt es zunächst. Reißt die Größengrenze in
Task 5, ist gzip-Kompression der Kacheln der erste Hebel. Dann anhalten und den Nutzer fragen.

Außerdem neue Test-Fixture `fixtures/regions/mini-contours/`: Kopie von `mini` mit
`"id": "mini-contours"`, dazu `contours.pmtiles` aus `contour-tiles` auf `mini/dem.pmtiles`.
Das Manifest bekommt Größe und SHA-256. `mini` selbst bleibt **ohne** Höhenlinien, sonst
schlagen die Tests aus Task 1 fehl, die genau das prüfen. Task 1 `has_layer_follows_installed_files`
und `style_uses_contour_tiles_when_installed` wechseln danach von ihrem Testarchiv auf diese
Fixture.

- [ ] **Step 1: Tests schreiben:**
  - `writes_zooms_11_to_15`: `run(mini/dem.pmtiles, tmp)`, dann `Archive::open(tmp)`: Header-`tile_type == Mvt`, `min_zoom == 11`, `max_zoom == 15`, `bounds` gleich denen des DEM-Archivs, `tiles_written > 0`.
  - `values_follow_intervals`: Jede dekodierte Kachel auf z13 hat nur `ele % 20 == 0`, und `level == 1` genau bei `ele % 100 == 0`. Auf z15 gilt `ele % 10 == 0` bzw. `% 50`.
  - `lines_continue_across_tile_edges`: Zwei auf z14 waagerecht benachbarte Kacheln. Für jeden Punkt einer Linie der linken Kachel bei `x == 4096` gibt es in der rechten Kachel eine Linie gleicher `ele` mit einem Punkt bei `x == 0` und `|Δy| <= 2`.
  - `edge_of_region_has_no_invented_lines`: In Kacheln am Rand der Regionsgrenzen liegt kein Punkt außerhalb `-8*16 .. 4096+8*16` (Rand in Extent-Einheiten), und keine Linie hat eine Höhe außerhalb der Min/Max-Höhe der DEM-Kacheln.
  - `empty_tiles_are_not_written`: Jede Kachel im Archiv hat mindestens ein Feature.
- [ ] **Step 2:** `cargo test -p contour-tiles --test run` → schlägt fehl.
- [ ] **Step 3:** Umsetzen.
- [ ] **Step 4:** `cargo test -p contour-tiles && cargo clippy -p contour-tiles --all-targets -- -D warnings` → grün. Dann `cargo run --release -p contour-tiles -- data/regions/isarwinkel-achensee/dem.pmtiles /tmp/…/contours.pmtiles` → eine Zeile Zusammenfassung, Exit 0.
- [ ] **Step 5:** Fixture `mini-contours` anlegen (siehe oben) und die beiden Task-1-Tests darauf umstellen. `cargo test --workspace` → grün.
- [ ] **Step 6:** Commit `contour-tiles: build contour archives from elevation tiles`.

### Task 4: Schritt `contours` im Bauskript

**Files:**
- Modify: `scripts/build-region.sh` (Kopfkommentar, `STEPS`, `ALL_STEPS`, neue Funktion `step_contours`, `step_manifest`), `README.md` (Liste der Paketdateien und Schritte)

`step_contours` läuft nach `slope` und wie dieser: `stale "$OUT/contours.pmtiles"` → `cargo run
--quiet --release -p contour-tiles -- "$OUT/dem.pmtiles" "$BUILD/contours.pmtiles"` → `mv`.
`step_manifest` nimmt `contours.pmtiles` in die Dateiliste auf, hinter `slope.pmtiles`. Fehlt
`routing.json` in `$OUT`, aber das alte `manifest.json` hat `routing`, dann bleibt dieser Wert
erhalten.

- [ ] **Step 1: Prüfen, dass es heute fehlschlägt:** Kopie von `data/regions/isarwinkel-achensee` ohne `routing.json` nach `/tmp/…/pkg/isarwinkel-achensee`. Dann `scripts/build-region.sh isarwinkel-achensee --package-dir /tmp/…/pkg --steps contours,manifest`. Erwartet: Fehler „unknown step contours“ (oder wie das Skript unbekannte Schritte meldet).
- [ ] **Step 2:** Umsetzen.
- [ ] **Step 3:** Denselben Aufruf wiederholen → Exit 0. `manifest.json` listet `contours.pmtiles` mit `size` und `sha256` (mit `sha256sum` nachrechnen), `routing` ist erhalten (`"valhalla": "3.9.1"`), `version` ist neu, und alle anderen Prüfsummen stimmen weiter.
- [ ] **Step 4:** `cargo test -p region-builder` (dessen Tests rufen das Skript bzw. `fake-build.sh` auf) → grün.
- [ ] **Step 5:** Commit `build-region: contours step, keep routing info when amending a package`.

### Task 5: Pakete ergänzen, messen, festhalten

**Files:**
- Modify: `docs/MESSPROTOKOLL.md`, `PLAN.md` (Stand I2; Abschnitt 2.5 „Höhenlinien“: Phase 2 nutzt `contours.pmtiles`)

- [ ] **Step 1:** `scripts/build-region.sh isarwinkel-achensee --steps contours,manifest`, mit Zeitmessung (`/usr/bin/time -v`). Größe von `contours.pmtiles` gegen 50 % von `dem.pmtiles` prüfen (Global Constraints). **Darüber: anhalten und den Nutzer fragen.**
- [ ] **Step 2:** Den Nutzer bitten, die Desktop-App zu schließen. Dann `alpenraum-ost` ergänzen: `scripts/build-region.sh alpenraum-ost --config ~/.local/share/de.outdoor-app.desktop/region-defs/alpenraum-ost.json --package-dir ~/.local/share/de.outdoor-app.desktop/regions --steps contours,manifest`. Das ändert das installierte Paket an Ort und Stelle (nur `contours.pmtiles` kommt dazu, `manifest.json` wird neu geschrieben).
- [ ] **Step 3:** Desktop-App starten. `alpenraum-ost` steht weiter als installiert da, Karte und Routing gehen. Zur Kontrolle eine Route über `OUTDOOR_SELFTEST_ROUTE` mit den Referenzpunkten aus dem Vorabtest: `ok: true`, Länge der Wanderroute 12 353 m.
- [ ] **Step 4:** In `docs/MESSPROTOKOLL.md`: Dauer und Größe je Paket (Kacheln, MB, Anteil am Paket), Rechner wie bei den bisherigen Messungen.
- [ ] **Step 5:** Commit `I2: contour lines added to the packages, measurements`. Push erst nach Task 6 und Rückfrage.

### Task 6 (Mac, nach I1 Task 8): Höhenlinien auf dem iPhone

**Files:**
- Test: `apps/ios/OutdoorUITests/MapUITests.swift`

- [ ] **Step 1: Test schreiben** `testContoursRendered`: Mit `-importFixture mini-contours` (aus Task 3) sendet `map-status` nach `rendered=1` auch `contours=<n>` mit `n > 0`. `n` ist die Zahl der gerenderten Features der Ebene `contour-major` oder `contour-minor` (`visibleFeatures(in:styleLayerIdentifiers:)`) bei Zoom 13 in der Mitte der Region.
- [ ] **Step 2:** `mini-contours` als Testressource neben `mini` ins Projekt aufnehmen (`project.yml`).
- [ ] **Step 3:** Test ausführen → grün. Das ergänzte `isarwinkel-achensee` per Finder nach `Import` kopieren, auf dem iPhone im Flugmodus prüfen, ob Höhenlinien mit Beschriftung zu sehen sind.
- [ ] **Step 4:** Commit `ios: contour lines from the package`. **Anhalten**, dem Nutzer zeigen, Push erfragen.
