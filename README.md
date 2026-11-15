# outdoor_app

Offline-Outdoor-App für Rad, MTB, Wandern und Skitouren.

| Dokument | Inhalt |
|---|---|
| [PLAN.md](PLAN.md) | Architektur, Entscheidungen, Meilensteine, Risiken |
| [docs/ALGORITHMEN.md](docs/ALGORITHMEN.md) | wie Statistik, Höhenmeter, Hangneigung und Routen berechnet werden |
| [docs/DATENQUELLEN.md](docs/DATENQUELLEN.md) | Kartendaten, Lizenzen, Attribution, verwendete Software |
| [docs/MESSPROTOKOLL.md](docs/MESSPROTOKOLL.md) | Startzeit, CPU, Speicher, Bildrate, Rechenzeiten |

Stand: **Meilenstein 6 (Phase 1 abgeschlossen)** – Desktop-App mit Offline-Karte (herunterladbare Regionen,
Topo-Sommerstil mit Schummerung, Höhenlinien, Wanderwegen und MTB-Trails; Winterstil mit
Hangneigungsklassen, Pisten und Skirouten), Offline-Routenplanung (Wandern, MTB, Trekkingrad,
Rennrad, Auto, manuelle Skitour) mit Höhenprofil, Positionsanzeige, Live-Aufzeichnung
(absturzsicher), GPX-Import/-Export.

## Voraussetzungen (Ubuntu 24.04 / 26.04)

```sh
sudo apt install build-essential curl git pkg-config \
  libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libayatana-appindicator3-dev \
  libssl-dev libxdo-dev \
  clang cmake libboost-dev libprotobuf-dev protobuf-compiler zlib1g-dev   # für Valhalla (Routing)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   # Rust ≥ 1.90
# Node.js ≥ 22 und pnpm ≥ 10 für das Frontend:
curl -fsSL https://get.pnpm.io/install.sh | sh -
```

Für GPS-Hardware zusätzlich `sudo apt install gpsd` (optional; ohne Empfänger genügt die
GPX-Simulation).

Die Routing-Engine Valhalla (C++) wird beim ersten Bauen mitkompiliert (einige Minuten).
Ohne die Valhalla-Pakete baut die App mit `--no-default-features`; dann gibt es nur manuelle
Routen (gerade Linien mit Höhenprofil).

## Bauen und starten

```sh
(cd apps/desktop/ui && pnpm install && pnpm build)   # Frontend, wird in die App eingebettet
cargo run --release -p outdoor-desktop
cargo run --release -p outdoor-desktop --no-default-features   # ohne Valhalla
```

Nach Änderungen am Frontend `pnpm build` wiederholen.

In der App:

- **Position → „GPX-Simulation …“** spielt eine GPX-Datei als GPS-Empfänger ab (1× bis 100×).
  Beispiele liegen in `fixtures/gpx/real/`.
- **Aufzeichnung → Start / Pause / Fortsetzen / Stopp.** Jeder Punkt wird sofort gespeichert.
  Wird die App während einer Aufzeichnung beendet, bietet sie beim nächsten Start
  „Fortsetzen“ oder „Abschließen“ an.
- **Route planen** – Profil wählen, „Wegpunkte setzen“, in die Karte klicken (siehe
  [Routenplanung](#routenplanung)).
- **Tracks** – Liste, Statistik, Umbenennen, GPX-Import und -Export, Löschen. Liegt der Track
  in einer installierten Region, erscheinen zusätzlich Auf- und Abstieg mit Höhen aus dem
  Geländemodell („Gelände“) – GPS-Höhen rauschen; der Track selbst bleibt unverändert.
- **Karten** – Kartenstil wählen, Legende, neue Region auf der Karte wählen und bauen,
  Katalog einstellen, Regionen herunterladen, anzeigen und löschen
  (siehe [Offline-Karten](#offline-karten)).
- **Über / Datenquellen** – Attribution der installierten Regionen und der Software.

Die Daten liegen unter `~/.local/share/de.outdoor-app.desktop/`: Datenbank `outdoor.sqlite`,
Kartenregionen in `regions/`, Beschreibungen der in der App gebauten Regionen in
`region-defs/`, das Protokoll des letzten Baus je Region in `region-build/<id>/build.log`.

| Umgebungsvariable | Wirkung |
|---|---|
| `OUTDOOR_DATA_DIR=<ordner>` | anderes Datenverzeichnis (z. B. zum Testen) |
| `OUTDOOR_FPS_PROBE=1` | bewegt die Karte 10 s, gibt Bildraten als JSON aus, beendet die App |
| `OUTDOOR_STARTUP_PROBE=1` | gibt aus, wann die Seite bereit und die Karte gezeichnet war, beendet die App |
| `OUTDOOR_SELFTEST=<datei.gpx>` | bedient die Oberfläche selbst (Simulation, Start, Pause, Fortsetzen, Stopp), gibt das Ergebnis als JSON aus, beendet die App; `OUTDOOR_SELFTEST_SPEED=<faktor>` stellt das Tempo der Simulation ein (Standard 100) |
| `OUTDOOR_SELFTEST_REGIONS=<katalog>` | lädt die erste Region des Katalogs über die Oberfläche, prüft die Karte, löscht die Region wieder, gibt das Ergebnis als JSON aus, beendet die App |
| `OUTDOOR_SELFTEST_BUILD=<west>,<süd>,<ost>,<nord>;<name>` | wählt das Gebiet über die Oberfläche, baut und installiert es, gibt das Ergebnis als JSON aus, beendet die App |
| `OUTDOOR_REPO=<ordner>` | Projektordner mit `scripts/build-region.sh` für „Region bauen“ (Standard: der Ordner, aus dem die App gebaut wurde) |
| `OUTDOOR_BUILD_SCRIPT=<datei>` | anderes Bauskript (Tests, z. B. `crates/region-builder/tests/fixtures/fake-build.sh`) |
| `OUTDOOR_SELFTEST_ROUTE=<lat>,<lon>;<lat>,<lon>[;…]` | plant über die Oberfläche eine Wanderroute durch die Punkte, speichert sie, gibt das Ergebnis als JSON aus, beendet die App (zusammen mit `OUTDOOR_SNAPSHOT` zeigt das Bild die Route). Ab vier Punkten: letztes Teilstück MTB, Etappenende am zweiten Punkt über das Menü, danach „Letzten entfernen“ |
| `OUTDOOR_SNAPSHOT=<datei.png>` | speichert ein Bild der Karte und beendet die App; mit `OUTDOOR_SNAPSHOT_VIEW=<lon>,<lat>,<zoom>` für einen bestimmten Ausschnitt und `OUTDOOR_SNAPSHOT_STYLE=topo\|winter\|online` für einen Stil |

## Offline-Karten

Die App zeigt Karten aus **Regionspaketen**, die einmal heruntergeladen werden und danach ohne
Netz funktionieren. Ein Paket besteht aus Basiskarte, Outdoor-Overlay (Wege mit
Schwierigkeit, markierte Wander-/Rad-/MTB-Routen, Pisten, Gipfel, Hütten), Höhendaten,
Hangneigung und Routingdaten.
Einen öffentlichen Server mit fertigen Paketen gibt es noch nicht (Phase 3) – Pakete werden
lokal gebaut und aus einem Ordner (oder von einem eigenen Webserver) geladen.

### Region bauen

Braucht zusätzlich Docker, Java ≥ 21, curl und python3. Lädt beim ersten Mal rund 2,5 GB
Quelldaten nach `data/sources/` (OSM-Auszüge, Küstenlinien, Natural Earth); `data/` ist nicht
Teil des Repositories.

```sh
scripts/build-region.sh isarwinkel-achensee      # Testregion, Ergebnis ~90 MB, Dauer ~3 min
scripts/build-region.sh isarwinkel-achensee --steps outdoor,manifest --force   # einzelne Schritte neu
```

Ergebnis: `data/regions/<id>/` (vier `.pmtiles`-Dateien, `routing.tar` und `manifest.json`) sowie
`data/regions/catalog.json` mit allen gebauten Regionen. Weitere Regionen entstehen aus einer
Beschreibung in `regions/<id>.json` (Name, Begrenzung, OSM-Auszüge, Zoomstufen).

Weitere Schalter: `--config <datei>` nimmt die Beschreibung von dort, `--package-dir <ordner>`
schreibt das Paket nach `<ordner>/<id>/` (ohne den Katalog in `data/regions` zu ändern),
`--refresh-osm` lädt die OSM-Auszüge neu. Jeder Schritt wird als
`== step <name> (<n>/<gesamt>)` angekündigt.

### Region in der App bauen

Am Desktop geht das auch ohne Terminal, solange die App aus diesem Projektordner gebaut wurde
und Docker, Java ≥ 21, python3, curl und cargo vorhanden sind (sonst nennt die App, was fehlt):

1. **Karten → Neue Region → Gebiet auf Karte wählen.** Die Karte wechselt dafür auf den
   Online-Stil. Rechteck aufziehen, danach an Ecken und Kanten anpassen.
2. Namen eintragen. Die App zeigt Kantenlängen, geschätzte Paketgröße und wie viele
   OSM-Auszüge sie lädt; die passenden Geofabrik-Auszüge sucht sie selbst, auch über
   Ländergrenzen. Ab geschätzten 5 GB fragt sie nach.
3. **Bauen.** Die Region steht mit „Schritt n/7“ in der Liste und lässt sich abbrechen; ein
   neuer Versuch setzt bei den fertigen Schritten fort. Am Ende wird das Paket geprüft, in
   `regions/` verschoben (nicht kopiert) und angezeigt.

Selbst gebaute Regionen haben zusätzlich **Neu bauen** (frische OSM-Daten); **Löschen**
entfernt auch ihre Beschreibung. Downloads und Zwischendateien teilt die App mit dem Skript
(`data/` im Projektordner). Schlägt ein Bau fehl, zeigt die Liste die letzten Zeilen der
Ausgabe; das ganze Protokoll liegt in `region-build/<id>/build.log`.

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

## Routenplanung

Routen werden vollständig auf dem Gerät berechnet (Valhalla im Prozess, Routingdaten aus dem
Regionspaket). Eine Route besteht aus **Teilstücken** zwischen je zwei Wegpunkten; jedes
Teilstück hat sein eigenes Profil und muss in einer installierten Region liegen. So lassen
sich lange und gemischte Touren planen (z. B. mit dem Rad zum Berg, dann zu Fuß weiter).

1. Unter **Route planen** ein Profil wählen. Die Regler darunter gehören zum Profil
   (z. B. Wandern: Schwierigkeit bis T1–T6 nach SAC-Skala, Steigungen meiden ↔ egal).
2. **Wegpunkte setzen** drücken und in die Karte klicken: erster Klick Start, weitere Klicks
   Zwischenziele und Ziel. Jeder neue Punkt bekommt ein Teilstück mit dem gewählten Profil;
   ein Wechsel von Profil oder Reglern gilt für das letzte Teilstück und alle neuen.
   Nur geänderte Teilstücke werden neu berechnet; **Letzten entfernen** nimmt nur das
   letzte Teilstück weg.
3. Punkte lassen sich ziehen. Ein Klick auf einen Punkt öffnet ein Menü: **Etappenende**
   (z. B. ein Nachtlager; eckiger Marker) oder **Entfernen** (die Nachbarn werden mit dem
   Profil des Teilstücks davor verbunden).
4. Angezeigt werden Distanz, Zeit, Auf- und Abstieg, niedrigster/höchster Punkt, bei mehreren
   Etappen eine Zeile je Etappe, und das Höhenprofil (Zeiger über dem Profil zeigt die Stelle
   in der Karte). Karte und Höhenprofil sind nach Profil eingefärbt, darunter steht die
   Legende; Etappengrenzen sind gestrichelt. Höhen kommen aus dem Geländemodell der Region,
   nicht aus dem Router. Ein Teilstück, das sich nicht berechnen lässt, erscheint als
   gestrichelte rote Luftlinie und zählt nicht mit; die übrigen bleiben stehen.
5. **Speichern** legt die Route in der Liste darunter ab (Klick lädt sie wieder, ✕ löscht),
   **Als GPX exportieren** schreibt sie mit Höhen: je Etappe ein Track, je Aktivität ein
   Abschnitt, Etappenenden als Wegpunkte. Routen aus älteren Versionen werden beim Öffnen neu
   berechnet.

Valhalla begrenzt je Anfrage die Summe der Luftlinien (Wandern 250 km, Rad 500 km, Auto
5 000 km). Weil jedes Teilstück einzeln gerechnet wird, gilt das nur noch je Teilstück.

| Profil | Rechnet mit |
|---|---|
| Wandern | Fußwege bis zur gewählten SAC-Schwierigkeit; Gehzeit zusätzlich nach DIN 33466 (4 km/h, 300 Hm/h auf, 500 Hm/h ab) – die Zeit des Routers ist im Gebirge zu kurz |
| MTB | unbefestigte Wege und Pfade, wenig Straße |
| Trekkingrad | Radwege und ruhige Straßen, mäßig schlechter Belag |
| Rennrad | Asphalt |
| Auto | Straßen |
| Skitour (manuell) | gerade Linien zwischen den Wegpunkten, Höhenprofil und DIN-Gehzeit aus dem Gelände; braucht keine Routingdaten |

**MTB-Schwierigkeit:** Wege schwerer als S2 (`mtb:scale` ≥ 3) sind für alle Radprofile
gesperrt. Die Grenze wird beim Bau der Routingdaten festgelegt, nicht pro Route:

```sh
scripts/build-region.sh isarwinkel-achensee --steps routing,manifest --max-mtb-scale 3 --force
```

Danach die Region in der App aktualisieren. Was Valhalla für MTB kann und was nicht (keine
aktive Bevorzugung von Trails, keine Schwierigkeitsgrenze pro Route), steht mit Beispielrouten
in [PLAN.md](PLAN.md), Abschnitt 3.3.

## Kommandozeile

```sh
cargo run --release -p track-cli -- stats tour.gpx                  # Aktivität aus <type>
cargo run --release -p track-cli -- stats tour.gpx --activity mtb   # bike | mtb | hike | ski_tour
cargo run --release -p track-cli -- stats tour.gpx --json
cargo run --release -p track-cli -- stats tour.gpx --set ele_threshold_m=5   # Parameter überschreiben
cargo run --release -p track-cli -- record tour.gpx --db test.sqlite         # GPX in eine Datenbank aufzeichnen
```

## iOS

Die iOS-App (`apps/ios/`, SwiftUI und MapLibre Native) zeigt die Sommer- und Winterkarte
samt Hangneigung offline aus installierten Regionen, dazu die eigene Position. Routen,
Aufzeichnung und Höhenlinien folgen.

Voraussetzungen: ein Mac mit Apple-Chip, Xcode, `brew install xcodegen` und
`rustup target add aarch64-apple-ios aarch64-apple-ios-sim`.

```sh
apps/ios/scripts/build-core.sh               # Rust-Kern als XCFramework und Swift-Bindings (nach jeder Änderung am Kern)
(cd apps/ios && xcodegen generate)           # erzeugt Outdoor.xcodeproj, dann in Xcode das Schema „Outdoor“ starten
apps/ios/scripts/test.sh                     # Unit- und UI-Tests im Simulator (iPhone 17)
```

Signiert wird mit dem „Personal Team“ aus `project.yml` (Profil gilt 7 Tage). Auf dem iPhone
muss der Entwickler einmal unter Einstellungen → Allgemein → VPN & Geräteverwaltung vertraut
werden.

**Region aufs iPhone bringen:** Das iPhone anschließen und im Finder unter dem Gerät →
Dateien → Outdoor den Regionsordner (mit `manifest.json` und den `.pmtiles`) in den Ordner
`Import` ziehen. Das geht auch in der App „Dateien“ unter Auf meinem iPhone → Outdoor → Import.
Dann die App neu starten: Sie prüft die Dateien, installiert die Region und entfernt den
Ordner. Schlägt die Prüfung fehl, meldet die App das, und der Ordner bleibt liegen.

## Entwickeln

```sh
cargo test --workspace                       # mit Valhalla (über die Desktop-App eingeschaltet)
cargo test --workspace --exclude outdoor-desktop --exclude region-builder   # auf dem Mac: ohne Valhalla und ohne den Regionsbau (nur Linux)
cargo test -p core-routing -p core-app       # dieselben Crates ohne Valhalla
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
(cd apps/desktop/ui && pnpm typecheck)
scripts/measure-performance.sh               # Messprotokoll neu aufnehmen (Release-Build, ~3 min)
```

Die Kern-Crates müssen für Mobilgeräte kompilierbar bleiben; das prüft die CI
(`.github/workflows/ci.yml`). Lokal ohne Android-NDK/Xcode geht das für die Crates ohne
SQLite:

```sh
rustup target add aarch64-apple-ios aarch64-linux-android
cargo check -p core-track -p core-gpx -p core-location -p core-routing --target aarch64-linux-android   # core-maps, core-storage: brauchen das NDK
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
| `crates/core-maps` | Offline-Regionen: Katalog, Download, PMTiles lesen und schreiben, Kachelzugriff, Höhen aus dem Geländemodell |
| `crates/core-routing` | Routenprofile, Router-Schnittstelle, Valhalla (Feature `valhalla`), manuelle Route, DIN-Gehzeit |
| `crates/core-app` | Anwendungsdienste; einzige Schnittstelle für Oberflächen |
| `apps/desktop/src-tauri` | Tauri-Hülle: Commands und Events, keine Fachlogik |
| `apps/desktop/ui` | Frontend (TypeScript, MapLibre GL JS), nur Darstellung |
| `crates/core-ffi` | UniFFI-Hülle um `core-app` für die mobilen Apps |
| `apps/ios` | iOS-App (SwiftUI, MapLibre Native): Karte, Regionsimport, Position; keine Fachlogik |
| `crates/region-builder` | nur Desktop: Regionen in der App bauen – Beschreibung, Wahl der Geofabrik-Auszüge, Lauf des Bauskripts mit Fortschritt und Abbruch |
| `tools/track-cli` | Kommandozeile für Statistik und Tests |
| `tools/slope-tiles` | Hangneigungskacheln aus Höhenkacheln |
| `regions/` | Beschreibungen der baubaren Regionen |
| `scripts/` | `build-region.sh` (Regionspaket bauen), `build-valhalla-tiles.sh` (Routingdaten), `make-style.py` (Kartenstile), `measure-performance.sh`, Planetiler-Profil |
| `docs/` | Algorithmen, Datenquellen, Messprotokoll |
| `styles/`, `assets/map/` | Kartenstile und Schriften, von allen Plattformen genutzt |
| `fixtures/` | Testdaten (GPX-Tracks, Mini-Region, Routingkacheln Tegernsee) |
