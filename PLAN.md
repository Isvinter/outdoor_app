# PLAN – Offline-Outdoor-App (Rad, MTB, Wandern, Skitour)

Stand: 2026-11-13 · Status: **freigegeben am 2026-11-13**, Rückfragen beantwortet (siehe Abschnitt 12).

Alle Versionen, Lizenzen und Wartungsstände in diesem Dokument wurden am 2026-11-13 gegen
crates.io, npm, GitHub und die jeweiligen Projektseiten geprüft. Wo etwas *nicht* verifiziert
werden konnte, steht das ausdrücklich dabei.

---

## 1. Ziel und Abgrenzung

Eine Outdoor-App im Stil von Komoot, die nach dem Laden der Regionsdaten vollständig offline
arbeitet: Tracks aufzeichnen, Topo-Karte (Sommer/Winter mit Hangneigung), lokales Routing.

- **Phase 1 (jetzt):** Desktop-App für Ubuntu LTS (Entwicklungsrechner: Ubuntu 26.04, x86_64).
- **Phase 2 (später):** iOS- und Android-App auf demselben Rust-Kern.
- **Phase 3 (später):** Accounts und Track-Sync mit privatem Server auf dem VPS.

Nicht-Ziele Phase 1: Sync, Accounts, mobile Builds, Turn-by-turn-Navigation mit Sprachansagen,
Skitouren-Routing auf dem Wegegraphen, soziale Funktionen.

Was das Projekt scheitern ließe, selbst wenn es „funktioniert“: Fachlogik im Frontend,
Netzabhängigkeit zur Laufzeit, verlorene Aufzeichnungen nach Absturz, Architektur, die für
Mobile umgebaut werden muss.

---

## 2. Architektur

### 2.1 Workspace-Layout

```
outdoor_app/
├─ Cargo.toml                 # Workspace, gemeinsame Versionen, Lints, Release-Profil (LTO)
├─ crates/
│  ├─ core-track/             # Track-Modell, Recorder-Zustandsautomat, Statistik, Filter
│  ├─ core-gpx/               # GPX 1.1 lesen/schreiben
│  ├─ core-storage/           # SQLite, Migrationen, Repositories
│  ├─ core-routing/           # Trait Router, Profile, Zeitschätzung; Feature "valhalla"
│  ├─ core-maps/              # Regionen (Katalog, Download, Löschen), PMTiles-Lesen, DEM-Abfrage
│  ├─ core-location/          # Trait LocationSource, GPX-Replay, gpsd-Client
│  └─ core-app/               # Anwendungsdienste: verbindet Ortung, Aufzeichnung, Speicher, GPX
├─ tools/
│  ├─ slope-tiles/            # CLI: DEM-PMTiles → Hangneigungs-PMTiles
│  └─ track-cli/              # CLI M1: GPX rein → Statistik raus
├─ apps/
│  └─ desktop/
│     ├─ src-tauri/           # Tauri-2-Hülle: Commands, Events, Kachel-Protokoll
│     └─ ui/                  # TypeScript + Vite + MapLibre GL JS, kein Framework
├─ styles/                    # MapLibre-Style-JSONs (Sommer, Winter) – plattformübergreifend
├─ scripts/                   # build-valhalla-tiles.sh, build-region.sh
├─ fixtures/                  # echte GPX-Beispieltracks, kleine DEM-/Kachel-Testdaten
├─ PLAN.md
└─ README.md
```

Zwei Crates kommen gegenüber der Vorgabe hinzu (meine Entscheidung):

- `core-location`: Der Trait `LocationSource` gehört weder in `core-track` noch in die
  Tauri-Hülle, und die mobilen Quellen aus Phase 2 implementieren denselben Trait von außen.
- `core-app`: die eine Stelle, an der Ortung, Aufzeichnung, Speicher und GPX zusammenkommen
  (Aufzeichnung starten/pausieren/stoppen, Wiederaufnahme nach Absturz, Import/Export,
  Trackverwaltung). Die Tauri-Commands rufen nur `core-app` auf; in Phase 2 wird genau diese
  Schnittstelle per UniFFI an Swift/Kotlin gereicht. Native Ortungsdienste liefern ihre Fixes
  über `App::handle_fix`.

### 2.2 Abhängigkeitsregeln

- `core-*` kennt kein Tauri, kein GTK, kein Dateisystem-Layout des Betriebssystems. Pfade
  (Datenverzeichnis, Regionsverzeichnis) werden von der Hülle hineingereicht.
- C++-Anteile sind per Cargo-Feature abschaltbar (`core-routing/valhalla`). Der gpsd-Client
  ist reines Rust auf der Standardbibliothek (blockierendes Lesen in einem eigenen Thread)
  und braucht kein Feature. Ohne Features kompilieren alle Kern-Crates rein in Rust plus
  gebündeltes SQLite in C – das ist der Zustand, den die CI für iOS/Android per
  `cargo check` prüft (Android mit dem NDK des CI-Runners, iOS auf einem macOS-Runner, weil
  SQLite einen C-Compiler für das Zielsystem braucht).
- Die Tauri-Hülle enthält nur Verdrahtung: Command → Kern-Aufruf → serialisiertes Ergebnis.
- Das Frontend enthält keine Fachlogik. Es rechnet keine Distanzen, keine Statistik, keine
  Höhenprofile; es zeichnet, was der Kern liefert.

### 2.3 Zentrale Traits

```rust
// core-location
pub trait LocationSource: Send {
    fn start(&mut self, cfg: LocationConfig, sink: Box<dyn FixSink>) -> Result<(), LocationError>;
    fn stop(&mut self) -> Result<(), LocationError>;
}
pub trait FixSink: Send { fn on_fix(&self, fix: Fix); fn on_status(&self, s: SourceStatus); }

// core-routing
pub trait Router: Send + Sync {
    fn route(&self, req: &RouteRequest) -> Result<Route, RoutingError>;
    fn capabilities(&self) -> RouterCapabilities;   // Profile, einstellbare Optionen
}

// core-maps
pub trait ElevationProvider: Send + Sync {
    fn elevation(&self, pos: LatLon) -> Option<f32>;            // bilinear aus DEM-Kacheln
    fn profile(&self, line: &[LatLon], step_m: f32) -> Vec<ProfilePoint>;
}
pub trait TileSource: Send + Sync {
    fn tile(&self, z: u8, x: u32, y: u32) -> Result<Option<Tile>, MapsError>;
}
```

`LocationSource` ist **push-basiert** (Callback statt Abfrage). Das passt zu gpsd (Datenstrom),
zu `CLLocationManager` und zum Android-`FusedLocationProvider`, und lässt sich mit UniFFI als
Callback-Interface abbilden. Keine Polling-Schleifen.

### 2.4 Datenfluss Aufzeichnung

```
LocationSource ──Fix──► Recorder (core-track) ──► core-storage (SQLite, WAL)
                              │
                              └─ Live-Statistik ──► Tauri-Event "recording:update" ──► Karte
```

- Der Recorder ist ein Zustandsautomat: `Idle → Recording ⇄ Paused → Stopped`.
- Jeder akzeptierte Fix wird sofort in SQLite geschrieben (WAL, `synchronous=NORMAL`).
  Bei 1 Hz ist das unkritisch; gegen Stromausfall gehen schlimmstenfalls die letzten Punkte
  seit dem letzten WAL-Sync verloren, die Datenbank bleibt konsistent.
- Der Aufzeichnungszustand (`recording` / `paused`) steht in der Tabelle `tracks`. Beim Start
  findet die App einen Track im Zustand `recording`/`paused` und bietet „Fortsetzen“ an; eine
  Lücke durch den Absturz wird als neues Segment begonnen.
- Aufzeichnung und Speicherung laufen in einem eigenen Thread, nie im UI-Thread.
- Die Karte bekommt Live-Updates gebündelt (höchstens ca. 1/s), nicht pro Fix.

### 2.5 Karte und Kacheln

- MapLibre GL JS 6 im Tauri-WebView. Achtung: v6 ist ESM-only und setzt WebGL2 voraus.
  Die Worker lädt v6 als eigene ES-Module von einer URL, die es nur unter http(s) selbst
  findet; im Tauri-Fenster (eigenes URL-Schema) werden die Worker-Dateien deshalb neben das
  Bundle kopiert und per `setWorkerUrl` gesetzt (`vite.config.ts`).
- Kacheln kommen über ein eigenes Tauri-URI-Schema (`tiles://localhost/<layer>/{z}/{x}/{y}`;
  umgesetzt in M3 ohne Region in der Adresse – der Kern sucht die Kachel in allen
  installierten Regionen), das in Rust von `core-maps` aus den lokalen PMTiles-Dateien bedient wird. Das Frontend sieht
  ganz normale Kachel-URLs; Lesen und Zusammenführen mehrerer Regionen liegt im Kern.
  (Alternative wäre das `pmtiles`-JS-Protokoll mit Range-Requests auf Dateien – verworfen, weil
  dann Regionslogik im Frontend läge.)
- Style-JSONs liegen in `styles/` und folgen der MapLibre-Style-Spezifikation, damit sie in
  Phase 2 von MapLibre Native wiederverwendet werden können.
- Höhenlinien: Phase 1 clientseitig aus den DEM-Kacheln mit `maplibre-contour` (läuft im
  Web-Worker, braucht keinen zusätzlichen Speicher). MapLibre Native hat dieses Plugin nicht;
  für Phase 2 müssen Höhenlinien als Vektor-PMTiles vorberechnet werden (Erweiterung des
  Regions-Build-Skripts, kein Umbau der App).
- Schummerung: MapLibre-`hillshade`-Layer aus derselben `raster-dem`-Quelle.
- Hangneigung: vorberechnete Rasterkacheln als `raster`-Layer, Deckkraft über
  `raster-opacity` einstellbar.

### 2.6 Routing

- `core-routing` definiert `Router`, `RouteRequest` (Wegpunkte, Profil, Optionen) und `Route`
  (Geometrie, Distanz, Dauer, Abschnitte). Implementierung `ValhallaRouter` hinter dem Feature
  `valhalla`.
- Höhenprofil kommt **nicht** aus Valhalla, sondern aus `ElevationProvider` (unser DEM). Grund:
  dieselbe Höhenquelle wie für die DEM-Korrektur von Tracks, höhere Auflösung, und es
  funktioniert genauso für manuell gezeichnete Skitouren-Routen.
- Manuelle Route (Skitour): Liste von Stützpunkten, gerade Verbindungen, gleiches
  `Route`-Modell, gleiches Höhenprofil, gleicher GPX-Export.
- Zeitschätzung: Valhalla liefert Fahr-/Gehzeit nach eigenem Modell. Für Wandern und Skitour
  berechnet `core-routing` zusätzlich eine Zeit nach DIN 33466 (4 km/h horizontal, 300 Hm/h
  Aufstieg, 500 Hm/h Abstieg; größerer Wert plus die Hälfte des kleineren). Parameter
  konfigurierbar. (Meine Entscheidung; die Valhalla-Gehzeit ist im Gebirge zu optimistisch.)

---

## 3. Abhängigkeiten (geprüft 2026-11-13)

### 3.1 Rust-Crates

| Zweck | Crate | Version | Letztes Update | Lizenz | Bemerkung |
|---|---|---|---|---|---|
| Desktop-Hülle | `tauri` | 2.12.5 | 2026-11-06 | MIT/Apache-2.0 | Tauri 3 ist erst Alpha (3.0.0-alpha.6) → bewusst 2.x |
| | `tauri-build` | 2.7.4 | 2026-10-21 | MIT/Apache-2.0 | |
| | `tauri-plugin-dialog` | 2.8.4 | 2026-10-28 | MIT/Apache-2.0 | Datei öffnen/speichern |
| SQLite | `rusqlite` (bundled) | 0.42.0 | 2026-11-11 | MIT | SQLite wird mitkompiliert → identisch auf allen Zielen |
| Migrationen | eigener Runner | – | – | – | `PRAGMA user_version`, ~30 Zeilen. `rusqlite_migration` 2.7.0 verlangt noch rusqlite 0.41 |
| XML | `quick-xml` | 0.42.0 | 2026-08-22 | MIT | GPX-Parser/-Writer selbst geschrieben |
| IDs | `uuid` (v7) | 1.29.2 | 2026-11-09 | MIT/Apache-2.0 | zeitlich sortierbare UUIDs |
| Zeit | `time` | 0.3.55 | 2026-08-01 | MIT/Apache-2.0 | |
| Fehler | `thiserror` | 2.0.21 | 2026-09-23 | MIT/Apache-2.0 | eigene Error-Typen je Crate |
| Serialisierung | `serde` / `serde_json` | 1.0.229 / 1.0.154 | 2026 | MIT/Apache-2.0 | |
| Async (Hülle, Download ab M3) | `tokio` | 1.54.1 | 2026-10-21 | MIT | über Tauri; die Kern-Crates sind bisher synchron |
| HTTP-Download | `ureq` (rustls) | 3.5.2 | 2026-11-13 | MIT/Apache-2.0 | M3: statt `reqwest` gewählt – blockierend, ohne tokio, damit bleibt `core-maps` synchron und klein; Range-Requests, Fortsetzen |
| PMTiles lesen | eigener Leser in `core-maps` | – | – | – | M3: das Crate `pmtiles` 0.28.1 liest nur asynchron über tokio; der eigene synchrone Leser (v3, ~300 Zeilen, gegen go-pmtiles-Archive getestet) hält den Kern frei von einer Async-Laufzeit. Geschrieben wird mit Planetiler und go-pmtiles |
| Bilder (WebP/PNG) | `image` | 0.25.10 | 2026-03-10 | MIT/Apache-2.0 | DEM-WebP dekodieren, Kacheln kodieren; reines Rust |
| Prüfsummen | `sha2` | 0.11.1 | 2026-10-12 | MIT/Apache-2.0 | Regionspakete verifizieren |
| CLI | `clap` | 4.6.7 | 2026-09-14 | MIT/Apache-2.0 | |
| Routing | `valhalla` | 0.7.6 | 2026-11-07 | MIT | siehe 3.3 |
| Logging | `tracing` | 0.1.44 | 2025-12-18 | MIT | |
| Tests | `proptest` / `criterion` | 1.12.0 / 0.8.2 | 2026 | MIT/Apache-2.0 | Eigenschaftstests, Benchmarks (M6) |
| Phase 2 | `uniffi` | 0.32.2 | 2026-09-23 | MPL-2.0 | nur vorgemerkt |
| Phase 3 | `axum` / `sqlx` | 0.8.9 / 0.9.0 | 2026 | MIT / MIT+Apache | nur vorgemerkt |

Bewusst **nicht** verwendet:

- `gpx` 0.10.0 – letztes Release 2023-12, nur ~57k Downloads in 90 Tagen. Ein eigener
  Parser auf `quick-xml` ist klein, gibt volle Kontrolle über Extensions und Fehlermeldungen.
- `gpsd_proto` 1.0.0 – letztes Release 2023-11. Das gpsd-JSON-Protokoll ist stabil und klein
  (`TPV`, `SKY`, `WATCH`); wir parsen die wenigen Nachrichten selbst mit `serde`.
- `geo` – Haversine und Peilung sind wenige Zeilen; spart einen Abhängigkeitsbaum.
- `chrono` – `time` reicht.
- GDAL – nicht nötig, weil DEM bereits als Terrarium-Kacheln vorliegt (siehe 4).

MPL-2.0 (`uniffi`) ist dateibezogenes Copyleft und für eine private App unproblematisch.

### 3.2 Frontend (npm)

| Paket | Version | Datum | Lizenz |
|---|---|---|---|
| `maplibre-gl` | 6.18.0 | 2026-11-11 | BSD-3-Clause |
| `maplibre-contour` | 0.1.1 | 2026-09-17 | BSD-3-Clause |
| `@tauri-apps/api` / `@tauri-apps/cli` | 2.12.4 | 2026-10 | MIT/Apache-2.0 |
| `vite` | 8.3.8 | 2026-11-11 | MIT |
| `typescript` | 7.0.2 | 2026-07-08 | Apache-2.0 |

Kein UI-Framework. Das Höhenprofil wird als kleines eigenes SVG/Canvas-Element gezeichnet
(keine Chart-Bibliothek).

### 3.3 Valhalla

- Valhalla 3.9.1 (2026-10-06), MIT, sehr aktiv (letzter Push 2026-11-12).
- Die Rust-Bindings sind seit Version 0.7.5 **offizieller Teil des Valhalla-Repos**
  (`src/bindings/rust`, vorher `kinkard/valhalla-rs`, jetzt archiviert). Technik: `cxx`,
  Valhalla-Quellen im Crate enthalten, Build per CMake.
- Geboten: `Actor`-API im Prozess (Route, wie die Python-Bindings), typisierte Konfiguration
  ohne JSON/Python.
- Einschränkungen:
  - Nur `tile_extract` (eine `tiles.tar`), kein Kachelverzeichnis → passt zu „eine Datei pro
    Region“.
  - Der Crate baut **ohne** Datenwerkzeuge (`ENABLE_DATA_TOOLS=OFF`). Kacheln werden also nicht
    in der App gebaut, sondern per Skript mit dem offiziellen Docker-Image
    (`ghcr.io/valhalla/valhalla`, Erreichbarkeit geprüft).
  - Valhalla-Logging lässt sich noch nicht nach `tracing` umleiten.
  - Die docs.rs-Dokumentation von 0.7.6 baut nicht (zuletzt erfolgreich 0.6.1) – wir lesen
    Quellen und Tests.
  - Nur ~1000 Downloads in 90 Tagen: junge Bindings, wenig Verbreitung. Deshalb strikt hinter
    dem Trait `Router`; Ausweichweg wäre ein eigener dünner `cxx`-Wrapper oder Valhalla als
    lokaler Unterprozess.
- Systempakete für den Build: `clang pkg-config build-essential cmake libboost-dev
  libprotobuf-dev protobuf-compiler zlib1g-dev` (in Ubuntu 26.04 vorhanden: Boost 1.90,
  Protobuf 3.21).
- Mehrere Regionen: ein Valhalla-`tile_extract` ist genau eine Datei. Routen über
  Regionsgrenzen hinweg gehen deshalb nur, wenn beide Orte im selben Paket liegen. Phase 1:
  Routing innerhalb einer Region; die Region wird anhand der Wegpunkte gewählt.

#### Valhalla und MTB – Befund aus dem Quelltext (Stand master, 2026-11-13)

Ausgewertet werden die Tags **beim Kachelbau** (`pbfgraphparser.cc`, `graph.lua`), nicht pro
Anfrage:

| Tag | Wirkung |
|---|---|
| `mtb:scale` | setzt die Oberflächenklasse: 0 → *dirt*, 1 → *gravel*, ≥ 2 → *path*; Fahrrad-Zugang wird für Werte < 6 erlaubt |
| `mtb:scale:uphill` | setzt Oberfläche nur, wenn `mtb:scale` fehlt (< 2 → *gravel*, sonst *path*); Zugang für Werte < 5 |
| `mtb:scale:imba` | erlaubt Fahrrad-Zugang, wenn die beiden anderen fehlen |
| `mtb=yes`, `route=mtb`, `network=mtb` | markiert die Kante als Teil des MTB-Netzes |
| `surface` | wird auf 8 Oberflächenklassen abgebildet; hat Vorrang vor `tracktype` |
| `tracktype` | grade1…5 → Oberflächenklasse; hat Vorrang vor `smoothness` |
| `sac_scale` | wird gespeichert und vom Fußgänger-Profil genutzt (`max_hiking_difficulty`) |

Zur Laufzeit im `bicycle`-Costing:

- `bicycle_type`: `road`, `hybrid`/`city`, `cross`, `mountain`. Je Typ eigene
  Geschwindigkeitsfaktoren pro Oberfläche und eine „schlechteste erlaubte“ Oberfläche;
  `mountain` darf bis *path*.
- MTB-Trails (`Use::kMountainBike`) werden bei `bicycle_type=mountain` bevorzugt behandelt.
- Einstellbar: `cycling_speed`, `use_roads` (0–1), `use_hills` (0–1), `avoid_bad_surfaces`
  (0–1; bei 1 werden zu schlechte Oberflächen ganz gesperrt), `use_living_streets`,
  `use_ferry`, diverse Strafwerte.
- Fußgänger: `walking_speed`, `max_hiking_difficulty` 0–6 (= `sac_scale`; **Standard ist 1**,
  muss für Bergwandern hochgesetzt werden), `use_hills`, `use_tracks`, `step_penalty`.

**Grenzen:**

1. Es gibt **keine** Laufzeit-Option „maximal S2“. Der `mtb:scale`-Wert selbst wird nicht pro
   Kante gespeichert, nur die daraus abgeleitete Oberflächenklasse; S2 bis S5 sind für den
   Router ununterscheidbar.
2. `mtb:scale` schaltet Fahrrad-Zugang frei, auch auf Wegen, die sonst als reine Fußwege
   gelten würden. Das ist für MTB erwünscht, kann aber rechtlich gesperrte Wege einschließen,
   wenn OSM kein `bicycle=no` trägt.
3. Kein eigenes MTB-Costing; „MTB“ ist `bicycle` mit `bicycle_type=mountain`.

Unsere Profile (Vorschlag, in `core-routing` als Daten, in der UI einstellbar):

| Profil | Costing | Voreinstellung |
|---|---|---|
| Rennrad/Straße | `bicycle` | `road`, `avoid_bad_surfaces` 0.8, `use_roads` 0.6 |
| Trekking | `bicycle` | `hybrid`, `avoid_bad_surfaces` 0.4, `use_roads` 0.3 |
| MTB | `bicycle` | `mountain`, `avoid_bad_surfaces` 0.0, `use_roads` 0.1, `use_hills` 0.8 |
| Wandern | `pedestrian` | `max_hiking_difficulty` 3 (einstellbar 1–6), `use_hills` 0.8 |
| Auto | `auto` | Standard |

Wie gut das MTB-Profil in der Praxis Trails wählt, wird in M5 an echten Beispielrouten
geprüft und in der Doku festgehalten.

**S-Grad-Begrenzung (entschieden):** Wege ab `mtb:scale` 3 werden für Fahrräder gesperrt.
Umsetzung beim Kachelbau über ein angepasstes `graph.lua`; die Schwelle ist ein Parameter von
`build-valhalla-tiles.sh` (`--max-mtb-scale`, Standard 2 = S0–S2 befahrbar) und wird im
Regionsmanifest vermerkt, damit die App sie anzeigen kann. Die Schwelle gilt damit fest pro
Kachelsatz; ändern heißt Routingkacheln der Region neu bauen. Eine pro Route in der App
einstellbare Schwelle ginge nur mit einem Eingriff in Valhallas C++-Kern (Wert pro Kante
speichern und im Costing auswerten) – das ist ein eigener Fork und nicht Teil von Phase 1.

Höhen im Valhalla-Graph (für `use_hills`): Der Kachelbau braucht Höhendaten im
SRTM-HGT-Format; das Skript lädt sie über `valhalla_build_elevation`. Auflösung ~30 m, reicht
für die Steigungsgewichtung.

---

## 4. Kartendaten: Quellen, Lizenzen, Attribution

### 4.1 Basiskarte (Vektor)

Die fertigen Protomaps-Kacheln (Tagesbuilds, Planet ~120 GB, Zoom 0–15, per `pmtiles extract`
ausschneidbar) wären der einfachste Weg. Ihr Schema enthält aber **weder** `sac_scale` noch
`mtb:scale`, `surface`, `tracktype`, **noch** Wander-/Radrouten-Relationen oder Gipfelhöhen.
Damit lässt sich „MTB-Trails hervorgehoben, Wander- und Radwege“ nicht umsetzen.
Das OpenMapTiles-Schema hat immerhin `mtb_scale`, `surface` (befestigt/unbefestigt) und
Gipfelhöhen, aber kein `sac_scale`.

**Empfehlung:** eigene Kacheln je Region mit **Planetiler** (Apache-2.0, v0.10.2, aktiv) aus
Geofabrik-Extrakten bauen. Grundlage ist das OpenMapTiles-Profil
(`planetiler-openmaptiles`, aktiv), ergänzt um einen eigenen Layer `outdoor` mit
`sac_scale`, `mtb_scale`, `mtb_scale_uphill`, `tracktype`, `surface`, `trail_visibility`,
Wander-/Rad-/MTB-Routenrelationen (Netz, Ref, Farbe/`osmc:symbol`), Skipisten/-routen
(`piste:type`), Hütten, Gipfel. Planetiler läuft per Docker (Java muss nicht installiert sein).

| Quelle | Inhalt | Lizenz | Attribution |
|---|---|---|---|
| OpenStreetMap über Geofabrik-Extrakte | Rohdaten (PBF) | ODbL 1.0 | „© OpenStreetMap-Mitwirkende“ |
| OpenMapTiles-Schema | Kachelschema | Code BSD-3, Schema CC-BY 4.0 | „© OpenMapTiles“ |
| Planetiler | Werkzeug | Apache-2.0 | keine in der Karte |
| Protomaps Basemaps (Alternative) | fertige Kacheln | ODbL (Produced Work), Code BSD-3 | „© OpenStreetMap“ |
| OpenFreeMap (nur M2, online) | gehostete OpenMapTiles-Kacheln, ohne Schlüssel | Daten ODbL | „© OpenStreetMap © OpenMapTiles“, OpenFreeMap-Hinweis |

M2 nutzt für die Online-Karte OpenFreeMap (Erreichbarkeit geprüft); ab M3 eigene PMTiles.
Schriften/Glyphen und Sprites werden mit der App ausgeliefert (Noto, SIL OFL 1.1), damit die
Karte ohne Netz Beschriftungen hat.

### 4.2 Höhenmodell (DEM)

**Empfehlung: Mapterhorn.** Fertige Terrarium-kodierte WebP-Kacheln (512 px) als PMTiles;
`planet.pmtiles` Zoom 0–12 (355,6 GB), darüber hochaufgelöste Gebietsdateien bis Zoom 17
(gesamt ~9,9 TB). Ausschnitte per `pmtiles extract --bbox` direkt über HTTP-Range, ohne die
großen Dateien zu laden. Code BSD-3; aktiv (letzter Push 2026-10-31, v0.0.13 – junges
Projekt, Versionsnummer beachten).

Die Lizenz richtet sich nach den **Quelldaten** je Gebiet. Für die Alpen relevant:

| Gebiet | Quelle in Mapterhorn | Auflösung | Lizenz | Attribution |
|---|---|---|---|---|
| Schweiz | swissALTI3D | 0,5 m | Open Government Data (swisstopo) | „© swisstopo“ |
| Bayern | DGM1 | 1 m | CC BY 4.0 | „Bayerische Vermessungsverwaltung – www.geodaten.bayern.de“ |
| Österreich | DGM Österreich | 10 m | CC BY 4.0 | „Land Kärnten – data.gv.at“ / geoland.at |
| Italien | TINITALY 1.1 | 10 m | CC BY 4.0 | „TINITALY/INGV“ (Zitat der Publikation) |
| Frankreich | IGN LiDAR HD / RGE ALTI | 0,5–5 m | Licence Ouverte 2.0 | „© IGN“ |
| Slowenien | DMR (ARSO) | 1 m | CC BY 4.0 | „ARSO“ |
| Rest der Welt | Copernicus GLO-30 | 30 m | Copernicus-Lizenz (frei, offen) | „© DLR e.V. 2010–2014 und © Airbus Defence and Space GmbH 2014–2018, bereitgestellt unter COPERNICUS durch die Europäische Union und ESA“ |

Mapterhorn veröffentlicht die vollständige Liste maschinenlesbar (`attribution.json`, 151
Quellen). Das Regions-Build-Skript schreibt die für die Region zutreffenden Einträge ins
Regionsmanifest; die App zeigt sie in der Kartenattribution und in einem „Über“-Dialog.

Österreich hat in Mapterhorn nur 10 m. Die Bundesländer bieten teils 1-m-ALS-DGMs (CC BY 4.0);
ob und wie man die einbindet, ist als mögliche spätere Verbesserung notiert, nicht Teil von
Phase 1.

### 4.3 Hangneigung

Wird von uns aus dem DEM abgeleitet (`slope-tiles`), erbt also Lizenz und Attribution des
DEM. Klassen wie üblich in Lawinenkarten: 30–35° gelb, 35–40° orange, 40–45° rot, > 45°
violett; < 30° transparent.

### 4.4 Regionspaket

Eine Region ist ein Verzeichnis mit Manifest:

```
<region-id>/
  manifest.json      # Name, BBox, Version, Dateien mit Größe + SHA-256, Attributionen
  basemap.pmtiles    # Vektor, OpenMapTiles-Schema (Planetiler)
  outdoor.pmtiles    # Vektor-Overlay: Wege (sac_scale, mtb:scale …), Routen, Pisten, Gipfel, Hütten
  dem.pmtiles        # Terrarium, WebP
  slope.pmtiles      # Hangneigung, Raster
  routing.tar        # Valhalla tile_extract
```

Stand M3: `scripts/build-region.sh <id>` liest `regions/<id>.json` und erzeugt `basemap`,
`outdoor`, `dem` und das Manifest sowie `catalog.json` (Liste aller Pakete; Ordner oder URL
davon ist der „Katalog“ der App). Testregion `isarwinkel-achensee`: 62 MB (Basiskarte 14 MB,
Overlay 5 MB, DEM bis Zoom 13 43 MB). Installierte Regionen stehen nicht in der Datenbank,
sondern werden aus den Verzeichnissen mit gültigem Manifest gelesen (eine Quelle der Wahrheit).

`scripts/build-region.sh <name> <bbox|polygon>` erzeugt das Paket auf dem Desktop
(Geofabrik-PBF → Planetiler; Mapterhorn → `pmtiles extract`; `slope-tiles`; Valhalla-Docker).
In Phase 3 läuft dasselbe Skript auf dem VPS, und die Pakete werden als statische Dateien mit
Range-Unterstützung ausgeliefert.

`core-maps` lädt Pakete von einer konfigurierbaren Katalog-URL (HTTP oder `file://`),
mit Fortsetzen nach Abbruch, SHA-256-Prüfung, atomarem Umbenennen nach Abschluss,
Speicherbedarfsanzeige vor und nach dem Download, Löschen.

---

## 5. Datenmodell (sync-fähig)

SQLite, Migrationen ab der ersten Version (eigener Runner über `PRAGMA user_version`; Tabellen für Routen kommen als weitere Migration mit M5; Regionen brauchen keine Tabelle, siehe 4.4).

Gemeinsam für alle synchronisierbaren Tabellen: `id` (UUIDv7, als 16-Byte-BLOB),
`created_at`, `updated_at` (UTC, Millisekunden), `deleted_at` (Soft-Delete, sonst NULL),
`rev` (lokaler Änderungszähler für späteren Sync).

| Tabelle | Inhalt |
|---|---|
| `tracks` | Name, Aktivität, Zustand (`recording`/`paused`/`finished`), Start/Ende, gecachte Statistik, Statistik-Parameter-Version |
| `track_segments` | gehört zu Track, laufende Nummer (neues Segment nach Pause/Absturz) |
| `track_points` | `segment_id`, `seq`, Zeit, Lat, Lon, Höhe (GPS), Höhe (DEM, optional), Genauigkeit, Geschwindigkeit, Kurs |
| `routes` | Name, Profil, Optionen, Geometrie, Distanz, Dauer, `manual` ja/nein |
| `route_waypoints` | gehört zu Route, Reihenfolge, Position, Name |
| `regions` | installierte Regionen, Version, Größe, Pfad, Status |
| `settings` | Schlüssel/Wert (Statistik-Parameter je Aktivität, Ortungsintervall, UI) |

`track_points` bekommen **keine** eigene UUID und keine Zeitstempelspalten für Sync (meine
Entscheidung): Punkte sind unveränderlich und gehören immer zu genau einem Track. In Phase 3
wird ein fertiger Track als Ganzes übertragen (Metadaten + komprimierter Punkteblock). Eine
UUID pro Punkt würde Speicher und Sync-Aufwand vervielfachen, ohne Nutzen.

---

## 6. Algorithmen Statistik

Alle Parameter sind je Aktivität konfigurierbar (Tabelle `settings`, Standardwerte im Code).
Die Statistik ist eine reine Funktion `(Punkte, Parameter) → Statistik` und damit testbar und
jederzeit neu berechenbar.

### 6.1 Vorverarbeitung / Ausreißerfilter

In dieser Reihenfolge, pro Segment:

1. **Ungültige Punkte verwerfen:** fehlende Koordinaten, Zeit nicht streng monoton steigend,
   exakte Duplikate.
2. **Genauigkeitsfilter:** Punkte mit gemeldeter horizontaler Genauigkeit schlechter als
   `max_hdop_m` (Standard 50 m) verwerfen, sofern der Empfänger den Wert liefert.
3. **Sprungfilter:** Ein Punkt ist *verdächtig*, wenn die aus Distanz/Zeit zum letzten
   akzeptierten Punkt berechnete Geschwindigkeit `max_plausible_speed` übersteigt **oder**
   die implizierte Beschleunigung `max_accel` (Standard 6 m/s²) überschreitet.
   Ein verdächtiger Punkt wird trotzdem behalten, wenn nur seine **Zeit** nicht stimmt, die
   Position aber auf dem Weg liegt (häufig bei Handy-Aufzeichnungen: sekundenlang kein
   Fortschritt, dann 150 m in einer Sekunde). Das gilt, wenn
   - die Durchschnittsgeschwindigkeit über die letzten `speed_window_s` Sekunden plausibel
     ist **und**
   - der Punkt kein Zacken ist, der Weg also nicht zu ihm hin und gleich wieder zurück führt
     (Umweg über den Punkt > 2 × direkte Strecke zum Folgepunkt + 2 × `min_move_m`).

   Alle anderen verdächtigen Punkte werden verworfen. Nach `max_reject_run` (Standard 5)
   aufeinanderfolgenden Verwerfungen wird der nächste Punkt als neuer Anker akzeptiert
   (echter Ortswechsel). Über einen solchen Neuanker hinweg wird keine Distanz gezählt.
4. **Stillstandsrauschen:** Punkte, die weniger als `min_move_m` (Standard 3 m) vom letzten
   akzeptierten Punkt entfernt sind, tragen nicht zur Distanz bei (GPS-Wandern im Stand).

### 6.2 Distanz

Summe der Haversine-Distanzen zwischen akzeptierten Punkten (Erdradius 6 371 008,8 m). Die
Höhendifferenz wird nicht in die Distanz eingerechnet (übliche Konvention; als Option
`distance_3d` vorgesehen, Standard aus).

### 6.3 Geschwindigkeit

Je Punkt wird die Geschwindigkeit über ein Zeitfenster um den Punkt bestimmt. Das Fenster
reicht immer bis zu den nächsten Fixes jenseits von ±Fenster/2, umfasst also mindestens die
volle Fensterdauer (echte Aufzeichnungen haben unregelmäßige Abstände).

- Liefert der Empfänger für alle Punkte eine Dopplergeschwindigkeit, gilt deren Median im
  Fenster.
- Sonst gibt es zwei Schätzer aus den Positionen:
  - **Nettoverschiebung** (Luftlinie über das Fenster / Dauer) für die Bewegungserkennung.
    Positionsrauschen im Stand hebt sich auf.
  - **Zeitgewichteter Median der Einzelgeschwindigkeiten** (Fix zu Fix) im Fenster für die
    Maximalgeschwindigkeit: die Geschwindigkeit, die während der Hälfte der Fensterzeit
    nicht überschritten wurde. Falsche Zeitstempel oder Positionen können ihn nicht anheben,
    auch nicht viele sehr kurze Intervalle hintereinander.
- **Max-Speed:** Maximum des Median-Schätzers mit Fenster `speed_window_s` (Standard 10 s).
- **Durchschnitt:** Distanz / Zeit in Bewegung. Zusätzlich Distanz / Gesamtzeit.

Befund an den echten Komoot-Aufzeichnungen: Die Zeitstempel sind stellenweise gestaucht –
mehrere Sekunden ohne Fortschritt, danach 140–150 m in einer Sekunde, bei plausibler Spur.
Die Position stimmt also, die Zeit nicht. Mit Nettoverschiebung über 5 s ergab das 84 km/h
Spitze, mit dem zeitgewichteten Median über 10 s 64 km/h (die schnellsten 30 s der Tour
liegen bei 64,5 km/h).

### 6.4 Zeit

- **Gesamtzeit:** je Segment letzter minus erster Zeitstempel, über alle Segmente summiert.
  Eine Pause beginnt ein neues Segment und zählt daher nicht mit.
- **Zeit in Bewegung:** Summe der Intervalle, in denen die Punktgeschwindigkeit (Fenster
  `move_window_s`: 5 s auf dem Rad, 15 s zu Fuß/auf Ski) mindestens `v_move` beträgt.
  Hysterese: Ein Abfall unter die Schwelle, der kürzer als `stop_delay_s` dauert, zählt
  weiter als Bewegung; dauert er länger, zählt er vollständig als Stillstand. 
- **Lücken:** Über ein Intervall länger als `max_gap_s` (Standard 30 s; Tunnel, App im
  Hintergrund) ist nur Strecke und Dauer bekannt. Die Strecke zählt voll. Als Bewegungszeit
  wird die Zeit angerechnet, die man mit dem Durchschnittstempo des übrigen Tracks für diese
  Strecke braucht, höchstens aber die Dauer der Lücke. Geschwindigkeitsfenster reichen nie
  über eine Lücke hinweg.

### 6.5 Höhenmeter

1. Höhenquelle wählen: GPS-Höhe oder – wenn Option aktiv und DEM für den Punkt vorhanden –
   **DEM-Höhe** (bilinear interpoliert aus der höchsten verfügbaren Zoomstufe). Mischung
   innerhalb eines Tracks wird vermieden: DEM nur, wenn es für ≥ 99 % der Punkte verfügbar
   ist. (DEM-Korrektur folgt mit M6; M1 rechnet mit den Höhen aus dem Track.)
2. Glätten: gleitender Mittelwert über eine **Wegstrecke** (nicht Punktanzahl), Standard
   `ele_smooth_m` je Aktivität. Das ist unabhängig vom Aufzeichnungsintervall; Punkte im
   Stand liegen auf derselben Wegmarke und werden gemeinsam gemittelt.
3. Hysterese: Im Anstieg wird jeder neue Höchstwert sofort verbucht. Ein Richtungswechsel
   wird erst erkannt, wenn die geglättete Höhe um mindestens `ele_threshold_m` vom letzten
   Extremwert abweicht. So gehen Gipfel und Senken vollständig ein, Rauschen unterhalb der
   Schwelle gar nicht.

Min-/Max-Höhe werden aus den geglätteten Werten bestimmt.

### 6.6 Standardwerte je Aktivität (Vorschlag, wird in M1 an echten Tracks kalibriert)

| Parameter | Rad | MTB | Wandern | Skitour |
|---|---|---|---|---|
| Bewegungsschwelle `v_move` | 3,0 km/h | 2,0 km/h | 1,0 km/h | 0,8 km/h |
| `stop_delay_s` | 5 s | 5 s | 10 s | 10 s |
| `move_window_s` | 5 s | 5 s | 15 s | 15 s |
| `max_plausible_speed` | 110 km/h | 90 km/h | 25 km/h | 110 km/h |
| `ele_smooth_m` | 60 m | 40 m | 30 m | 30 m |
| `ele_threshold_m` (Track-Höhen; mit DEM-Korrektur ab M6: 2 m) | 3 m | 3 m | 3 m | 3 m |
| Ortungsintervall | 1 s | 1 s | 3 s | 3 s |

### 6.7 Abgleich mit Komoot (Stand M2)

Zwei echte Rad-Aufzeichnungen (`fixtures/gpx/real/`), Vergleichswerte aus der Komoot-App.
Komoots Verfahren sind nicht bekannt; die Werte sind Plausibilitätsreferenz, keine Wahrheit.
Ein Test hält die Abweichung in Grenzen (Distanz ±1 %, Bewegungszeit ±10 %, Höhenmeter ±20 %).

| | Sylvensteinsee: wir | Komoot | Rottach-Egern: wir | Komoot |
|---|---|---|---|---|
| Distanz | 27,60 km | 27,7 km | 31,77 km | 31,9 km |
| Zeit in Bewegung | 0:56:46 | 0:54:24 | 1:39:54 | 1:32:19 |
| Ø in Bewegung | 29,2 km/h | 30,7 km/h | 19,1 km/h | 20,7 km/h |
| Aufstieg / Abstieg | 134 / 144 m | 160 / 170 m | 234 / 289 m | 250 / 310 m |

- **Distanz:** 0,4 % unter Komoot; der Rest ist die Stillstandsschwelle `min_move_m`.
- **Bewegungszeit:** Wir liegen 4–8 % höher. Die Aufzeichnungen enthalten Lücken (bis 13 min
  ohne Fix bei 2,9 km Strecke). Komoot rechnet dafür fast keine Bewegungszeit an, was für
  2,9 km nicht stimmen kann; wir rechnen das Durchschnittstempo an (6.4).
- **Höhenmeter:** Komoots Zahl entspricht der ungefilterten Summe aller Anstiege in den
  (bereits DEM-geglätteten) Höhen der Datei. Wir glätten und verlangen 3 m Mindesthub und
  liegen 6–16 % darunter. Mit 5 m Schwelle waren es bis zu 29 %.

### 6.8 Hangneigung (`slope-tiles`)

- Eingabe: DEM-PMTiles (Terrarium: `h = R·256 + G + B/256 − 32768`).
- Je Ausgabekachel wird die DEM-Kachel mit 1 Pixel Rand aus den 8 Nachbarn geladen
  (keine Kanten an Kachelgrenzen).
- Gradient nach Horn (3×3). Die Pixelgröße in Metern hängt in Web-Mercator von der Breite ab:
  `res = 156543,03 · cos(φ) / 2^z · (256 / Kachelgröße)`; sie wird pro Zeile berechnet.
- Neigung `= atan(√(dz/dx² + dz/dy²))`, Einfärbung nach den vier Klassen, Ausgabe als
  verlustfreie WebP-/PNG-Kacheln mit Alphakanal.
- **Rechenzoom:** Neigung ist maßstabsabhängig. Gerechnet wird auf einer festen Stufe mit
  ~5–10 m Bodenauflösung (bei 512-px-Kacheln Zoom 13 ≈ 6,5 m auf 47° N); gröbere Stufen
  entstehen durch Verkleinern, feinere durch Overzoom in MapLibre. Feiner zu rechnen (1-m-DGM)
  ergäbe ein verrauschtes Bild, das für Lawinenbeurteilung irreführend wäre. Parameter
  `--zoom` einstellbar.
- Parallel über alle Kerne, Speicher begrenzt (Kacheln streamen, identische Kacheln –
  z. B. vollständig transparent – werden von PMTiles nur einmal gespeichert).

---

## 7. Tauri-Schnittstelle (Skizze)

Commands (Frontend → Rust): `recording_start(activity)`, `recording_pause`, `recording_resume`,
`recording_stop`, `recording_recover`, `tracks_list`, `track_get`, `track_delete`,
`track_import_gpx(path)`, `track_export_gpx(id, path)`, `track_recompute_stats(id)`,
`regions_catalog`, `region_download(id)`, `region_cancel(id)`, `region_delete(id)`,
`route_compute(request)`, `route_save`, `route_export_gpx`, `elevation_profile(line)`,
`settings_get/set`, `location_source_set(kind, params)`.

Events (Rust → Frontend): `location:fix`, `recording:update`, `region:progress`,
`region:state`.

Geometrien gehen als GeoJSON ans Frontend; lange Tracks werden einmal vollständig, danach nur
noch als Zuwachs übertragen.

---

## 8. Vorbereitung auf spätere Phasen

### 8.1 Phase 2 – Anbindung des Rust-Kerns auf iOS/Android

| | A: Tauri Mobile | B: Native UI + UniFFI |
|---|---|---|
| UI | dasselbe TypeScript-Frontend im WebView | SwiftUI bzw. Jetpack Compose |
| Karte | MapLibre GL JS im WebView | MapLibre Native (Metal/Vulkan/OpenGL ES) |
| Hintergrund-Ortung | eigenes Tauri-Plugin in Swift/Kotlin nötig; das WebView wird im Hintergrund angehalten | natives Service/`CLLocationManager` ruft den Rust-Kern direkt |
| Akku/Performance | WebView + JS-Kartenrenderer dauerhaft aktiv; höherer Verbrauch, träger bei großen Tracks | nativer Renderer, kein WebView; Aufzeichnung läuft ohne UI-Prozesslast |
| Aufwand | geringer am Anfang, UI wird wiederverwendet | UI zweimal neu (dünn, da keine Fachlogik) |
| Wiederverwendung | Frontend + Styles | Kern-Crates + Style-JSONs |

**Empfehlung: Variante B.** Die Aufzeichnung muss stundenlang mit ausgeschaltetem Bildschirm
laufen. Dafür braucht es ohnehin native Schichten (iOS: `CLLocationManager` mit
Hintergrundmodus; Android: Foreground Service mit Fused Location Provider), und die sollen
Fixes direkt an `core-track`/`core-storage` geben, ohne dass ein WebView lebt. MapLibre Native
ist auf dem Handy deutlich sparsamer als GL JS im WebView. Der Preis ist, dass das
TypeScript-Frontend nicht wiederverwendet wird – deshalb bleibt es in Phase 1 bewusst dünn.

Konsequenzen schon jetzt:

- Push-basierter `LocationSource`-Trait (UniFFI-Callback-fähig).
- Kern-API als gewöhnliche Funktionen/Objekte mit serialisierbaren Typen; die
  Tauri-Commands sind nur eine von mehreren möglichen Hüllen.
- Style-JSONs ohne GL-JS-Spezialitäten, soweit möglich (Ausnahme Höhenlinien, siehe 2.5).
- CI: `cargo check` der Kern-Crates für `aarch64-apple-ios`, `aarch64-linux-android`,
  `armv7-linux-androideabi`, `x86_64-linux-android` – ohne die Features `valhalla`/`gpsd`.
  (iOS-`cargo check` funktioniert unter Linux für reines Rust; gebündeltes SQLite braucht für
  die Zielplattform einen C-Compiler – in CI über das Android-NDK bzw. einen macOS-Runner für
  iOS. Details werden in M1 geklärt und ggf. hier nachgetragen.)
- Valhalla auf Mobilgeräten ist machbar (`Rallista/valhalla-mobile`, MIT, aktiv), aber der
  Rust-Crate nennt iOS/Android nicht als unterstützte Plattform. Das ist ein Phase-2-Risiko
  mit eigenem Spike; der Trait `Router` hält die Tür offen.

### 8.2 Phase 3 – Skizze

- **Server:** Rust, Axum + PostgreSQL (sqlx), hinter Caddy (TLS). Ein Binary, Docker Compose.
- **Authentifizierung:** wenige private Nutzer. Passwort mit Argon2id, kurzlebiges
  Zugriffstoken + rotierendes Refresh-Token pro Gerät; Geräte einzeln widerrufbar. Keine
  offene Registrierung (Einladung/Anlage per CLI).
- **Sync-Protokoll:** Der Server vergibt pro Änderung eine monoton steigende Revisionsnummer.
  - *Pull:* „alle Änderungen seit Revision N“ (Metadaten von Tracks/Routen, inkl.
    Soft-Deletes).
  - *Push:* geänderte Datensätze mit ihrer Basisrevision; Konflikt bei Metadaten →
    „letzte Änderung gewinnt“ je Datensatz (über `updated_at`), Löschungen gewinnen nicht
    gegen spätere Änderungen.
  - Trackpunkte: unveränderlicher, komprimierter Block pro Track, Upload nach Abschluss der
    Aufzeichnung, fortsetzbar; identifiziert über Track-UUID + Prüfsumme.
  - Client hält `rev`/`synced_rev` je Datensatz – die Spalten sind ab Phase 1 im Schema.
- **Regionspakete:** statische Dateien mit HTTP-Range, `catalog.json` mit Versionen, Größen,
  SHA-256. Derselbe Downloader wie in Phase 1, nur andere Katalog-URL. Paketbau per
  `build-region.sh` auf dem VPS (Cron).

---

## 9. Qualität

- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`; Workspace-Lints verbieten
  `unwrap()`/`expect()`/`panic!` außerhalb von Tests (`clippy::unwrap_used` usw.).
- Eigene Error-Typen je Crate (`thiserror`), an der Tauri-Grenze in ein serialisierbares
  Fehlerobjekt mit Code und Meldung übersetzt.
- Tests:
  - `core-track`: Statistik gegen echte Fixture-Tracks mit erwarteten Bandbreiten;
    synthetische Tracks mit bekannter Wahrheit (z. B. Kreis, Rampe, eingestreute Sprünge);
    Eigenschaftstests (Distanz ≥ 0, Zeit in Bewegung ≤ Gesamtzeit, Filter idempotent).
  - `core-gpx`: Roundtrip, Dateien verschiedener Hersteller (Garmin, Komoot, Strava, OsmAnd),
    kaputte Dateien.
  - Integration: GPX-Replay → Recorder → SQLite → Export → Vergleich mit dem Original;
    Absturztest (Prozess mitten in der Aufzeichnung beenden, fortsetzen).
  - `slope-tiles`: synthetische Ebene mit bekannter Neigung.
- Performance/Akku: Release-Profil mit `lto = "fat"`, `codegen-units = 1`; keine
  Polling-Schleifen; Ortungsintervall je Aktivität; Karten-Updates gebündelt; Messung in M6
  (CPU im Leerlauf und bei Aufzeichnung, Speicher, Startzeit, Dauer Import eines langen
  Tracks).
- CI (GitHub Actions oder lokal per Skript – siehe offene Fragen): fmt, clippy, Tests,
  `cargo check` für die mobilen Ziele.
- README mit Ubuntu-Build-Anleitung und vollständiger Paketliste.

---

## 10. Meilensteine Phase 1

Nach jedem Meilenstein: anhalten, zeigen, Freigabe abwarten.

| | Inhalt | Abnahme |
|---|---|---|
| **M1** | Workspace, Lints, Release-Profil. `core-track` (Modell, Filter, Statistik), `core-gpx` (lesen/schreiben), Fixtures, Tests. `track-cli stats <datei.gpx> [--activity …]` | CLI gibt Statistik für echte Tracks aus; Tests, clippy, fmt grün; `cargo check` für mobile Ziele |
| **M2** | `core-storage` mit Migrationen, `core-location` (GPX-Replay, gpsd), Recorder mit Absturzsicherheit. Tauri-App mit Online-Karte (OpenFreeMap), Position, Live-Track, Start/Pause/Stopp, Trackliste, GPX-Import/-Export. **Zuerst:** kurzer Test, ob MapLibre im Tauri-WebView auf diesem Rechner flüssig läuft (siehe Risiko R1) | Replay-Track erscheint live; App während Aufzeichnung beenden → Fortsetzen möglich; Integrationstest grün |
| **M3** | `core-maps`: Regionskatalog, Download, Speicheranzeige, Löschen; Kachel-Protokoll; `build-region.sh` (Basiskarte + DEM); Sommerstil mit Höhenlinien, Schummerung, Wegen, MTB-Trails | Netz aus → App startet, Karte der Region vollständig nutzbar |
| **M4** | `slope-tiles`-CLI, Einbindung in `build-region.sh`, Winterstil mit Deckkraftregler | Hangneigung plausibel gegen bekannte Hänge/Referenzkarte; Test mit synthetischer Ebene |
| **M5** | `core-routing` + Valhalla, `build-valhalla-tiles.sh`, alle Profile, Wegpunkte, Höhenprofil, Zeit, GPX-Export; manuelle Route für Skitour; MTB-Befund mit Beispielrouten dokumentiert | Routen offline für alle Profile; Profiloptionen wirken nachvollziehbar |
| **M6** | Feinschliff, DEM-Höhenkorrektur für Tracks in der UI, Performance-Messung, README, Doku der Algorithmen und Datenquellen, Attributionen | Messprotokoll, vollständige Doku |

---

## 11. Risiken

| | Risiko | Einschätzung | Gegenmaßnahme |
|---|---|---|---|
| R1 | **WebKitGTK + NVIDIA + Wayland** (RTX 5090). Tauri warnt, dass WebGL unter WebKitGTK auf NVIDIA still auf einen langsamen Pfad fallen kann | **gemessen am 2026-11-13, unkritisch:** 62 fps im Mittel, 95 % der Bilder < 17 ms, Maximum 29 ms (10 s Zoom/Drehung/Neigung, Vektorkarte, 1400×900, Debug-Build) | Messung jederzeit wiederholbar: `OUTDOOR_FPS_PROBE=1 cargo run -p outdoor-desktop`. In M6 erneut mit Topo-Stil, Schummerung und großem Fenster messen |
| R2 | Valhalla-Rust-Bindings jung (wenig Nutzer, docs.rs-Build defekt), C++-Build schwergewichtig (lange Kompilierzeit) | mittel | Feature-Flag, Trait `Router`; Ausweichwege: eigener `cxx`-Wrapper oder Valhalla als lokaler Unterprozess |
| R3 | Valhalla auf iOS/Android über denselben Crate nicht belegt | mittel (Phase 2) | eigener Spike in Phase 2; `valhalla-mobile` als Referenz |
| R4 | MTB-Routing: keine S-Grad-Begrenzung zur Laufzeit | sicher (Befund) | Sperre ab S3 beim Kachelbau, Schwelle als Skriptparameter (siehe 3.3) |
| R5 | Eigenes Kachelschema bedeutet eigenen Style-Aufwand (Sommerstil von Grund auf) | mittel | auf OpenMapTiles-Profil und einem bestehenden freien Style aufbauen, nur Outdoor-Layer ergänzen |
| R6 | **Plattenplatz:** 89 GB frei (96 % belegt). DEM bis Zoom 17 für große Alpenregionen, Planetiler-Zwischendateien und Valhalla-Kacheln können das sprengen | mittel | Testregion klein halten; DEM-Maxzoom je Region begrenzen (z. B. 14); Größen mit `--dry-run` vorab anzeigen |
| R7 | Mapterhorn ist jung (v0.0.x), URLs/Struktur können sich ändern | niedrig–mittel | nur im Build-Skript verwendet, nicht in der App; Copernicus GLO-30 direkt als Rückfallquelle |
| R8 | Höhenlinien-Plugin nur für GL JS | niedrig (Phase 2) | vorberechnete Höhenlinien-Kacheln im Regions-Build ergänzen |
| R9 | MapLibre GL JS 6 ist neu (Juli 2026, ESM-only, WebGL2-Pflicht); `maplibre-contour` 0.1.1 muss dazu passen | erledigt (M3) | funktioniert mit MapLibre 6.18 im Tauri-WebView über `addProtocol` und eigenen Worker |
| R10 | gpsd ohne echte Hardware nur gegen `gpsfake` testbar | niedrig | Protokolltests mit aufgezeichneten gpsd-Strömen |

---

## 12. Offene Punkte / getroffene Annahmen

Von mir getroffen (bitte beim Lesen prüfen):

1. Zusätzliches Crate `core-location`; zusätzliches Tool `track-cli`.
2. Eigener GPX-Parser auf `quick-xml` und eigener gpsd-Client statt der veralteten Crates.
3. Kacheln über ein Rust-seitiges URI-Schema statt PMTiles-JS im Frontend.
4. Höhenprofil aus dem eigenen DEM, nicht aus Valhalla.
5. Zusätzliche Zeitschätzung nach DIN 33466 für Wandern/Skitour.
6. `track_points` ohne UUID; Sync eines Tracks als Ganzes.
7. Höhenlinien in Phase 1 clientseitig (`maplibre-contour`).
8. Hangneigung auf fester Rechenstufe (~6,5 m), nicht auf voller DEM-Auflösung.
9. Datenwerkzeuge (Planetiler, Valhalla-Kachelbau) laufen per Docker.
10. Routing in Phase 1 nur innerhalb einer Region.

### Entscheidungen des Auftraggebers (2026-11-13)

1. **Regionen:** Bayern + Tirol. Testdaten klein halten (SSD): entwickelt wird mit einer
   kleinen Testregion im bayerisch-tirolerischen Grenzgebiet; DEM-Maxzoom begrenzt.
2. **Regionsauswahl:** vorgefertigte Pakete aus einem Katalog, kein freies Rechteck.
3. **Basiskarte:** eigene Planetiler-Kacheln mit Outdoor-Layer.
4. **Phase 2:** native UI + MapLibre Native + UniFFI.
5. **gpsd:** kein GPS-Modul am Rechner; Tests gegen simulierte gpsd-Datenströme genügen.
6. **Fixtures:** echte GPX-Tracks liefert der Auftraggeber, unverändert übernehmen. Bis dahin
   synthetische Tracks mit bekannter Wahrheit.
7. **MTB:** ab S3 fürs Rad sperren; Schwelle als Parameter des Kachelbaus (siehe 3.3).
8. **Repo:** lokales Git-Repo, Remote `https://github.com/Isvinter/outdoor_app.git`. Nach
   jedem erreichten Meilenstein pushen – vorher einmal nachfragen. CI über GitHub Actions.
9. **Sprache:** UI zunächst Deutsch, später auf Englisch umstellbar → alle UI-Texte von
   Anfang an in einer Sprachdatei, keine festen Texte im Code. Code und Kommentare Englisch.
