# outdoor_app

Offline-Outdoor-App für Rad, MTB, Wandern und Skitouren. Architektur, Datenquellen,
Algorithmen und Meilensteine stehen in [PLAN.md](PLAN.md).

Stand: **Meilenstein 2** – Desktop-App mit Online-Karte, Positionsanzeige,
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

Die Datenbank liegt unter `~/.local/share/de.outdoor-app.desktop/outdoor.sqlite`.

| Umgebungsvariable | Wirkung |
|---|---|
| `OUTDOOR_DATA_DIR=<ordner>` | anderes Datenverzeichnis (z. B. zum Testen) |
| `OUTDOOR_FPS_PROBE=1` | bewegt die Karte 10 s, gibt Bildraten als JSON aus, beendet die App |
| `OUTDOOR_SELFTEST=<datei.gpx>` | bedient die Oberfläche selbst (Simulation, Start, Pause, Fortsetzen, Stopp), gibt das Ergebnis als JSON aus, beendet die App |

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
cargo check -p core-track -p core-gpx -p core-location --target aarch64-linux-android
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
| `crates/core-app` | Anwendungsdienste; einzige Schnittstelle für Oberflächen |
| `apps/desktop/src-tauri` | Tauri-Hülle: Commands und Events, keine Fachlogik |
| `apps/desktop/ui` | Frontend (TypeScript, MapLibre GL JS), nur Darstellung |
| `tools/track-cli` | Kommandozeile für Statistik und Tests |
| `fixtures/` | Testdaten |
