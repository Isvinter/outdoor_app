# outdoor_app

Offline-Outdoor-App für Rad, MTB, Wandern und Skitouren. Architektur, Datenquellen,
Algorithmen und Meilensteine stehen in [PLAN.md](PLAN.md).

Stand: **Meilenstein 4** – Desktop-App mit Offline-Karte (herunterladbare Regionen,
Topo-Sommerstil mit Schummerung, Höhenlinien, Wanderwegen und MTB-Trails; Winterstil mit
Hangneigungsklassen, Pisten und Skirouten), Positionsanzeige,
Live-Aufzeichnung (absturzsicher), GPX-Import/-Export.

## Voraussetzungen (Ubuntu 24.04 / 26.04)

```sh
sudo apt install build-essential curl git pkg-config \
  libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libayatana-appindicator3-dev \
  libssl-dev libxdo-dev
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   # Rust ≥ 1.90
# Node.js ≥ 22 und pnpm ≥ 10 für das Frontend:
curl -fsSL https://get.pnpm.io/install.sh | sh -
```

Für GPS-Hardware zusätzlich `sudo apt install gpsd` (optional; ohne Empfänger genügt die
GPX-Simulation). Pakete für Valhalla kommen mit Meilenstein 5 hinzu.

## Bauen und starten

```sh
(cd apps/desktop/ui && pnpm install && pnpm build)   # Frontend, wird in die App eingebettet
cargo run --release -p outdoor-desktop
```

Nach Änderungen am Frontend `pnpm build` wiederholen.

In der App:

- **Position → „GPX-Simulation …“** spielt eine GPX-Datei als GPS-Empfänger ab (1× bis 100×).
  Beispiele liegen in `fixtures/gpx/real/`.
- **Aufzeichnung → Start / Pause / Fortsetzen / Stopp.** Jeder Punkt wird sofort gespeichert.
  Wird die App während einer Aufzeichnung beendet, bietet sie beim nächsten Start
  „Fortsetzen“ oder „Abschließen“ an.
- **Tracks** – Liste, Statistik, Umbenennen, GPX-Import und -Export, Löschen.
- **Karten** – Kartenstil wählen, Katalog einstellen, Regionen herunterladen, anzeigen und
  löschen (siehe [Offline-Karten](#offline-karten)).

Die Daten liegen unter `~/.local/share/de.outdoor-app.desktop/`: Datenbank `outdoor.sqlite`,
Kartenregionen in `regions/`.

| Umgebungsvariable | Wirkung |
|---|---|
| `OUTDOOR_DATA_DIR=<ordner>` | anderes Datenverzeichnis (z. B. zum Testen) |
| `OUTDOOR_FPS_PROBE=1` | bewegt die Karte 10 s, gibt Bildraten als JSON aus, beendet die App |
| `OUTDOOR_SELFTEST=<datei.gpx>` | bedient die Oberfläche selbst (Simulation, Start, Pause, Fortsetzen, Stopp), gibt das Ergebnis als JSON aus, beendet die App |
| `OUTDOOR_SELFTEST_REGIONS=<katalog>` | lädt die erste Region des Katalogs über die Oberfläche, prüft die Karte, löscht die Region wieder, gibt das Ergebnis als JSON aus, beendet die App |
| `OUTDOOR_SNAPSHOT=<datei.png>` | speichert ein Bild der Karte und beendet die App; mit `OUTDOOR_SNAPSHOT_VIEW=<lon>,<lat>,<zoom>` für einen bestimmten Ausschnitt und `OUTDOOR_SNAPSHOT_STYLE=topo\|winter\|online` für einen Stil |

## Offline-Karten

Die App zeigt Karten aus **Regionspaketen**, die einmal heruntergeladen werden und danach ohne
Netz funktionieren. Ein Paket besteht aus Basiskarte, Outdoor-Overlay (Wege mit
Schwierigkeit, markierte Wander-/Rad-/MTB-Routen, Pisten, Gipfel, Hütten) und Höhendaten.
Einen öffentlichen Server mit fertigen Paketen gibt es noch nicht (Phase 3) – Pakete werden
lokal gebaut und aus einem Ordner (oder von einem eigenen Webserver) geladen.

### Region bauen

Braucht zusätzlich Docker, Java ≥ 21, curl und python3. Lädt beim ersten Mal rund 2,5 GB
Quelldaten nach `data/sources/` (OSM-Auszüge, Küstenlinien, Natural Earth); `data/` ist nicht
Teil des Repositories.

```sh
scripts/build-region.sh isarwinkel-achensee      # Testregion, Ergebnis ~71 MB, Dauer ~2 min
scripts/build-region.sh isarwinkel-achensee --steps outdoor,manifest --force   # einzelne Schritte neu
```

Ergebnis: `data/regions/<id>/` (vier `.pmtiles`-Dateien und `manifest.json`) sowie
`data/regions/catalog.json` mit allen gebauten Regionen. Weitere Regionen entstehen aus einer
Beschreibung in `regions/<id>.json` (Name, Begrenzung, OSM-Auszüge, Zoomstufen).

### Region in der App laden

1. Unter **Karten → Katalog** den Ordner `data/regions` wählen („Ordner …“) oder die Adresse
   eines Webservers eintragen, der diesen Ordner ausliefert.
2. Bei der Region **Herunterladen** drücken. Der Download lässt sich abbrechen und wird beim
   nächsten Mal fortgesetzt; jede Datei wird per SHA-256 geprüft, bevor die Region erscheint.
3. Kartenstil **Topo Sommer (offline)** – ab jetzt wird kein Netz mehr gebraucht.

Kartenstil: Wanderwege nach SAC-Skala (braun gestrichelt = Wanderweg, rot = Bergwanderweg,
blau gepunktet = alpin), MTB-Trails farbig hinterlegt nach Singletrail-Skala (grün S0, blau
S1, orange S2, schwarz ab S3), markierte Routen als breite Linien (rot Wandern, blau Rad,
orange MTB).

**Topo Winter (offline)** zeigt statt der Sommerrouten die Hangneigung in den Klassen der
Lawinenkarten (gelb 30–35°, orange 35–40°, rot 40–45°, violett über 45°; Deckkraft per
Regler), dazu Pisten nach Schwierigkeit, Loipen (türkis gestrichelt) und Skirouten (magenta
gestrichelt). Die Hangneigung wird beim Bau der Region mit `tools/slope-tiles` aus den
Höhendaten berechnet (Gradient nach Horn auf Zoom 13, ca. 6,5 m je Pixel; gröbere Stufen
gemittelt). Sie ist eine Planungshilfe aus einem Geländemodell und ersetzt weder
Lawinenlagebericht noch Beurteilung im Gelände.

```sh
cargo run --release -p slope-tiles -- dem.pmtiles slope.pmtiles   # einzeln aufrufen; --help zeigt Optionen
```

Die Stile werden aus `scripts/make-style.py` erzeugt (`styles/topo-summer.json`,
`styles/topo-winter.json`).

Kartendaten: © OpenStreetMap-Mitwirkende (ODbL), Schema © OpenMapTiles (CC BY), Höhendaten
über Mapterhorn (u. a. Bayerische Vermessungsverwaltung, geoland.at, Copernicus); die genauen
Angaben stehen im Manifest jeder Region und in der Karte unten rechts.

## Kommandozeile

```sh
cargo run --release -p track-cli -- stats tour.gpx                  # Aktivität aus <type>
cargo run --release -p track-cli -- stats tour.gpx --activity mtb   # bike | mtb | hike | ski_tour
cargo run --release -p track-cli -- stats tour.gpx --json
cargo run --release -p track-cli -- stats tour.gpx --set ele_threshold_m=5   # Parameter überschreiben
cargo run --release -p track-cli -- record tour.gpx --db test.sqlite         # GPX in eine Datenbank aufzeichnen
```

## Entwickeln

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
(cd apps/desktop/ui && pnpm typecheck)
```

Die Kern-Crates müssen für Mobilgeräte kompilierbar bleiben; das prüft die CI
(`.github/workflows/ci.yml`). Lokal ohne Android-NDK/Xcode geht das für die Crates ohne
SQLite:

```sh
rustup target add aarch64-apple-ios aarch64-linux-android
cargo check -p core-track -p core-gpx -p core-location --target aarch64-linux-android   # core-maps, core-storage: brauchen das NDK
```

Echte GPX-Aufzeichnungen in `fixtures/gpx/real/` werden von den Tests automatisch
eingelesen. Stehen in `reference.json` Vergleichswerte einer anderen App, wird die
Abweichung geprüft.

## Aufbau

| Pfad | Inhalt |
|---|---|
| `crates/core-track` | Track-Modell, GPS-Filter, Statistik, Aufzeichnungs-Zustandsautomat |
| `crates/core-gpx` | GPX 1.1 lesen und schreiben |
| `crates/core-storage` | SQLite: Schema, Migrationen, Tracks, Einstellungen |
| `crates/core-location` | Positionsquellen: GPX-Simulation, gpsd |
| `crates/core-maps` | Offline-Regionen: Katalog, Download, PMTiles-Leser, Kachelzugriff |
| `crates/core-app` | Anwendungsdienste; einzige Schnittstelle für Oberflächen |
| `apps/desktop/src-tauri` | Tauri-Hülle: Commands und Events, keine Fachlogik |
| `apps/desktop/ui` | Frontend (TypeScript, MapLibre GL JS), nur Darstellung |
| `tools/track-cli` | Kommandozeile für Statistik und Tests |
| `tools/slope-tiles` | Hangneigungskacheln aus Höhenkacheln (mit PMTiles-Schreiber) |
| `regions/` | Beschreibungen der baubaren Regionen |
| `scripts/` | `build-region.sh` (Regionspaket bauen), `make-style.py` (Kartenstile), Planetiler-Profil |
| `styles/`, `assets/map/` | Kartenstile und Schriften, von allen Plattformen genutzt |
| `fixtures/` | Testdaten (GPX-Tracks, Mini-Region) |
