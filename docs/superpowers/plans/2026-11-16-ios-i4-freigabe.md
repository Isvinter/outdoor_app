# iOS I4: Freigabe im Heimnetz, Übernahme aufs iPhone, Kartenverwaltung – Bauplan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Regionen und Routen kommen ohne Kabel und ohne Finder vom Desktop aufs iPhone:
QR-Code scannen, Regionen laden, Routen übernehmen. Auf dem iPhone verwaltet ein
Kartenbildschirm die Regionen: laden, aktualisieren, löschen, **aktivieren und deaktivieren**.

**Architecture:**
- Ein neues Crate `crates/core-share` enthält den Webserver. Er liefert nur aus, auf einem
  eigenen Thread mit `tiny_http`. Was er ausliefert, kommt aus `core-app` (Katalog aus den
  installierten Regionen, Dateien, Routen). Der Desktop schaltet ihn ein und aus und zeigt
  Adresse und QR-Code.
- Die Übernahme liegt ganz im Kern. Das iPhone liest Katalog und Routen über den
  vorhandenen `DefaultFetcher` (ureq), mit Prüfsummen und Fortsetzen per `Range`. Die
  Merge-Regel für Routen steht in `core-app`, und Swift zeigt nur an.
- Aktivieren und Deaktivieren liegt in `RegionStore`. Eine deaktivierte Region bleibt auf der
  Platte, liefert aber keine Kacheln, keine Höhen und kein Routing. Sie zählt auch nicht zu
  den Grenzen und zur Attribution der Karte.
- Ein kleines Programm `share-serve` startet denselben Server ohne Desktop-App. Damit laufen
  die UI-Tests auf dem Mac über echtes HTTP.

**Tech Stack:**
- Rust 2024: `core-maps`, `core-storage`, `core-app`, neues `core-share` (`tiny_http`,
  `qrcode`, `if-addrs`) und `core-ffi` mit UniFFI 0.32.2.
- Tauri-Desktop (TypeScript).
- Swift 5.9 mit SwiftUI und VisionKit (`DataScannerViewController`). Mindestversion iOS 16.4.

**Spec:** `docs/superpowers/specs/2026-11-15-ios-app-offline-planung-design.md`. Maßgeblich
sind Ziele 4–5, „Freigabe im Heimnetz (Desktop)“, „Übernahme auf dem iPhone“ und Meilenstein
I4. Dazu kommt der Wunsch des Nutzers vom 2026-11-16: Kartenpakete in der App aktivieren,
deaktivieren und löschen. Baut auf I1–I3 auf (in `main`, 48b0eca).

**Wo:**
- Task 1–6 laufen auf dem **Linux-Rechner**, auf dem Branch `feature/ios-i4` von `main`.
- Nach Task 6 wird gepusht, nach einmaliger Rückfrage beim Nutzer.
- Task 7–10 laufen auf dem **Mac** auf demselben Branch.
- Vor Task 7 auf dem Mac `apps/ios/scripts/build-core.sh` ausführen, weil sich die
  UniFFI-API ändert.

**Abweichungen vom Entwurf (meine Wahl, hier festgehalten):**
- Die Tabelle `routes` bekommt die Spalte `source_rev`: den `rev` des Desktops bei der
  letzten Übernahme. Mit `rev`/`synced_rev` allein ließe sich „aktuell“ nicht von „am Desktop
  neuer“ unterscheiden.
- Der Firewall-Hinweis steht immer unter der Adresse. Ob der Port von außen offen ist, kann
  die App von innen nicht feststellen.
- Die Kopie „(Desktop)“ ist eine gewöhnliche iPhone-Route ohne Verbindung zum Desktop.
  Übernimmt man die Route danach noch einmal, während die iPhone-Fassung weiter geändert ist,
  entsteht eine weitere Kopie. So geht nichts verloren.

## Stand nach Task 1–6 (Linux, 2026-11-16) – für die Mac-Sitzung

Task 1–6 sind fertig (Branch `feature/ios-i4`). Vor Task 7 `apps/ios/scripts/build-core.sh`
ausführen, die UniFFI-API ist neu. Bitte so übernehmen:

- **Swift-API** (aus den erzeugten Bindings, zur Probe auf Linux erzeugt):
  - `CoreListener` hat eine neue Methode `onRegionProgress(region:done:total:)` (`UInt64`,
    Bytes). **`CoreHost` in `apps/ios/Outdoor/Core/CoreHost.swift` muss sie umsetzen**, sonst
    baut die App nicht. Sie kommt von dem Thread, der `installRegion` ausführt, höchstens alle
    paar hundert Millisekunden und beim Ende immer mit `done == total`.
  - `CoreApp`:
    - `catalogSource() throws -> String` und `setCatalogSource(source:) throws -> String`
    - `regions() throws -> RegionList`: **blockiert** und liest den Katalog, also nur auf einem
      Hintergrund-Thread und nur, wenn der Kartenbildschirm geöffnet oder aktualisiert wird.
    - `installRegion(id:) throws -> RegionSummary`: **blockiert**, Fortschritt über den Listener
    - `cancelRegionDownload(id:)`, `deleteRegion(id:) throws`, `setRegionEnabled(id:enabled:) throws`
    - `remoteRoutes() throws -> [RemoteRouteEntry]` (blockiert, liest über das Netz)
    - `importRoute(id:) throws -> ImportedRoute`
  - Typen:
    - `RegionSummary { id, name, version, enabled: Bool, bytes: UInt64 }` (neu: `enabled`, `bytes`)
    - `RegionEntry { id, name, size: UInt64, version, installedVersion: String?, state, enabled }`
    - `RegionList { source, catalogError: String?, regions: [RegionEntry] }`
    - `RemoteRouteEntry { id, name, profile, distanceM: Double, ascentM: Double?, rev: Int64, state }`
    - `ImportedRoute { outcome, id }`
  - Zustände als Strings:
    - `RegionEntry.state`: `available`, `installed`, `update_available`, `downloading`
      (`not_built` gibt es auf dem iPhone nicht)
    - `RemoteRouteEntry.state`: `new`, `up_to_date`, `desktop_newer`, `changed_here`, `deleted_here`
    - `ImportedRoute.outcome`: `added`, `replaced`, `restored`, `copied`
- **Start ohne Netz:** `overview()` und `offlineStyle(...)` lesen den Katalog **nie** mehr
  (`App::map_overview()`), auch nicht bei gesetzter, toter Quelle. `MapOverview.bounds` und
  `attribution` zählen nur aktivierte Regionen. `overview().regions` enthält alle
  installierten, auch die deaktivierten (`enabled == false`).
- **Adresse:** `setCatalogSource` normalisiert im Kern (nur FFI, der Desktop nimmt weiter auch
  Ordner): Ohne `://` kommt `http://` davor. Ohne `/` am Ende kommt `/` dazu, außer der Pfad
  endet auf `.json`. Leer (auch nur Leerzeichen) heißt keine Quelle. Der Rückgabewert ist das
  Gespeicherte, das die Oberfläche anzeigt. Swift normalisiert nichts.
- **Fehlercodes** (neu zu übersetzen): `download`, `checksum`, `busy`, `no_catalog`,
  `route_import`. `no_catalog` heißt: keine Quelle gesetzt.
- **Rulings aus den Linux-Tasks:**
  - Task 4 nutzt einen eigenen `std::net`-HTTP-Server, nicht `tiny_http`: `tiny_http` ließ
    abgebrochene Verbindungen offen, und „Aus ist aus“ ließ sich so nicht halten.
  - `share-serve` (Bin in `core-share`, für Selbsttests am iPhone) hat die Optionen `--data`,
    `--port`, `--seed-region`, `--seed-route` und `--throttle`. Er gibt
    `serving http://127.0.0.1:<port>/` aus.
  - `ShareServer::start(addr, content, ShareOptions)` hat einen dritten Parameter
    `ShareOptions` (Fristen, Drosselung für Tests).
  - Der Schalter der Freigabe am Desktop hat den Selbsttest `OUTDOOR_SELFTEST_SHARE=1`.
  - Fehler beim Holen von `routes/<id>.json` behalten ihren eigenen Code (zum Beispiel
    `download`). Nur ungültiger Inhalt ergibt `route_import`.
- **Entscheidungen in Task 6:**
  - `RegionSummary.bytes` ist der belegte Platz auf dem Gerät, `RegionEntry.size` bei
    installierten Regionen ebenso, sonst die Downloadgröße.
  - `regions()` nimmt `not_built` heraus. Ist der Katalog nicht erreichbar, bleibt
    `catalogError` gesetzt und die installierten Regionen stehen trotzdem in der Liste.
  - Eine ungültige Routen-`id` in `importRoute` ergibt den Code `route_id`.
  - Die Tests von `core-ffi` bedienen ein zweites `App` über `core-share` auf `127.0.0.1:0`
    (Dev-Abhängigkeit).
  - Die Bindings lassen sich auf Linux nur aus der `.a` erzeugen
    (`target/release/liboutdoor_core.a`), die `.so` baut `crate-type` nicht.

## Global Constraints

- **Kein Schritt der Planung braucht ein Netz.** Der App-Start und die Karte lesen den
  Katalog **nie**. Nur der Kartenbildschirm liest ihn, wenn er geöffnet oder aktualisiert
  wird.
- **Keine Logik in Swift.** Katalogabgleich, Zustände der Regionen, Merge-Regel der Routen und
  die Normalisierung der Adresse liegen im Kern.
- Der Server liefert **nur aus**, nie schreibend, nur `GET`:
  - `GET /catalog.json`: Format `core-maps` `Catalog`, `format: 1`, je installierte und
    **vollständige** Region ein Eintrag mit `path` = Region-`id`.
  - `GET /<region>/manifest.json` und `GET /<region>/<datei>`: nur Dateien aus dem Manifest.
  - `GET /routes.json`: Liste aus `id`, `name`, `profile`, `distance_m`, `ascent_m`,
    `updated_at_ms` und `rev`, ohne gelöschte Routen.
  - `GET /routes/<id>.json`: dieselben Felder und dazu `plan`, den gespeicherten
    `plan_json` als JSON-Wert.
  - Alles andere ergibt `404`, eine andere Methode `405`.
  - `Range: bytes=<n>-` ergibt `206` mit `Content-Range: bytes <n>-<len-1>/<len>`. Bei
    `n >= len` kommt `416`. Andere Range-Formen werden ignoriert und mit `200` beantwortet.
  - Lauscht auf `0.0.0.0:<port>`, Standardport **8765**, in der Desktop-App änderbar
    (Einstellung `share.port`).
- Auf der Adresse stehen nur private IPv4-Adressen (10/8, 172.16/12, 192.168/16). Die
  Schnittstellen `wg*`, `docker*`, `br-*`, `veth*`, `virbr*`, `tun*` und `lo` werden
  übergangen. Die Adresse der Standardroute steht vorne.
- Der Firewall-Hinweis lautet genau `sudo ufw allow from <netz>/<präfix> to any port <port> proto tcp`.
  Das Netz wird aus Adresse und Netzmaske berechnet.
- Aus ist aus: Schalter aus oder App zu beendet den Server, und laufende Downloads brechen ab.
  Das iPhone setzt später per `Range` fort.
- Merge-Regel beim Übernehmen, genau so:
  - Gibt es die Route auf dem iPhone nicht: Sie wird mit **derselben `id`** angelegt.
  - Ist sie dort gelöscht: Sie kommt mit der Desktop-Fassung zurück.
  - Gilt `rev == synced_rev`, ist sie also auf dem iPhone seit der Übernahme unverändert:
    Die Desktop-Fassung ersetzt sie.
  - Sonst kommt eine **neue** Route mit dem Namen `<name> (Desktop)` dazu.
  - Nach dem Ersetzen oder Zurückholen gilt `synced_rev = rev` und `source_rev` = Desktop-`rev`.
- Eine deaktivierte Region bleibt **deaktiviert, auch nach einem Update**. Eine neu
  installierte Region ist aktiv. Löschen entfernt sie auch aus der Liste der deaktivierten.
- iOS-Rechte in `project.yml`:
  - `NSLocalNetworkUsageDescription`: „Lädt Karten und Routen vom Desktop im Heimnetz.“
  - `NSCameraUsageDescription`: „Liest den QR-Code der Freigabe am Desktop.“
  - `NSAppTransportSecurity` mit `NSAllowsLocalNetworking: true`
- Texte auf Deutsch. Die neuen Fehlercodes werden auf iOS übersetzt: `download`,
  `checksum`, `busy`, `no_catalog`, `route_import`.
- **Neue Crates** kommen in PLAN.md 3.1 mit Version, Datum und Lizenz. Gemeint sind die beim
  Bauen aktuellen stabilen Versionen von `tiny_http`, `qrcode` und `if-addrs`, geprüft mit
  `cargo search`. Alle müssen mit `rust-version = 1.90` bauen.
- Selbsttests treiben **echte Eingaben**: am Desktop Klicks auf den Schalter, auf dem iPhone
  Tippen. Den Zustand lesen sie aus `map-status` (iOS) beziehungsweise aus dem JSON-Bericht
  (Desktop).
- Push erst nach Task 6 (Übergabe) und nach dem Meilenstein, jeweils einmal fragen.

## Review Focus

1. **Start ohne Netz bei gesetzter Quelle:** Nach dem ersten Scan ist eine Katalogquelle
   gespeichert. Startet die App danach im Flugmodus oder in einem fremden WLAN, darf der Start
   nicht 20 s am Verbindungsaufbau hängen. Heute fragt `App::regions()` den Katalog ab, und
   `CoreApp::overview()` und `offline_style()` rufen es beim Start auf. → Task 2
   (`map_overview_never_reads_catalog`) und Task 6 (`overview_returns_quickly_with_dead_source`).
2. **Geladene Route wird ersetzt:** Auf dem iPhone ist die Route geladen, dann ersetzt die
   Übernahme sie durch die Desktop-Fassung. Danach darf „Speichern“ nicht die alte Planung über
   die neue schreiben. Der Kern lädt die neue Fassung als geplante Route. → Task 3
   (`import_replaces_loaded_route_in_memory`).
3. **Abbruch mitten im Download:** Der Schalter geht aus, während das iPhone ein
   Mehr-GB-Paket lädt. Das iPhone meldet einen Fehler, behält den Teil und setzt beim nächsten
   Mal fort. Der Server hört wirklich auf zu senden. → Task 4
   (`stop_ends_running_download`, `range_resumes`) und Task 7
   (`testInterruptedDownloadResumes`).
4. **Update einer Region, deren Router offen ist:** Nach einem Update oder Löschen darf
   valhalla-mobile nicht weiter die alte `routing.tar` lesen. Der Kern vergisst den Router
   (`forget_region`), und die Swift-Engine wirft ihre Instanz für diesen Pfad weg. → Task 2
   (`disable_forgets_router`) und Task 7 (`testDeleteReleasesEngine`).
5. **Feindliche Pfade:** `GET /mini/../outdoor.sqlite`, `GET /mini/%2e%2e/x` oder
   `GET /routes/../../etc.json` liefern nichts außerhalb der Manifeste und der Routen.
   → Task 4 (`only_manifest_files_are_served`).

---

### Task 1 (Linux): Regionen aktivieren und deaktivieren, Dateien für die Freigabe

**Files:**
- Modify: `crates/core-maps/src/store.rs`, `crates/core-maps/src/lib.rs`
- Test: `crates/core-maps/tests/store_enabled.rs` (neu)

**Interfaces:**
- Produces:
  - `InstalledRegion { manifest, bytes, enabled: bool }`
  - `RegionStore::set_enabled(&self, id: &str, enabled: bool) -> Result<(), MapsError>`:
    Ist `id` nicht installiert, kommt `MapsError::NotInstalled`. Gespeichert wird in
    `<dir>/.disabled.json` als sortierte JSON-Liste von Ids. `open` liest die Datei, eine
    kaputte Datei gilt als leer.
  - Diese Methoden übergehen deaktivierte Regionen: `tile`, `has_layer`, `routing_tiles`,
    `elevation` und `elevation_profile`. `installed()` liefert weiter alle Regionen, mit
    `enabled`.
  - `activate` (Update) behält den Zustand. `delete` nimmt die Id aus der Liste.
  - `RegionStore::package_file(&self, id: &str, name: &str) -> Option<(PathBuf, u64)>` liefert
    Pfad und Länge, aber nur unter drei Bedingungen: Die Region ist installiert, `name` ist
    `manifest.json` oder steht in `manifest.files`, und die Datei liegt da (bei
    Manifest-Dateien mit der Größe aus dem Manifest). Aktiv oder nicht spielt keine Rolle.
  - `RegionStore::is_complete(&self, id: &str) -> bool`: alle Manifest-Dateien da und richtig groß.
  - Neue freie Funktionen, öffentlich:
    - `pub fn catalog_location(source: &str, relative: &str) -> String`: Das ist
      `join(&catalog_base(source), relative)`.
    - `pub fn fetch_document(fetcher: &dyn Fetcher, location: &str) -> Result<String, MapsError>`:
      die vorhandene Funktion, mit der Grenze `MAX_DOCUMENT_LEN`.

- [ ] **Step 1: Tests schreiben** (Fixture `fixtures/regions/mini`, kopiert in ein Temp-Verzeichnis und mit `install_local` installiert):
  - `disabled_region_serves_no_tiles`: `tile(Basemap, …)` auf einer Kachel in `mini` ist `Some`, nach `set_enabled("mini", false)` `None`, nach `true` wieder `Some`.
  - `disabled_region_has_no_layer_and_elevation`: `has_layer(Dem)` ergibt `false`, und `elevation(47.575, 11.55)` ergibt `None`.
  - `disabled_state_survives_reopen`: deaktivieren, `RegionStore::open` neu, dann `installed()[0].enabled == false`.
  - `update_keeps_disabled`: deaktivieren, dasselbe Paket noch einmal mit `install_local` installieren, `enabled == false`.
  - `delete_clears_disabled`: deaktivieren, löschen, neu installieren, `enabled == true`.
  - `set_enabled_unknown_region_fails`: Ergebnis `MapsError::NotInstalled`.
  - `package_file_only_lists_manifest_files`: `manifest.json` und `basemap.pmtiles` liefern `Some`. `../manifest.json`, `x.pmtiles`, `.disabled.json` und `""` liefern `None`, ebenso eine nicht installierte Region.
  - `incomplete_region_is_not_complete`: Nach dem Löschen von `outdoor.pmtiles` im Store-Verzeichnis gilt `is_complete == false`, und `package_file("mini","outdoor.pmtiles")` liefert `None`.
  - `catalog_location_joins`: `("http://h:8765/", "routes.json")` ergibt `http://h:8765/routes.json`, `("http://h:8765/catalog.json", "routes/x.json")` ergibt `http://h:8765/routes/x.json`.
- [ ] **Step 2:** `cargo test -p core-maps --test store_enabled` ausführen. Erwartet: FAIL (kompiliert nicht).
- [ ] **Step 3:** Umsetzen. `Region` bekommt `enabled: bool`. Die Filter stehen dort, wo heute über `regions.values()` gelaufen wird. Die Liste wird unter der Schreibsperre geschrieben (atomar: Temp-Datei, dann umbenennen).
- [ ] **Step 4:** `cargo test -p core-maps` und `cargo clippy -p core-maps --all-targets -- -D warnings` ausführen. Erwartet: grün.
- [ ] **Step 5:** Commit `core-maps: enable and disable regions, files for sharing`.

### Task 2 (Linux): Übersicht ohne Katalog, Aktivieren im Kern, Inhalte der Freigabe

**Files:**
- Modify: `crates/core-app/src/regions.rs`, `crates/core-app/src/lib.rs`
- Create: `crates/core-app/src/share.rs`
- Test: `crates/core-app/tests/regions_offline.rs` (neu), `crates/core-app/tests/share_content.rs` (neu)

**Interfaces:**
- Consumes: Task 1 (`set_enabled`, `enabled`, `package_file`, `is_complete`).
- Produces (`regions.rs`):
  - `RegionInfo.enabled: bool`, `true` bei nicht installierten Regionen.
  - `RegionsOverview.attribution` und `installed_bounds` zählen nur **aktive** Regionen.
  - `pub struct MapOverview { pub regions: Vec<RegionInfo>, pub attribution: Vec<String>, pub installed_bounds: Option<[f64; 4]> }`
  - `App::map_overview(&self) -> MapOverview`: nur installierte Regionen, alle mit `state`
    `Installed` (beziehungsweise `Downloading`). Die Methode liest **nie** den Katalog.
  - `App::set_region_enabled(&self, id: &str, enabled: bool) -> Result<(), AppError>`: ruft
    `store.set_enabled` und danach `routing.forget_region(id)` auf.
  - `App::share_port(&self) -> Result<u16, AppError>` (Einstellung `share.port`, Standard
    8765) und `App::set_share_port(&self, port: u16) -> Result<(), AppError>`.
- Produces (`share.rs`, re-exportiert aus `lib.rs`):
  - `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] pub struct SharedRouteInfo { pub id: Uuid, pub name: String, pub profile: String, pub distance_m: f64, pub ascent_m: Option<f64>, pub updated_at_ms: i64, pub rev: i64 }`
  - `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] pub struct SharedRoute { #[serde(flatten)] pub info: SharedRouteInfo, pub plan: serde_json::Value }`
  - `App::shared_catalog(&self) -> Catalog`: installierte, vollständige Regionen mit
    `path = id`, `size = manifest.size` und `version` aus dem Manifest.
  - `App::shared_region_file(&self, id: &str, name: &str) -> Option<(PathBuf, u64)>`
  - `App::shared_routes(&self) -> Result<Vec<SharedRouteInfo>, AppError>`
  - `App::shared_route(&self, id: Uuid) -> Result<SharedRoute, AppError>`: `plan` ist der
    gespeicherte `plan_json`, als `Value` gelesen. Eine gelöschte oder unbekannte Route ergibt
    `StorageError::NotFound`.

- [ ] **Step 1: Tests schreiben.**
  - `regions_offline.rs`:
    - `map_overview_never_reads_catalog`: Katalogquelle `http://10.255.255.1:9/` (nicht erreichbar). `map_overview()` endet in unter 1 s und listet `mini`.
    - `disabled_region_leaves_bounds_and_attribution`: Mit `mini` installiert ist `installed_bounds` `Some`. Nach `set_region_enabled("mini", false)` ist es `None` und `attribution` leer. In `map_overview().regions` steht `mini` weiter, mit `enabled == false`.
    - `disable_forgets_router`: mit einer Fake-`RouterFactory`, die das Öffnen zählt. Ein Plan mit `hike` öffnet einmal. Dann deaktivieren und aktivieren, und der nächste Plan öffnet erneut: Zähler 2.
    - `share_port_defaults_to_8765`: Standard 8765, nach `set_share_port(9000)` ergibt es 9000.
  - `share_content.rs`:
    - `catalog_lists_complete_regions`: `mini` installiert, Ergebnis ein Eintrag `{id:"mini", path:"mini", size: manifest.size}`. Nach dem Löschen von `outdoor.pmtiles` im Store ist die Liste leer.
    - `catalog_lists_disabled_regions_too`.
    - `catalog_roundtrips_through_parse`: `Catalog::parse(&serde_json::to_string(&c)?, "x")` ist ok.
    - `shared_routes_skip_deleted`: zwei manuelle Routen speichern und eine löschen. Ergebnis 1 Eintrag mit `rev`.
    - `shared_route_carries_plan`: Ergebnis `plan["format"]` vorhanden, `info.id` gleich. Der Plan lässt sich als `RoutePlan` lesen.
- [ ] **Step 2:** `cargo test -p core-app --test regions_offline --test share_content` ausführen. Erwartet: FAIL.
- [ ] **Step 3:** Umsetzen. `regions()` und `map_overview()` teilen sich das Zusammenstellen der installierten Regionen (eine private Funktion). Die Fake-Fabrik im Test hängt an `App::set_router_factory`. Eine Hilfsfunktion für die Test-Fixtures gibt es in `crates/core-app/tests/` vielleicht schon, sonst eine kleine anlegen.
- [ ] **Step 4:** `cargo test --workspace` und clippy ausführen. Erwartet: grün. Auch der Desktop kompiliert mit dem neuen Feld.
- [ ] **Step 5:** Commit `core-app: map overview without the catalog, enable regions, shared content`.

### Task 3 (Linux): Routen vom Desktop übernehmen (Kern)

**Files:**
- Modify: `crates/core-storage/src/migrations.rs` (Migration 3), `crates/core-storage/src/storage.rs`, `crates/core-app/src/share.rs`, `crates/core-app/src/routes.rs` (Plan-Prüfung herausziehen), `crates/core-app/src/error.rs`
- Test: `crates/core-storage/tests/route_sync.rs` (neu), `crates/core-app/tests/route_import.rs` (neu)

**Interfaces:**
- Consumes: Task 1 (`catalog_location`, `fetch_document`), Task 2 (`SharedRoute`, `SharedRouteInfo`).
- Produces (core-storage):
  - Migration 3: `ALTER TABLE routes ADD COLUMN source_rev INTEGER;`. Die Spalte ist bei
    Routen, die auf dem Gerät entstanden sind, `NULL`.
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct RouteSync { pub rev: i64, pub synced_rev: i64, pub source_rev: Option<i64>, pub deleted: bool }`
  - `Storage::route_sync(&self, id: Uuid) -> Result<Option<RouteSync>, StorageError>`: auch
    für gelöschte Zeilen.
  - `Storage::put_synced_route(&mut self, id: Uuid, route: &SyncedRoute<'_>, now_ms: i64) -> Result<(), StorageError>`
    mit `pub struct SyncedRoute<'a> { pub name: &'a str, pub profile: &'a str, pub distance_m: f64, pub ascent_m: Option<f64>, pub plan_json: &'a str, pub source_rev: i64 }`.
    Fehlt die Zeile, entsteht sie mit `rev = 1`. Sonst wird sie ersetzt: `deleted_at = NULL`,
    `rev = rev + 1`. In beiden Fällen gilt danach `synced_rev = rev`, und `source_rev` wird
    gesetzt. `created_at` bleibt beim Ersetzen erhalten.
- Produces (core-app, `share.rs`):
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)] #[serde(rename_all = "snake_case")] pub enum RemoteRouteState { New, UpToDate, DesktopNewer, ChangedHere, DeletedHere }`
  - `#[derive(Debug, Clone, PartialEq, Serialize)] pub struct RemoteRoute { pub info: SharedRouteInfo, pub state: RemoteRouteState }`
  - `App::remote_routes(&self) -> Result<Vec<RemoteRoute>, AppError>`: liest
    `catalog_location(source, "routes.json")`. Ohne Quelle kommt `AppError::NoCatalog`.
    - Ist keine Zeile da: `New`.
    - Ist die Zeile gelöscht: `DeletedHere`.
    - Gilt `rev != synced_rev`: `ChangedHere`.
    - Gilt `source_rev == Some(info.rev)`: `UpToDate`.
    - Sonst: `DesktopNewer`.
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)] #[serde(rename_all = "snake_case")] pub enum ImportOutcome { Added, Replaced, Restored, Copied }`
  - `App::import_route(&self, id: Uuid) -> Result<(ImportOutcome, Uuid), AppError>` liefert
    das Ergebnis und die Id der Route auf diesem Gerät.
    - Liest `routes/<id>.json`.
    - Prüft, dass `info.id == id` gilt und `plan` sich als aktueller Plan (`format`) oder als
      alter Plan lesen lässt. Sonst kommt `AppError::RouteImport(String)` mit dem Code
      `route_import`.
    - Wendet die Merge-Regel aus den Global Constraints an. `Copied` speichert mit
      `save_route` unter `"<name> (Desktop)"`.
    - Ist die ersetzte oder zurückgeholte Route die geladene (`loaded_route() == Some(id)`),
      wird sie neu geladen (`saved_route(id)`), damit die geplante Route der neuen Fassung
      entspricht.

- [ ] **Step 1: Tests schreiben.**
  - `route_sync.rs`:
    - `migration_3_adds_source_rev`: Die Schema-Version ist 3, und alte Zeilen haben `source_rev = NULL`.
    - `put_synced_inserts_with_given_id`: Ergebnis `rev 1`, `synced_rev 1`, `source_rev 4`, und `list_routes` enthält die Id.
    - `put_synced_replaces_and_undeletes`: anlegen und löschen, dann `put_synced_route`. Ergebnis `deleted == false`, `rev` gleich dem alten `rev` + 1, `synced_rev == rev`, `created_at` unverändert.
    - `local_edit_breaks_sync`: Nach `put_synced_route` und `rename_route` gilt `rev == synced_rev + 1`.
  - `route_import.rs`. Der „Desktop“ ist ein zweites `App` in einem eigenen Temp-Verzeichnis. Seine Inhalte werden mit `shared_routes`/`shared_route` als Dateien `routes.json` und `routes/<id>.json` in ein Temp-Verzeichnis geschrieben, das als Katalogquelle dient. Kein HTTP nötig, `DefaultFetcher` liest Verzeichnisse.
    - `new_route_is_added_with_same_id`: `remote_routes()` liefert `[New]`. `import_route` liefert `(Added, id)`, danach steht dort `UpToDate`.
    - `desktop_change_replaces_unchanged`: am Desktop umbenennen (rev 2) und neu ausgeben. Der Zustand ist `DesktopNewer`, `import_route` liefert `Replaced`, und der Name ist der neue.
    - `iphone_change_makes_copy`: übernehmen, auf dem iPhone umbenennen. Der Zustand ist `ChangedHere`, `import_route` liefert `(Copied, other_id)` mit `other_id != id`, und der Name endet auf ` (Desktop)`. Die iPhone-Fassung ist unverändert.
    - `deleted_here_is_restored`: übernehmen, auf dem iPhone löschen. Der Zustand ist `DeletedHere`, und `import_route` liefert `Restored`.
    - `import_replaces_loaded_route_in_memory`: übernehmen und laden, am Desktop einen Punkt dazu (neu planen, `save_planned_route`), übernehmen. `planned_route().request.waypoints.len()` ist der neue Wert. Ein `save_planned_route` danach ändert die Punkte nicht.
    - `mismatched_id_is_rejected`: Die Datei `routes/<a>.json` enthält Id `b`. Ergebnis Code `route_import`, und nichts wird gespeichert.
    - `broken_plan_is_rejected`: `plan: {"x":1}` ergibt `route_import`.
    - `no_source_fails`: Ergebnis Code `no_catalog`.
- [ ] **Step 2:** `cargo test -p core-storage --test route_sync` und `cargo test -p core-app --test route_import` ausführen. Erwartet: FAIL.
- [ ] **Step 3:** Umsetzen. Das Lesen eines Plans (aktuell oder alt) zieht aus `load_plan` eine Funktion `fn check_plan(value: &serde_json::Value) -> Result<(), AppError>`, die den Import nutzt. Gespeichert wird `plan` als Text (`serde_json::to_string`), damit `float_roundtrip` die Zahlen genau erhält.
- [ ] **Step 4:** `cargo test --workspace` und clippy ausführen. Erwartet: grün.
- [ ] **Step 5:** Commit `core: take routes over from the desktop`.

### Task 4 (Linux): Crate `core-share`: Server, Adressen, QR-Code, `share-serve`

**Files:**
- Create: `crates/core-share/Cargo.toml`, `crates/core-share/src/lib.rs`, `src/server.rs`, `src/address.rs`, `src/bin/share-serve.rs`
- Modify: `Cargo.toml` (Workspace-Abhängigkeiten `core-share`, `tiny_http`, `qrcode`, `if-addrs`), `PLAN.md` 3.1 (drei Zeilen)
- Test: `crates/core-share/tests/server.rs`, `crates/core-share/tests/address.rs`

**Interfaces:**
- Consumes: Task 2 (`shared_catalog`, `shared_region_file`, `shared_routes`, `shared_route`).
- Produces:
  - `pub trait ShareContent: Send + Sync { fn catalog(&self) -> Catalog; fn region_file(&self, id: &str, name: &str) -> Option<(PathBuf, u64)>; fn routes(&self) -> Result<Vec<SharedRouteInfo>, String>; fn route(&self, id: Uuid) -> Option<SharedRoute>; }`
  - `impl ShareContent for core_app::App`
  - `pub struct ShareServer`:
    - `ShareServer::start(addr: SocketAddr, content: Arc<dyn ShareContent>) -> std::io::Result<ShareServer>`
    - `fn local_addr(&self) -> SocketAddr`
    - `fn stop(self)`: Danach nimmt der Server nichts mehr an, laufende Antworten brechen ab
      (ein `Read`-Wrapper prüft ein gemeinsames Stop-Flag je 64 KiB), und die Threads sind
      beendet.
    - `Drop` ruft `stop` auf.
    - Vier Arbeits-Threads.
  - `#[derive(Debug, Clone, PartialEq, Eq, Serialize)] pub struct LocalAddress { pub ip: Ipv4Addr, pub interface: String, pub prefix: u8, pub default_route: bool }`
  - `pub fn pick_addresses(all: &[(String, Ipv4Addr, Ipv4Addr)], default_ip: Option<Ipv4Addr>) -> Vec<LocalAddress>`:
    reine Funktion, Filter laut Global Constraints. Die Standardroute steht vorne, danach nach
    Name.
  - `pub fn local_addresses() -> Vec<LocalAddress>`: `if-addrs` plus Standardroute.
    Diese ermittelt ein UDP-Socket, der mit `connect` zu `192.0.2.1:9` verbunden wird und
    nichts sendet. Gelesen wird dann `local_addr`.
  - `pub fn share_url(ip: Ipv4Addr, port: u16) -> String` liefert `http://<ip>:<port>/`.
  - `pub fn qr_svg(text: &str) -> String`: Das SVG hat schwarze Module auf weißem Grund.
  - `pub fn firewall_hint(address: &LocalAddress, port: u16) -> String`
  - Programm `share-serve --data <dir> [--port 8765] [--seed-region <paket>] [--seed-route <name>=<lat>,<lon>;<lat>,<lon>[;…]] [--throttle <KiB/s>]`:
    - Öffnet `App` auf `<dir>/outdoor.sqlite`.
    - `--seed-region` kopiert das Paket und installiert es mit `install_local`.
    - `--seed-route` plant mit `manual` und speichert, ist mehrfach erlaubt.
    - `--throttle` bremst das Senden der Paketdateien (nur für Tests: damit lässt sich ein Download zuverlässig mitten drin abbrechen). Dafür nimmt `ShareServer::start` ein `ShareOptions { throttle_bytes_per_s: Option<u64> }` als dritten Parameter.
    - Danach druckt es `serving <url>` und läuft bis Strg-C.
    - Argumente ohne `clap` (nur zwei Optionen), oder mit `clap` aus dem Workspace. Das ist die
      Wahl des Bauenden.

- [ ] **Step 1: Tests schreiben.**
  - `server.rs`: Fake-`ShareContent` über ein Temp-Verzeichnis mit einer 300-KiB-Datei `big.pmtiles`. Server auf `127.0.0.1:0`, Anfragen mit `ureq`.
    - `serves_catalog_and_routes`: `/catalog.json` ergibt 200 mit `application/json`, und `Catalog::parse` ist ok. `/routes.json` ergibt die Liste. `/routes/<id>.json` ergibt `SharedRoute`, eine unbekannte Id 404.
    - `only_manifest_files_are_served`: `/r/big.pmtiles` ergibt 200 mit genau den Bytes. 404 kommt für `/r/other`, `/r/../x`, `/r/%2e%2e/x`, `/x/big.pmtiles`, `/routes/nope.json`, `/routes/../catalog.json` und `/`.
    - `post_is_rejected`: Ergebnis 405.
    - `range_resumes`: `Range: bytes=1000-` ergibt 206 mit `Content-Range: bytes 1000-307199/307200`, und der Body ist das Ende der Datei. `bytes=307200-` ergibt 416, `bytes=0-99` ergibt 200 mit voller Länge.
    - `resume_through_region_store`: Ein echter `RegionStore::install` von `http://127.0.0.1:<port>/` mit `mini` als Inhalt (Fake gibt die Dateien von `fixtures/regions/mini` heraus). Abbruch über `cancel` nach dem ersten Fortschritt, dann zweiter Aufruf. Ergebnis installiert, Prüfsummen ok.
    - `stop_ends_running_download`: 300-KiB-Antwort über einen Reader lesen, der nach 64 KiB anhält. Dann `stop()`, und das weitere Lesen endet mit Fehler oder kurz vor 300 KiB. Eine neue Verbindung wird abgewiesen.
    - `serves_app_content`: ein echtes `App` mit `mini` und einer Route. `/catalog.json` listet `mini`, `/mini/manifest.json` ergibt 200.
  - `address.rs`:
    - `picks_private_ipv4_only`: Eingabe `eth0 192.168.178.20/24`, `wg0 10.8.0.2/24`, `docker0 172.17.0.1/16`, `lo 127.0.0.1/8`, `wlan0 10.0.0.5/8` und `eth1 8.8.8.8/24`. Ergebnis `[eth0, wlan0]`.
    - `default_route_first`: Mit `default_ip = 10.0.0.5` steht `wlan0` vorne, mit `default_route == true`.
    - `firewall_hint_uses_network`: `192.168.178.20/24` mit Port 8765 ergibt `sudo ufw allow from 192.168.178.0/24 to any port 8765 proto tcp`.
    - `qr_svg_is_svg`: Ergebnis beginnt mit `<?xml` oder `<svg` und enthält `</svg>`.
- [ ] **Step 2:** `cargo test -p core-share` ausführen. Erwartet: FAIL.
- [ ] **Step 3:** Umsetzen.
  - Pfade prüfen: Der rohe Pfad wird **vor** jeder Dekodierung an `/` geteilt. Erlaubt sind genau zwei Segmente aus Kleinbuchstaben, Ziffern, `-`, `_` und `.`, ohne `..`. Die Datei selbst kommt nur aus `region_file`.
  - `Content-Type` ist `application/json` für `.json`, `application/octet-stream` sonst.
  - `Content-Length` immer, auch bei 206.
- [ ] **Step 4:**
  - `cargo test --workspace` und clippy ausführen. Erwartet: grün.
  - `cargo run -p core-share --bin share-serve -- --data <tmp> --seed-region fixtures/regions/mini` starten und `curl -s localhost:8765/catalog.json` abfragen. Erwartet: `mini`.
- [ ] **Step 5:** PLAN.md 3.1 um die drei Crates ergänzen. Commit `core-share: read-only server for the home network`.

### Task 5 (Linux): Desktop: Schalter, Adresse, QR-Code, Selbsttest

**Files:**
- Create: `apps/desktop/src-tauri/src/share.rs`
- Modify: `apps/desktop/src-tauri/src/main.rs` (Commands, Stopp beim Beenden), `apps/desktop/src-tauri/Cargo.toml`, `apps/desktop/ui/index.html` (Block im Bereich „Karten“), `apps/desktop/ui/src/regions.ts` oder neu `apps/desktop/ui/src/share.ts`, `apps/desktop/ui/src/api.ts`, `apps/desktop/ui/src/i18n/de.ts`, `apps/desktop/ui/src/diagnostics.ts` (Selbsttest), `README.md` (Bedienung, Selbsttest)
- Test: Selbsttest `OUTDOOR_SELFTEST_SHARE=1`

**Interfaces:**
- Consumes: Task 4 (`ShareServer`, `local_addresses`, `share_url`, `qr_svg`, `firewall_hint`), Task 2 (`share_port`).
- Produces (Tauri-Commands):
  - `share_status() -> ShareStatus`
  - `share_start(port: u16, ip: Option<String>) -> ShareStatus`: Ein geänderter Port wird
    gespeichert.
  - `share_stop()`
  - `ShareStatus { running: bool, port: u16, addresses: Vec<LocalAddress>, selected: Option<String>, url: Option<String>, qr_svg: Option<String>, firewall_hint: Option<String> }`
  - Der Desktop-Adapter `struct Content(AppHandle)` implementiert `ShareContent`, indem er an
    `handle.state::<App>()` weiterreicht.
  - Der Server liegt als `Mutex<Option<ShareServer>>` im Tauri-State. Beim
    `RunEvent::ExitRequested` wird er gestoppt.

Oberfläche im Bereich „Karten“ unter der Regionsliste:
- Ein Schalter „Im Heimnetz freigeben“ und ein Feld „Port“, das nur bei ausgeschaltetem
  Schalter änderbar ist.
- Ist der Schalter an, folgen:
  - eine Auswahl der Adresse, nur wenn es mehr als eine gibt
  - die URL als Text mit dem Knopf „Kopieren“
  - der QR-Code, 200 px groß
  - der Hinweis „Erreicht das iPhone die Adresse nicht, lässt die Firewall den Port vielleicht nicht durch:“ mit dem Befehl und dem Knopf „Kopieren“
- Fehlt eine Adresse, steht dort „Keine Heimnetz-Adresse gefunden. Ist der Rechner im WLAN
  oder per Kabel verbunden?“.
- Ist der Port belegt, erscheint die Fehlermeldung des Kerns, und der Schalter bleibt aus.

- [ ] **Step 1: Selbsttest schreiben** (`shareSelfTest` in `diagnostics.ts`, angestoßen über den Command `selftest_share()`, wie `selftest_regions`):
  - Den Schalter mit einem echten Klick auf das Element einschalten.
  - Warten, bis die URL sichtbar ist.
  - Über einen Command `selftest_fetch(url) -> String` (Rust, `ureq`, nur mit gesetztem Selbsttest-Env) `<url>catalog.json` und `<url>routes.json` lesen.
  - Den Schalter mit einem Klick ausschalten, dann `selftest_fetch` erneut aufrufen. Erwartet: Fehler.
  - Der Bericht als JSON: `{running_after_on, url, catalog_regions, routes, qr_present, fetch_after_off_failed}`.
- [ ] **Step 2:** `OUTDOOR_SELFTEST_SHARE=1` mit einem Datenverzeichnis ausführen, in dem `mini` installiert ist (`OUTDOOR_DATA_DIR`). Erwartet: FAIL, weil es die Commands noch nicht gibt.
- [ ] **Step 3:** Umsetzen.
- [ ] **Step 4:**
  - Den Selbsttest ausführen. Erwartet: `running_after_on: true`, `catalog_regions >= 1`, `qr_present: true` und `fetch_after_off_failed: true`.
  - `pnpm -C apps/desktop/ui typecheck` und `cargo clippy --workspace --all-targets -- -D warnings` ausführen. Erwartet: grün.
  - Einen Schnappschuss des Bereichs ansehen (`OUTDOOR_SNAPSHOT`). QR-Code und Hinweis sind lesbar.
- [ ] **Step 5:** Commit `desktop: share regions and routes in the home network`.

### Task 6 (Linux): `core-ffi`: Kartenverwaltung und Übernahme für Swift

**Files:**
- Modify: `crates/core-ffi/src/lib.rs`
- Create: `crates/core-ffi/src/share.rs`
- Test: `crates/core-ffi/tests/share.rs` (neu)

**Interfaces:**
- Consumes: Task 1–4.
- Produces (UniFFI):
  - `CoreListener` bekommt die Methode
    `fn on_region_progress(&self, region: String, done: u64, total: u64)`. Der Listener-Adapter
    reicht `AppListener::on_region_progress` weiter.
  - `RegionSummary { id, name, version, enabled: bool, bytes: u64 }`
  - `CoreApp::overview()` und `offline_style()` nutzen `map_overview()` und lesen **nie** den
    Katalog.
  - `RegionEntry { id, name, size: u64, version, installed_version: Option<String>, state: String, enabled: bool }`
    mit `state` aus `available`, `installed`, `update_available` und `downloading`.
    `not_built` wird auf dem iPhone weggelassen.
  - `RegionList { source: String, catalog_error: Option<String>, regions: Vec<RegionEntry> }`
  - `CoreApp`-Methoden:
    - `catalog_source() -> Result<String, CoreError>`
    - `set_catalog_source(source: String) -> Result<String, CoreError>`: Normalisiert und
      gibt das Gespeicherte zurück.
      - Ohne `://` wird `http://` vorangestellt.
      - Ohne `/` am Ende wird `/` angehängt, es sei denn, der Pfad endet auf `.json`.
      - Leer heißt: keine Quelle.
    - `regions() -> Result<RegionList, CoreError>`: blockiert und liest den Katalog.
    - `install_region(id) -> Result<RegionSummary, CoreError>`: blockiert, Fortschritt über
      den Listener.
    - `cancel_region_download(id)`
    - `delete_region(id) -> Result<(), CoreError>`
    - `set_region_enabled(id, enabled: bool) -> Result<(), CoreError>`
  - `RemoteRouteEntry { id: String, name, profile, distance_m: f64, ascent_m: Option<f64>, rev: i64, state: String }`
    mit `state` aus `new`, `up_to_date`, `desktop_newer`, `changed_here` und `deleted_here`.
  - Weitere `CoreApp`-Methoden:
    - `remote_routes() -> Result<Vec<RemoteRouteEntry>, CoreError>`
    - `import_route(id: String) -> Result<ImportedRoute, CoreError>` mit
      `ImportedRoute { outcome: String, id: String }`. `outcome` ist einer von `added`,
      `replaced`, `restored` und `copied`.

- [ ] **Step 1: Tests schreiben** (`tests/share.rs`, mit einem `ShareServer` aus `core-share` als Dev-Abhängigkeit vor einem zweiten `App`):
  - `overview_returns_quickly_with_dead_source`: Quelle `10.255.255.1:9`. `overview()` und `offline_style()` enden in unter 1 s.
  - `source_is_normalised`: `"192.168.1.5:8765"` ergibt `http://192.168.1.5:8765/`, `"http://h/catalog.json"` bleibt, `" "` ergibt `""`.
  - `install_over_http_reports_progress`: Ein Listener sammelt Fortschritte. Nach `install_region("mini")` gibt es ≥ 1 Fortschritt mit `done == total`, und `overview().regions` enthält `mini`.
  - `regions_shows_update_and_states`: vor der Installation `available`, danach `installed`.
  - `enable_toggle_roundtrip`: Nach `set_region_enabled("mini", false)` gilt `overview().regions[0].enabled == false`, und `overview().bounds` ist `None`.
  - `import_route_over_http`: Eine Route am zweiten App ergibt `remote_routes()` `[new]`, `import_route` liefert `added` mit derselben Id, danach `up_to_date`, und `saved_routes()` enthält sie.
- [ ] **Step 2:** `cargo test -p core-ffi --test share` ausführen. Erwartet: FAIL.
- [ ] **Step 3:** Umsetzen. Nur Typumwandlung, dazu die Normalisierung der Adresse.
- [ ] **Step 4:**
  - `cargo test --workspace` und clippy ausführen. Erwartet: grün.
  - Die Swift-Bindings zur Probe erzeugen wie in I3 Task 4. Erwartet: `onRegionProgress` steht im Protokoll `CoreListener`.
- [ ] **Step 5:**
  - Commit `core-ffi: region manager and route import for the apps`.
  - Den Abschnitt „Stand nach Task 1–6“ in diesen Plan schreiben, mit Rulings und der Swift-API für die Mac-Sitzung, und committen.
  - **Den Nutzer fragen, ob `feature/ios-i4` gepusht werden darf.**

### Task 7 (Mac): Kartenbildschirm: Regionen laden, aktualisieren, aktivieren, löschen

**Files:**
- Create: `apps/ios/Outdoor/Regions/RegionsScreen.swift`, `apps/ios/Outdoor/Regions/RegionsController.swift` (ObservableObject: `list`, `installed`, `progress[id]`, `error`)
- Modify: `apps/ios/project.yml` (Info-Schlüssel), `apps/ios/Outdoor/Core/CoreHost.swift` (Listener mit Fortschritt), `apps/ios/Outdoor/Core/ValhallaEngine.swift` (`forget(tilesPath:)`), `apps/ios/Outdoor/Map/MapView.swift` (`reloadRegions()`: Stil neu, Grenzen neu), `apps/ios/Outdoor/Plan/PlanSheet.swift` (Eintrag „Regionen“), `apps/ios/Outdoor/Map/MapScreen.swift` (Status, Hinweis ohne Region), `apps/ios/Outdoor/AppStart.swift` (`-catalogSource <url>`), `apps/ios/scripts/test.sh` (`share-serve` starten)
- Test: `apps/ios/OutdoorUITests/RegionsUITests.swift`, `apps/ios/OutdoorTests/ValhallaEngineTests.swift`

Bedienung:
- Der Bildschirm „Regionen“ öffnet sich aus dem ganz ausgezogenen Sheet, neben „Routen“.
- Ohne installierte Region zeigt der Hinweis auf der Karte einen Knopf „Regionen laden“.
- Abschnitt **„Auf dem iPhone“**: jede installierte Region mit Name, Größe und Stand.
  - Ein Schalter „Aktiv“.
  - Wischen bietet „Löschen“, mit Rückfrage.
  - Bei `update_available` gibt es den Knopf „Aktualisieren“.
- Abschnitt **„Vom Desktop“**:
  - die Adresse der Quelle mit „QR-Code scannen“ (Task 9) und „Adresse eingeben“
  - die nicht installierten Regionen mit Größe und „Laden“
  - während des Downloads Fortschrittsbalken, Bytes und „Abbrechen“
  - Ist der Katalog nicht lesbar: „Desktop nicht erreichbar: <Meldung>“ und „Erneut versuchen“
- Der Katalog wird nur beim Öffnen und beim Herunterziehen gelesen, abseits des Main-Threads.
- Nach Installieren, Aktualisieren, Löschen oder Umschalten passiert dreierlei:
  - `MapController.reloadRegions()` lädt Stil und Grenzen neu.
  - Bei Update und Löschen verwirft die Engine ihre Instanz für die `routing.tar` der Region.
  - Eine geplante Route wird neu berechnet.

Status `map-status`, neue Felder:
- `regions=<id>:<0|1>[,…]`: installiert, mit Aktiv-Zustand
- `catalog=<n>` und `catalog_error=<0|1>`
- `downloading=<id>|-` und `download_pct=<0–100>`

- [ ] **Step 1: Tests schreiben.**
  - `test.sh` startet vor den UI-Tests `share-serve --data <tmp> --port 8766 --seed-region fixtures/regions/mini --seed-route "Sylvenstein Test=47.572,11.545;47.578,11.555"` und beendet es danach. Die UI-Tests starten mit `-resetData -catalogSource http://127.0.0.1:8766/`.
  - `testInstallFromDesktop`:
    - Zu Beginn gilt `regions=` (leer).
    - „Regionen“ öffnen: `catalog=1`.
    - „Laden“ bei „Sylvenstein (Test)“ tippen und warten, bis `regions=mini:1` gilt. Die Karte zeigt Kacheln (Kachelzähler aus I1, `TileURLProtocol.stats.served > 0`).
  - `testDisableHidesRegion`:
    - installieren, dann den Schalter „Aktiv“ tippen
    - Ergebnis `regions=mini:0`, Grenzen weg, und eine Planung mit `manual` geht weiter (manuell braucht keine Region)
    - Nach erneutem Tippen gilt `mini:1`.
  - `testDeleteRegion`: Wischen, „Löschen“ und Rückfrage bestätigen. Ergebnis `regions=`, und der Hinweis „Regionen laden“ ist sichtbar.
  - `testDesktopUnreachable`: Start mit `-catalogSource http://127.0.0.1:9/`. Ergebnis `catalog_error=1` mit sichtbarer Meldung. Die Karte ist trotzdem in unter 5 s da. Gemessen wird mit dem Startzeit-Feld aus I1 oder, falls es keines gibt, mit einem neuen Feld `start_ms`.
  - `testInterruptedDownloadResumes`: ein zweites `share-serve` mit `--throttle 200` auf Port 8767, sodass `mini` rund 4 s braucht. „Laden“ tippen, bei `download_pct >= 20` „Abbrechen“ tippen. Erneut „Laden“ beginnt mit `download_pct > 0` (fortgesetzt, nicht von vorn) und endet mit `mini:1`.
  - `ValhallaEngineTests.testDeleteReleasesEngine`: Nach `forget(tilesPath:)` legt der nächste `route` eine neue Instanz an (Zähler im Test).
- [ ] **Step 2:** `apps/ios/scripts/test.sh` ausführen. Erwartet: FAIL.
- [ ] **Step 3:** Umsetzen. Fehlercodes übersetzen (Global Constraints).
- [ ] **Step 4:** Ausführen. Erwartet: grün, auch die UI-Tests aus I1–I3.
- [ ] **Step 5:** Commit `ios: region manager with download from the desktop`.

### Task 8 (Mac): Routen vom Desktop übernehmen

**Files:**
- Create: `apps/ios/Outdoor/Plan/RemoteRoutesList.swift`
- Modify: `apps/ios/Outdoor/Plan/RoutesList.swift` (Knopf „Vom Desktop übernehmen“), `PlanController.swift` (nach dem Übernehmen neu laden, wenn es die geladene Route war), `MapScreen.swift` (Status)
- Test: `apps/ios/OutdoorUITests/RemoteRoutesUITests.swift`

Bedienung:
- „Vom Desktop übernehmen“ öffnet die Liste aus `remote_routes()`. Je Route stehen dort Name,
  Distanz und Aufstieg und der Zustand als Text:
  - „Neu“
  - „Aktuell“
  - „Am Desktop geändert“
  - „Auf dem iPhone geändert – wird als Kopie übernommen“
  - „Auf dem iPhone gelöscht“
- Der Knopf „Übernehmen“ fehlt bei „Aktuell“. Danach gibt es eine kurze Meldung je Ergebnis:
  - „Übernommen“
  - „Ersetzt“
  - „Wiederhergestellt“
  - „Als Kopie „… (Desktop)“ übernommen“
- Ist keine Quelle gesetzt, verweist die Liste auf „Regionen → QR-Code scannen“.

Status `map-status`, neue Felder: `remote_routes=<n>`, `remote_states=<state,…>` und `import_outcome=<…>`.

- [ ] **Step 1: UI-Tests schreiben** (mit `share-serve` aus Task 7, Region `mini` vorher installiert):
  - `testImportNewRoute`: Ergebnis `remote_states=new`, nach „Übernehmen“ `import_outcome=added`, `routes=1` und `remote_states=up_to_date`. Laden ergibt `waypoints=2`.
  - `testChangedHereMakesCopy`: übernehmen, laden, einen Punkt dazu, speichern. Ergebnis `remote_states=changed_here`, nach „Übernehmen“ `import_outcome=copied` und `routes=2`, und in der Liste steht „Sylvenstein Test (Desktop)“.
  - `testDeletedHereIsRestored`: übernehmen und löschen. Ergebnis `remote_states=deleted_here`, nach „Übernehmen“ `restored` und `routes=1`.
- [ ] **Step 2:** Ausführen. Erwartet: FAIL.
- [ ] **Step 3:** Umsetzen.
- [ ] **Step 4:** Ausführen. Erwartet: grün.
- [ ] **Step 5:** Commit `ios: take routes over from the desktop`.

### Task 9 (Mac): QR-Code scannen und Adresse eingeben

**Files:**
- Create: `apps/ios/Outdoor/Regions/ScanSheet.swift` (VisionKit `DataScannerViewController` in `UIViewControllerRepresentable`, nur `.barcode(symbologies: [.qr])`), `apps/ios/Outdoor/Regions/ShareAddress.swift`
- Modify: `RegionsScreen.swift`
- Test: `apps/ios/OutdoorTests/ShareAddressTests.swift`, `apps/ios/OutdoorUITests/RegionsUITests.swift`

`ShareAddress.accept(_ scanned: String) -> String?` nimmt nur `http://` und `https://` an,
mit Host, ohne Leerzeichen. Normalisiert wird im Kern (`set_catalog_source`), Swift filtert nur
fremde QR-Codes aus. Nach einem gültigen Scan schließt sich der Scanner, die Quelle wird
gesetzt und der Katalog gelesen. Ist `DataScannerViewController.isSupported` oder
`isAvailable` falsch, oder fehlt die Kamera-Erlaubnis, erscheint ein Hinweis, und „Adresse
eingeben“ bleibt.

- [ ] **Step 1: Tests schreiben.**
  - `ShareAddressTests`: `http://192.168.178.20:8765/` wird angenommen. `WIFI:S:x;;`, `hello` und `ftp://h/` werden abgelehnt, `""` ebenso.
  - `testTypedAddress`: „Adresse eingeben“, `127.0.0.1:8766` tippen, bestätigen. Ergebnis `catalog=1`, und die angezeigte Quelle ist `http://127.0.0.1:8766/`.
- [ ] **Step 2:** Ausführen. Erwartet: FAIL.
- [ ] **Step 3:** Umsetzen. Den Scanner selbst kann der Simulator nicht prüfen, das prüft der Nutzer in Task 10 am iPhone.
- [ ] **Step 4:** Ausführen. Erwartet: grün.
- [ ] **Step 5:** Commit `ios: scan the desktop's QR code or type its address`.

### Task 10 (Mac + Linux): Abnahme, Messwerte, Doku

**Files:**
- Modify: `docs/MESSPROTOKOLL.md` (Abschnitt „iOS I4: Freigabe“), `PLAN.md` (8.1, Stand I4), `README.md` (Freigabe am Desktop, Kartenbildschirm auf dem iPhone)

- [ ] **Step 1:** `apps/ios/scripts/test.sh` komplett ausführen, im Simulator und auf dem iPhone (`DEVICE=<udid>`). Erwartet: grün.
- [ ] **Step 2: Abnahme durch den Nutzer** (Fertig-Kriterium aus dem Entwurf), Ablauf aufschreiben:
  - Die App auf dem iPhone löschen und neu installieren.
  - Am Desktop „Im Heimnetz freigeben“ einschalten.
  - Auf dem iPhone „Regionen → QR-Code scannen“, die Erlaubnis für das lokale Netzwerk erteilen und eine Region laden.
  - Eine Desktop-Route übernehmen.
  - Flugmodus an, die Route laden, umplanen, speichern.
  - Eine Region deaktivieren: Die Karte dort ist leer. Wieder aktivieren, dann löschen.
- [ ] **Step 3:** Messwerte in `docs/MESSPROTOKOLL.md`:
  - die Download-Geschwindigkeit einer großen Region im WLAN (MB/s, aus `download_pct` und Zeit, oder vom Desktop-Log)
  - die App-Größe
  - PLAN.md 8.1 und README nachziehen
- [ ] **Step 4:** Commit `ios: I4 done – sharing in the home network`. **Anhalten**, dem Nutzer zeigen und den Push erfragen.

## Danach

- Gesamtprüfung durch einen frischen Gutachter über **I3 und I4** (`fc5f979..HEAD`). Der
  geplante Review von I3 vor dem Merge entfiel, weil I3 nach der Abnahme durch den Nutzer
  direkt nach `main` ging. Danach Merge nach `main`, nach Rückfrage.
- Dann das Brainstorming für **Aufzeichnen + Tourenbuch** (Wunsch des Nutzers vom 2026-11-16).
