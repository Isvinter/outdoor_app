# outdoor_app

Offline-Outdoor-App für Rad, MTB, Wandern und Skitouren. Architektur, Datenquellen,
Algorithmen und Meilensteine stehen in [PLAN.md](PLAN.md).

Stand: **Meilenstein 1** – Track-Modell, Statistik, GPX-Import/-Export, CLI.

## Bauen (Ubuntu 24.04 / 26.04)

```sh
sudo apt install build-essential curl git
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   # Rust ≥ 1.90
cargo build --release
```

Weitere Systempakete (Tauri/WebKitGTK, Valhalla) kommen mit den Meilensteinen 2 und 5
hinzu und werden dann hier ergänzt.

## Benutzen

```sh
cargo run --release -p track-cli -- stats tour.gpx                  # Aktivität aus <type>
cargo run --release -p track-cli -- stats tour.gpx --activity mtb   # bike | mtb | hike | ski_tour
cargo run --release -p track-cli -- stats tour.gpx --activity hike --json
```

## Entwickeln

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check

# Kern-Crates müssen für Mobilgeräte kompilierbar bleiben
rustup target add aarch64-apple-ios aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
cargo check -p core-track -p core-gpx --target aarch64-apple-ios
```

Echte GPX-Aufzeichnungen in `fixtures/gpx/real/` werden von den Tests automatisch
eingelesen und auf plausible Statistik geprüft.

## Aufbau

| Pfad | Inhalt |
|---|---|
| `crates/core-track` | Track-Modell, GPS-Filter, Statistik |
| `crates/core-gpx` | GPX 1.1 lesen und schreiben |
| `tools/track-cli` | Kommandozeile: GPX rein, Statistik raus |
| `fixtures/` | Testdaten |
