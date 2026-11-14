# Region in der Desktop-App bauen – Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Am Desktop ein Rechteck auf der Karte wählen, benennen und per Knopf als Offline-Region bauen und installieren.

**Architecture:** Ein neues Desktop-Crate `region-builder` wählt Geofabrik-Auszüge, schreibt die Regionsbeschreibung und startet `scripts/build-region.sh` als Prozessgruppe. Das fertige Paket wird über `RegionStore::install_local` geprüft und in den Regionsordner verschoben. Die Tauri-Hülle verdrahtet Commands und das Event `regions:build`; das Frontend bekommt Auswahlmodus und Formular.

**Tech Stack:** Rust 2024 (ureq, serde_json, sha2, thiserror), bash, Tauri 2, TypeScript, MapLibre GL JS.

**Spec:** `docs/superpowers/specs/2026-11-14-region-in-app-bauen-design.md`

## Global Constraints

- `unsafe_code = "forbid"`, clippy verbietet `unwrap`/`expect`/`panic` außerhalb von Tests.
- `region-builder` wird von keinem `core-*`-Crate benutzt; die CI-Prüfungen für Android/iOS bleiben unverändert.
- `build-region.sh` und `build-valhalla-tiles.sh` verhalten sich ohne neue Schalter wie bisher (bis auf die zusätzlichen Zeilen `== step …` auf stderr).
- Feste Werte: `basemap_maxzoom` 14, `dem_maxzoom` 13, Schätzung 40 000 Byte je km², Rückfrage ab 5 GB (5·10⁹ Byte), Mindestkantenlänge 1 km.
- Ordner im Datenordner der App: `region-defs/<id>.json`, `region-packages/<id>/`, `region-build/<id>/build.log`.
- Code und Kommentare englisch, Oberflächentexte deutsch in `i18n/de.ts`, Doku deutsch.
- Branch `feature/region-build`; ein Commit je Task; kein Push ohne Rückfrage.

## Review Focus

1. Name nur aus Sonderzeichen, Umlauten oder Emoji, oder ein schon vergebener Name → gültige, eindeutige `id` (Task 3).
2. Rechteck „rückwärts“ aufgezogen oder winzig → normalisiert bzw. mit `build_area` abgelehnt, nie ein Bau mit west > east (Task 3, Task 7).
3. Kein Netz oder Skriptfehler → Fehler mit Protokollauszug; der Zustand „wird gebaut“ ist danach frei für den nächsten Versuch (Task 4, Task 6).
4. Reste eines abgebrochenen Laufs (`.staging/<id>`, halbes Paket) → nächster Bau und `install_local` funktionieren (Task 2).
5. Skriptausgabe mit ungültigem UTF-8 oder ohne Zeilenende → kein Absturz, Protokoll vollständig (Task 4).

---

### Task 1: Schalter für die Skripte

**Files:**
- Modify: `scripts/build-region.sh`, `scripts/build-valhalla-tiles.sh`
- Modify: `README.md` (Abschnitt „Region bauen“: neue Schalter)

**Interfaces:**
- Produces: `build-region.sh <id> [--config <file>] [--package-dir <dir>] [--refresh-osm]`; Ausgabe nach `<dir>/<id>/`; vor jedem ausgeführten Schritt auf stderr `== step <name> (<n>/<total>)`, `total` = Zahl der gewählten Schritte. `build-valhalla-tiles.sh` kennt `--config` und `--package-dir` ebenso und bekommt sie durchgereicht.

- [ ] **Step 1:** `--config`: `CONFIG` von dort; Abbruch, wenn die `id` in der Datei nicht dem Argument entspricht. `--package-dir`: `OUT="<dir>/$ID"` statt `$DATA/regions/$ID`. `--refresh-osm`: in `step_osm` die zwischengespeicherten Auszüge dieser Region löschen und `region.osm.pbf` als veraltet behandeln. Schrittzeilen in der Schleife am Ende. Kopfkommentar (`usage`) ergänzen.
- [ ] **Step 2:** Verify ohne Schalter: `scripts/build-region.sh fixture-tegernsee --steps manifest` → Exit 0, `data/regions/catalog.json` unverändert bis auf `version`, stderr enthält `== step manifest (1/1)`.
- [ ] **Step 3:** Verify mit Schaltern: `scripts/build-region.sh fixture-tegernsee --config regions/fixture-tegernsee.json --package-dir <scratch>/pkg` → `<scratch>/pkg/fixture-tegernsee/manifest.json` mit fünf Dateien, stderr enthält `== step osm (1/7)` … `== step manifest (7/7)`; `data/regions/` unberührt. Falsche id (`--config regions/isarwinkel-achensee.json`) → Exit ≠ 0.
- [ ] **Step 4:** Commit `Region build scripts: --config, --package-dir, --refresh-osm, step lines`.

### Task 2: `RegionStore::install_local`

**Files:**
- Modify: `crates/core-maps/src/store.rs`
- Test: `crates/core-maps/tests/store.rs`

**Interfaces:**
- Produces: `pub fn install_local(&self, package: &Path) -> Result<InstalledRegion, MapsError>` – liest `package/manifest.json`, hält den `BusyGuard` der Region, prüft Größe und SHA-256 jeder Datei (`MapsError::Checksum`), leert `.staging/<id>`, verschiebt Manifest und Dateien dorthin (`rename`, bei Fehler kopieren und Quelle löschen), `load_region`, `activate`, löscht den Paketordner (Fehler dabei ignorieren). Bei einem Prüffehler bleibt das Paket unberührt.

- [ ] **Step 1: Failing tests** (Paket = `common::catalog_copy(name).join("mini")`):
  - `local_package_is_installed_and_moved`: `install_local` ok, `store.tile(Layer::Basemap, …)` wie in `installed_region_serves_tiles` liefert eine Kachel, Paketordner existiert nicht mehr.
  - `local_package_with_wrong_checksum_is_rejected`: ein Byte an `basemap.pmtiles` anhängen → `Err(MapsError::Checksum { .. })`, `store.installed()` leer, Paketdateien noch da.
  - `local_package_replaces_installed_region`: erst über Katalog installieren, dann Paket mit geänderter `version` (Manifest anpassen) → `installed()[0].manifest.version` ist die neue.
  - `local_install_ignores_leftovers_in_staging`: vorher `.staging/mini/junk.part` anlegen → Installation ok, `junk.part` nicht im Regionsordner.
- [ ] **Step 2:** `cargo test -p core-maps --test store local_` → FAIL (Methode fehlt).
- [ ] **Step 3:** Implementieren; Verschieben als private `fn move_file(from, to, rename: impl Fn(&Path,&Path)->io::Result<()>)`, dazu Modultest `move_file_copies_when_rename_fails` mit einem `rename`, das immer `Err` liefert.
- [ ] **Step 4:** `cargo test -p core-maps` → PASS.
- [ ] **Step 5:** Commit `core-maps: install a locally built region package`.

### Task 3: Crate `region-builder` – Beschreibung, Schätzung, Auszugswahl

**Files:**
- Create: `crates/region-builder/Cargo.toml`, `src/lib.rs`, `src/error.rs`, `src/definition.rs`, `src/extracts.rs`
- Create: `crates/region-builder/tests/definition.rs`, `tests/extracts.rs`, `tests/fixtures/index.json`
- Modify: `Cargo.toml` (workspace dependency `region-builder`)

**Interfaces:**
- Produces:
  - `enum BuildError` mit `code()`: `Prerequisites(Vec<String>)`→`build_prerequisites`, `InvalidArea(String)`→`build_area`, `NoExtract`→`build_no_extract`, `Index(String)`→`build_index`, `Failed { tail: String, log: PathBuf }`→`build_failed`, `Cancelled`→`build_cancelled`, `Busy`→`build_busy`, `UnknownRegion(String)`→`build_unknown_region`, `Io { path, source }`→`build_io`.
  - `struct RegionDefinition { id, name, bbox: [f64; 4], osm: Vec<String>, basemap_maxzoom: u8, dem_maxzoom: u8, attribution: Vec<String> }` (serde, Feldnamen wie `regions/*.json`).
  - `fn slug(name: &str) -> String`: Kleinbuchstaben; `ä→ae ö→oe ü→ue ß→ss`; alles außer `a-z0-9` wird `-`; Folgen zusammengefasst, Ränder abgeschnitten; höchstens 40 Zeichen; leer → `region`.
  - `fn unique_id(slug: &str, taken: &dyn Fn(&str) -> bool) -> String`: `slug`, sonst `slug-2`, `slug-3`, …
  - `fn validate_bbox(bbox: [f64; 4]) -> Result<(), BuildError>`: west < east, south < north, |lon| ≤ 180, |lat| ≤ 85, beide Kanten ≥ 1 km.
  - `fn size_km(bbox) -> (f64, f64)` (Breite auf mittlerer Breite, Höhe), `fn estimated_bytes(bbox) -> u64` = Fläche·40 000.
  - `struct Extract { id: String, url: String, area_km2: f64 }`; `struct GeofabrikIndex`; `GeofabrikIndex::parse(json: &str) -> Result<Self, BuildError>`; `fn select(&self, bbox: [f64; 4]) -> Result<Vec<Extract>, BuildError>`.

Auszugswahl (vom Test nicht eindeutig bestimmt):

```text
points = centres of a 16×16 grid over the bbox
covered(e) = points inside e's (multi)polygon, holes respected
open = points inside at least one extract        (none → NoExtract)
while open not empty:
    pick e maximising |covered(e) ∩ open| / area_km2(e); ties → smaller area
    result += e; open -= covered(e)
```

Der Index ist GeoJSON: je Feature `properties.id`, `properties.urls.pbf`, Geometrie `Polygon` oder `MultiPolygon`; Features ohne `urls.pbf` oder Geometrie werden übergangen.

- [ ] **Step 1: Failing tests `tests/definition.rs`:**
  - `slug("Dolomiten West") == "dolomiten-west"`, `slug("Zillertal – Ötztal (groß)") == "zillertal-oetztal-gross"`, `slug("🏔️") == "region"`, `slug("  ") == "region"`, Länge von `slug(&"a".repeat(100))` == 40.
  - `unique_id("x", &|id| id == "x" || id == "x-2") == "x-3"`.
  - `validate_bbox([11.0, 46.0, 12.0, 47.0])` ok; `[12.0, 46.0, 11.0, 47.0]`, `[11.0, 46.0, 11.001, 47.0]`, `[11.0, 86.0, 12.0, 87.0]` → `InvalidArea`.
  - `estimated_bytes([11.35, 47.35, 11.95, 47.80])` zwischen 80e6 und 100e6 (Testregion, gemessen 90 MB).
  - `RegionDefinition` → JSON → `core_maps`-unabhängig wieder lesbar; Feldnamen `basemap_maxzoom`, `dem_maxzoom`.
- [ ] **Step 2: Failing tests `tests/extracts.rs`** mit `fixtures/index.json` (vier Rechteck-Polygone: `a` lon 10–12/lat 46–48, `b` lon 12–14/lat 46–48, `big` lon 0–20/lat 40–50, `nourl` ohne `urls.pbf` über allem):
  - bbox ganz in `a` → `["a"]`.
  - bbox 11.5–12.5 → `["a", "b"]` (in dieser Reihenfolge egal, als Menge prüfen), nicht `big`.
  - bbox 9.5–10.5 (links nur `big`) → Menge enthält `big`.
  - bbox bei lon 30–31 → `Err(NoExtract)`.
  - bbox halb außerhalb aller Auszüge (lon 19.5–20.5) → `["big"]`.
  - `parse("{")` → `Err(Index(_))`.
- [ ] **Step 3:** `cargo test -p region-builder` → FAIL, dann implementieren, → PASS.
- [ ] **Step 4:** Commit `region-builder: region definitions, size estimate, Geofabrik extract selection`.

### Task 4: `region-builder` – Voraussetzungen, Lauf, `Builder`

**Files:**
- Create: `crates/region-builder/src/prereq.rs`, `src/run.rs`; Modify: `src/lib.rs`
- Create: `crates/region-builder/tests/run.rs`, `tests/builder.rs`, `tests/fixtures/fake-build.sh`

**Interfaces:**
- Consumes: Task 1 (Schalter, Schrittzeilen), Task 3.
- Produces:
  - `struct StepProgress { region: String, step: String, index: u32, total: u32 }` (Serialize, Clone).
  - `struct BuildPlan { width_km: f64, height_km: f64, estimated_bytes: u64, extracts: Vec<String> }` (Serialize).
  - `struct Builder`; `Builder::new(app_data: &Path) -> Self` (Projektordner: `OUTDOOR_REPO`, sonst zur Compilezeit `CARGO_MANIFEST_DIR/../..`; Skript: `OUTDOOR_BUILD_SCRIPT`, sonst `<repo>/scripts/build-region.sh`); `Builder::with_paths(app_data, repo, script)` für Tests.
  - `fn check(&self) -> Vec<String>`: Codes des Fehlenden aus `platform` (nicht Linux), `repo` (Skript fehlt), `bash`, `curl`, `python3`, `java`, `java_version` (< 21), `docker`, `docker_daemon` (`docker info` scheitert), `cargo`.
  - `fn plan(&self, bbox: [f64; 4]) -> Result<BuildPlan, BuildError>` – lädt den Index einmal je `Builder` (`https://download.geofabrik.de/index-v1.json`, im Speicher behalten).
  - `fn define(&self, name: &str, bbox: [f64; 4], taken: &dyn Fn(&str) -> bool) -> Result<RegionDefinition, BuildError>` – schreibt `region-defs/<id>.json`; `taken` gilt zusätzlich zu vorhandenen Beschreibungen. Attribution: `"© OpenStreetMap-Mitwirkende (ODbL)"`, `"© OpenMapTiles"`, Mapterhorn-Zeile (Wortlaut gegen mapterhorn.com prüfen).
  - `fn build(&self, id: &str, refresh: bool, on_step: &mut dyn FnMut(StepProgress), cancel: &AtomicBool) -> Result<PathBuf, BuildError>` – `UnknownRegion` ohne Beschreibung; bei `refresh` zuerst die Auszüge neu wählen und die Beschreibung aktualisieren; ruft `bash <script> <id> --config … --package-dir <app_data>/region-packages [--force --refresh-osm]`; liefert den Paketordner.
  - `fn cleanup(&self, id: &str, definition: bool)` – entfernt `<repo data>/build/<id>` (`OUTDOOR_DATA` beachten), `region-packages/<id>`, `region-build/<id>` und auf Wunsch die Beschreibung; fehlende Ordner sind kein Fehler.
  - Lauf: eigene Prozessgruppe (`CommandExt::process_group(0)`), stdout und stderr zeilenweise (verlustbehaftet dekodiert) ins Protokoll, Zeilen `== step <name> (<n>/<total>)` → `on_step`. Abbruch: `kill -TERM -- -<pgid>` als Kommando (kein `unsafe`), nach 10 s `-KILL`; Ergebnis `Cancelled`. Exit ≠ 0 → `Failed` mit den letzten 15 Zeilen.

- [ ] **Step 1: `fake-build.sh`:** verhält sich nach Umgebungsvariable `FAKE_MODE`: `ok` (zwei Schrittzeilen, kopiert `fixtures/regions/mini` nach `<package-dir>/<id>` und setzt dort die `id` im Manifest nicht um – Tests nutzen die id `mini`), `fail` (Schrittzeile, 20 Zeilen Ausgabe, `exit 3`), `hang` (Schrittzeile, startet `sleep 600` als Kind, wartet), `binary` (gibt Bytes `\xff\xfe` ohne Zeilenende aus, `exit 0`).
- [ ] **Step 2: Failing tests `tests/run.rs` / `tests/builder.rs`:**
  - `steps_are_reported`: `ok` → `on_step` sah `("osm",1,2)` und `("manifest",2,2)`; Rückgabe `…/region-packages/mini`, `manifest.json` liegt dort; `build.log` enthält beide Zeilen.
  - `failure_carries_the_tail_of_the_output`: `fail` → `Failed { tail, log }`, `tail` hat 15 Zeilen und endet mit der letzten Ausgabezeile, `log` existiert.
  - `cancel_stops_the_whole_process_group`: `hang`, `cancel` nach 300 ms gesetzt → `Cancelled` binnen 5 s; kein `sleep 600` mit der Prozessgruppe mehr vorhanden (über `ps -o pgid=` prüfen).
  - `output_need_not_be_text`: `binary` → `Ok`.
  - `unknown_region_is_reported`: `build("nope", …)` → `UnknownRegion`.
  - `define_writes_a_unique_definition`: zweimal derselbe Name → ids `x` und `x-2`, beide Dateien vorhanden (Index dafür über `Builder::with_index(json)` oder vergleichbaren Testzugang vorgeben, kein Netz in Tests).
  - `cleanup_removes_everything_of_a_region`.
  - `check_reports_a_missing_script`: `repo` enthalten, wenn das Skript fehlt.
- [ ] **Step 3:** `cargo test -p region-builder` → FAIL, implementieren, → PASS; `cargo clippy -p region-builder --all-targets -- -D warnings`.
- [ ] **Step 4:** Commit `region-builder: prerequisites, build run with progress and cancel`.

### Task 5: `core-app` – Beschreibungen kennen, lokal installieren

**Files:**
- Modify: `crates/core-app/src/regions.rs`, `crates/core-app/src/app.rs` (Ordner `region-defs` neben der Datenbank)
- Test: `crates/core-app/tests/regions.rs`

**Interfaces:**
- Consumes: Task 2.
- Produces:
  - `RegionState::NotBuilt` (serde `not_built`): Beschreibung vorhanden, nicht installiert, nicht im Katalog.
  - `RegionInfo.buildable: bool` – es gibt `region-defs/<id>.json`. Aus der Datei werden nur `id`, `name`, `bbox` gelesen; unlesbare Dateien werden übergangen. Größe einer `NotBuilt`-Region: 0.
  - `App::install_local_region(&self, package: &Path) -> Result<InstalledRegion, AppError>` (ruft danach `routing.forget_region`).
  - `App::delete_region` entfernt auch die Beschreibung und gelingt für `NotBuilt`-Regionen (nur Beschreibung löschen); ohne beides weiter `NotInstalled`.

- [ ] **Step 1: Failing tests:** `defined_region_is_listed_as_not_built` (Beschreibung anlegen → Zustand `NotBuilt`, `buildable`), `installed_region_with_definition_is_buildable`, `region_without_definition_is_not_buildable`, `local_package_is_installed` (Fixture-Kopie → `Installed`), `deleting_removes_the_definition`, `deleting_a_region_that_was_never_built_removes_its_definition`, `broken_definition_is_ignored`.
- [ ] **Step 2:** `cargo test -p core-app --test regions` → FAIL, implementieren, → PASS (auch `cargo test -p core-app` ohne Features).
- [ ] **Step 3:** Commit `core-app: region definitions in the overview, local package install`.

### Task 6: Tauri-Hülle

**Files:**
- Create: `apps/desktop/src-tauri/src/build.rs`
- Modify: `apps/desktop/src-tauri/src/main.rs`, `commands.rs`, `error.rs`, `Cargo.toml`

**Interfaces:**
- Consumes: Task 4, Task 5.
- Produces (Commands, Namen wie im Frontend benutzt):
  - `region_build_check() -> { missing: string[], running: StepProgress | null }`
  - `region_build_plan(bbox) -> BuildPlan`
  - `region_build_start(name?: string, bbox?: [f64;4], id?: string, refresh: bool) -> InstalledRegion` – ohne `id` zuerst `define` (`taken` = ids aus `app.regions()`); ein Bau zur Zeit (`build_busy`); nach dem Bau `install_local_region`, dann `cleanup(id, false)`; der Zustand wird in jedem Ausgang freigegeben.
  - `region_build_cancel()`
  - Event `regions:build` mit `StepProgress`.
  - `region_delete` ruft zusätzlich `cleanup(id, true)`; während des Baus dieser Region `build_busy`.
  - `selftest_build() -> string | null` aus `OUTDOOR_SELFTEST_BUILD` (`<west>,<south>,<east>,<north>;<name>`).
  - `From<BuildError> for CommandError` (Code aus `code()`).
  - Beim Beenden (`RunEvent::ExitRequested`): Abbruch setzen und höchstens 15 s auf das Ende des Baus warten.

- [ ] **Step 1:** Implementieren; `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` → PASS.
- [ ] **Step 2:** Commit `desktop: commands and events for building regions`.

### Task 7: Frontend

**Files:**
- Modify: `apps/desktop/ui/src/api.ts`, `map.ts`, `regions.ts`, `main.ts`, `diagnostics.ts`, `i18n/de.ts`, `style.css`, `index.html`

**Interfaces:**
- Consumes: Commands und Event aus Task 6; `RegionInfo.buildable`, Zustand `not_built`.
- Produces:
  - `MapView.startAreaSelection(onChange: (bbox: [number, number, number, number] | null) => void): void`, `stopAreaSelection(): void`, `setArea(bbox): void`. Aufziehen mit gedrückter Maustaste (Kartenverschieben dabei aus), danach vier Eck- und vier Kantengriffe; das gemeldete Rechteck ist immer normalisiert (west < east, south < north).
  - Abschnitt „Neue Region“ über dem Katalogfeld: Knopf `#build-pick`, Feld `#build-name`, Zeile `#build-estimate`, Knopf `#build-start`, Hinweis `#build-hint`.
  - Verhalten: „Gebiet wählen“ merkt sich den Stil und schaltet auf `online`; Abbrechen oder Baubeginn stellt ihn wieder her. Nach jeder Änderung des Rechtecks (300 ms entprellt) `region_build_plan` → „ca. B × H km · geschätzt X“. `#build-start` nur aktiv mit Rechteck, nicht leerem Namen, keiner fehlenden Voraussetzung und ohne laufenden Bau. Ab 5·10⁹ Byte Rückfrage (`confirm` des Dialog-Plugins).
  - Liste: laufender Bau als Zeile „Schritt n/t: <Schrittname>“ mit „Abbrechen“; `buildable` installiert → zusätzlich „Neu bauen“ (`refresh: true`); `not_built` → „Bauen“ (`refresh: false`) und „Löschen“. Während eines Baus sind andere Bau-Knöpfe gesperrt.
  - Fehler `build_failed` zeigt die Meldung mit Protokollauszug in einem aufklappbaren Block unter der Zeile; `build_cancelled` als Meldung.
  - Texte `maps.build.*`, `maps.state.not_built`, Schrittnamen `maps.build.step.{osm,basemap,outdoor,dem,slope,routing,manifest}`, Fehlertexte `error.build_*`, Voraussetzungen `maps.build.missing.*`.
  - Selbsttest `OUTDOOR_SELFTEST_BUILD`: öffnet den Auswahlmodus, setzt das Rechteck über `setArea`, trägt den Namen ein, wartet auf die Schätzung, prüft `layoutProblems()`, drückt „Bauen“, wartet auf die installierte Region, gibt `{ ok, id, steps_seen, layout_problems, error }` als JSON aus und beendet die App – nach dem Muster des Regionen-Selbsttests.

- [ ] **Step 1:** Implementieren; `pnpm build` in `apps/desktop/ui` → ohne Fehler.
- [ ] **Step 2: Selbsttest mit Attrappe:** `OUTDOOR_DATA_DIR=<scratch>/d OUTDOOR_BUILD_SCRIPT=crates/region-builder/tests/fixtures/fake-build.sh FAKE_MODE=ok OUTDOOR_SELFTEST_BUILD='11.54,47.57,11.56,47.58;mini' cargo run --release -p outdoor-desktop` → JSON `ok: true`, `layout_problems: []`, `steps_seen: 2`. (Braucht Netz für den Geofabrik-Index.)
- [ ] **Step 3:** Vorhandene Selbsttests (`OUTDOOR_SELFTEST_REGIONS=fixtures/regions`, `OUTDOOR_SELFTEST=<gpx>`) → weiter `ok`, `layout_problems: []`.
- [ ] **Step 4:** Commit `desktop UI: choose an area on the map and build it as a region`.

### Task 8: Echter Lauf und Dokumentation

**Files:**
- Modify: `README.md`, `PLAN.md`, `docs/MESSPROTOKOLL.md`, `docs/DATENQUELLEN.md` (Geofabrik-Index)

- [ ] **Step 1:** Kleine echte Region über den Selbsttest ohne Attrappe bauen (`OUTDOOR_SELFTEST_BUILD='11.60,47.60,11.75,47.70;Testgebiet Lenggries'`, eigenes `OUTDOOR_DATA_DIR`) → `ok: true`, sieben Schritte; Bild mit `OUTDOOR_SNAPSHOT` ansehen.
- [ ] **Step 2:** Abbruch echt prüfen: Bau starten, im Schritt `basemap` bzw. `routing` abbrechen → kein Java-Prozess und kein Container der Gruppe läuft weiter (`docker ps`, `pgrep`). Schlägt das fehl: Container benennen und gezielt stoppen (siehe Spec „Dem Umsetzer überlassen“).
- [ ] **Step 3:** „Ganz Südtirol“ bauen (`10.38,46.22,12.48,47.09`), Dauer, Rohdaten- und Paketgröße messen, ins Messprotokoll; Schätzfaktor (40 000 Byte/km²) bei Abweichung über 30 % nachziehen.
- [ ] **Step 4:** README (Offline-Karten: „Region in der App bauen“, Voraussetzungen, Umgebungsvariablen `OUTDOOR_REPO`, `OUTDOOR_BUILD_SCRIPT`, `OUTDOOR_SELFTEST_BUILD`, Ordnertabelle `crates/region-builder`), PLAN.md (Stand, Vorhaben 2 als nächster Schritt).
- [ ] **Step 5:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `pnpm build` → alles grün. Commit `Region building in the app: measurements and documentation`.
