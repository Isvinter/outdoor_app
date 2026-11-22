# Übergabe: Kartenübertragung Linux-Desktop → ZeroGrid Sky (Stand 22.11.2026)

Für die Weiterarbeit auf dem Linux-Rechner. Ziel des nächsten Schritts: Regionen von der
outdoor_app auf dem Linux-Desktop (künftig **ZeroGrid Base**) per WLAN auf **ZeroGrid Sky**
(dieses Repository, iPhone) übertragen, Ende zu Ende mit der echten Desktop-App statt mit
`share-serve` am Mac.

## Was es schon gibt

**Desktop (Repository `Isvinter/outdoor_app`, Branch `main`):**
- `crates/core-share`: kleiner, nur lesender HTTP-Server. Liefert `/catalog.json` (Regionen mit
  Version, Größe, Dateien und SHA-256), die Dateien jeder Region (mit `Range`, also
  fortsetzbar) sowie `/routes.json` und die Routen (die Sky nicht nutzt).
- `apps/base/src-tauri/src/share.rs` und `apps/base/ui/src/share.ts`: Freigabe in der
  Desktop-App ein- und ausschalten, Anzeige der Adresse `http://<IPv4>:<port>/` als Text und
  QR-Code, Hinweis zur Firewall. Selbsttest über `OUTDOOR_SELFTEST_SHARE` (siehe README dort).
- `share-serve` (Binary in `core-share`): derselbe Server ohne Oberfläche, zum Testen.
- Region-Builder mit Flug-Layer `flying` (Stromleitungen, Seilbahnen, Start-/Landeplätze),
  gemergt in `main` (`aa6b27f`).
- Die iOS-App der outdoor_app (künftig ZeroGrid Trail) lädt damit schon Regionen und Routen,
  inklusive QR-Scan; vom Nutzer auf dem iPhone abgenommen (Meilenstein I4).

**ZeroGrid Sky (dieses Repository):**
- `apps/sky/src-tauri/src/share.rs`: liest den Katalog, lädt Regionen über
  `core_maps::RegionStore::install` (Prüfsummen, Fortsetzen, Umschalten erst nach
  vollständigem Download), meldet Fortschritt per Event `region-download`.
- Einstellungen → Karte → „Desktop im WLAN (outdoor_app)“: Adresse von Hand eintippen
  (`192.168.x.y:8765` wird zu `http://192.168.x.y:8765/`), dann „Verbinden“. Liste mit
  Laden, Aktualisieren, Abbrechen, Löschen.
- `core-maps` ist per Git auf Commit `75592d0` der outdoor_app festgelegt (liegt auf `main`).
- iOS: Berechtigung „Lokales Netzwerk“ ist eingetragen und wird beim ersten Verbinden
  abgefragt.
- Auf dem iPhone geprüft gegen `share-serve` am Mac: Laden, Aktualisieren, Abbrechen,
  Fortsetzen. **Nicht** geprüft gegen die echte Freigabe der Desktop-App auf Linux.

## Nächste Schritte auf Linux

1. outdoor_app auf `main` bauen, Desktop-App starten, Region mit Flug-Layer bauen (ältere
   Regionen ohne `flying` zeigen in Sky keine Seilbahnen), Freigabe einschalten.
2. Firewall: Port der Freigabe im Heimnetz freigeben (die Desktop-App zeigt einen Hinweis).
3. iPhone im selben WLAN: in Sky die angezeigte Adresse eintragen, „Verbinden“, Region laden.
   Prüfen: Liste, Laden, Abbrechen und Fortsetzen, Aktualisieren nach Neubau der Region am
   Desktop, Anzeige offline auf der Flugkarte.
4. Ergebnis in `docs/sky/ios-test.md` (Abschnitt Regionen per WLAN) und in der Phasentabelle von
   `CLAUDE.md` (Zeile 6b) eintragen.

## Offene Punkte (Vorschläge, vor Umsetzung mit dem Nutzer abstimmen)

- **QR-Code in Sky:** Der Desktop zeigt die Adresse schon als QR-Code
  (Inhalt genau `http://<IPv4>:<port>/`). Sky hat noch keinen Scanner; die iOS-App der
  outdoor_app nutzt VisionKit, das ließe sich in das Swift-Plugin von Sky
  (`apps/sky/plugins/vario-sensors`) übernehmen.
- **Namen:** Die Desktop-App heißt künftig ZeroGrid Base. Texte in Sky („Desktop im WLAN
  (outdoor_app)“) und in der Desktop-Freigabe anpassen, wenn die Umbenennung der outdoor_app
  ansteht.
- **Später in ZeroGrid Base** (beschlossen am 22.11.2026: Sky bekommt keine eigene
  Linux-App): Flüge (IGC) und Lufträume am Desktop. Die Kern-Crates von Sky (`flight-core`
  usw.) sind dafür plattformunabhängig gehalten.
- `core-maps`-Pin in Sky nur anheben, wenn sich Katalog- oder Paketformat ändern; danach in
  Sky `cargo test --workspace` und den Selbsttest gegen `share-serve` laufen lassen.

## Testen ohne iPhone

```
# in der outdoor_app: Freigabe ohne Oberfläche (--seed-region verschiebt den Ordner,
# also eine Kopie angeben)
cargo run --release -p core-share --bin share-serve -- --data /tmp/share --port 8765 --seed-region /tmp/kopie-der-region
# in Sky: Selbsttest lädt die Region von dort, mit leerem HOME
cargo build -p vario-app && HOME=$(mktemp -d) VARIO_SMOKE_TEST=1 VARIO_SMOKE_SHARE=127.0.0.1:8765 target/debug/vario-app
```

Sky selbst ist eine iOS-App; der Desktop-Build läuft auf dem Mac als Entwicklungswerkzeug.
Auf Linux braucht er die Tauri-Systempakete (`libwebkit2gtk-4.1-dev` usw.) und ist seit
Phase 2b nicht mehr geprüft.

## Regeln, die gelten

- Beide Repositories sind öffentlich: keine IP-Adressen aus dem Heimnetz, keine
  Wohnort-Koordinaten, keine Zugangsdaten committen. Nur nach Rückfrage pushen.
- Vor jedem Commit `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test` (in Sky zusätzlich `--workspace`, wenn die Tauri-Pakete installiert sind).
- Mit dem Nutzer Deutsch, Code und Commits Englisch; der Nutzer ist Rust-Einsteiger.
