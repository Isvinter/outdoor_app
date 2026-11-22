# ZeroGrid: ein Repository für Base, Trail und Sky

Stand: 2026-11-22. Vom Nutzer im Gespräch freigegeben.

## Absicht

Aus den Projekten `outdoor_app` und `zerogrid-sky` (vormals Vario) wird das Ökosystem
**ZeroGrid**:

- **ZeroGrid Base** (Desktop, bisher Desktop-App der `outdoor_app`): Planung, Karten bauen und
  für die mobilen Apps bereitstellen. Soll später deutlich wachsen: KI-Funktionen mit lokalen
  Open-Weight-Modellen, Verwaltung von Bildern, Notizen und Beschreibungen, Flüge und Lufträume.
- **ZeroGrid Trail** (iOS, bisher iOS-App der `outdoor_app`): spezialisierte App für unterwegs.
- **ZeroGrid Sky** (iOS, bisher Vario): spezialisierter Flugcomputer.

Base ist der Schwerpunkt, Trail und Sky beziehen ihre Karten von dort. Das Kartenformat koppelt
alle drei Apps, deshalb gehören sie in ein Repository.

## Ziele

1. Ein Repository `zerogrid` mit der vollständigen Git-Historie beider Projekte.
2. Base und Trail tragen ihre neuen Namen und App-Kennungen. Sky behält seine schon richtige
   Kennung.
3. Sky nutzt `core-maps` über einen lokalen Pfad statt über eine Git-Abhängigkeit.
4. Vorhandene Desktop-Daten (Karten, Routen) bleiben erhalten.
5. Danach: WLAN-Übertragung von der echten Base-Freigabe auf Sky geprüft.

## Nicht-Ziele

- Keine gemeinsame Kartenablage von Trail und Sky (App Group). Das ist ein eigener Schritt.
- Kein QR-Scanner in Sky.
- Keine Angleichung der Sky-Crates an die strengeren Lint-Regeln.
- Keine neuen Funktionen in Base.

## Was auf keinen Fall passieren darf

- Gebaute Kartenpakete werden ungültig.
- Desktop-Daten gehen verloren.
- Funktionierender Flugcode ändert sein Verhalten.
- Heimnetz-Adressen, Wohnort-Koordinaten oder Zugangsdaten landen im öffentlichen Repository.

## Ansätze

1. **Ein Repository (gewählt):** Änderungen am Kartenformat landen in einem Commit über alle
   Apps, und Base kann die Flug-Crates später direkt nutzen.
2. **Zwei Repositories:** sauber getrennt, aber jede Core-Änderung braucht einen zweiten Schritt
   in Sky.
3. **Core-Repository plus drei App-Repositories:** lohnt sich erst bei getrennten Teams.

## Zielstruktur

```
zerogrid/
  apps/base/     Tauri-Desktop-App (bisher apps/desktop)
  apps/trail/    SwiftUI-iOS-App (bisher apps/ios)
  apps/sky/      Tauri-iOS-App (bisher apps/vario-app im Sky-Repository)
  crates/        core-*, region-builder, flight-core, sensor-*, vario-*
  tools/ styles/ assets/ fixtures/ regions/ scripts/ spikes/ docs/
  docs/sky/      Dokumente aus dem Sky-Repository
  testdata/sky/  Testdaten aus dem Sky-Repository
```

Trail und Sky haben je ein eigenes Xcode-Projekt. Ein gemeinsamer Workspace bringt nichts, weil
der geteilte Code in Rust liegt.

## Namen

| | Anzeigename | Kennung | Xcode-Projekt, Target |
|---|---|---|---|
| Base | ZeroGrid Base | `de.zeibig.zerogrid.base` | – |
| Trail | ZeroGrid Trail | `de.zeibig.zerogrid.trail` | `ZeroGridTrail` |
| Sky | ZeroGrid Sky | `de.zeibig.zerogrid.sky` (unverändert) | `vario-app` (unverändert) |

**Bleibt absichtlich gleich:** `outdoor.pmtiles`, die Layer-Namen, `outdoor.sqlite`, das Schema
`outdoor-tiles://`, die Umgebungsvariablen `OUTDOOR_*`, die Crate-Namen `core-*` und `vario-*`,
der Ordner `outdoor` in den App-Daten von iOS. Das sind Daten- und Entwicklerschnittstellen, die
kein Nutzer sieht. Eine Änderung würde gebaute Karten oder vorhandene Daten brechen.

**Wird angepasst:** alle sichtbaren Texte, darunter Fenstertitel, Überschriften, iOS-Berechtigungs-
texte und „Desktop im WLAN (outdoor_app)“ in Sky (künftig „ZeroGrid Base im WLAN“). Der
GPX-Eintrag `creator` wird `ZeroGrid`. README, `PLAN.md` und Pfade in Skripten und CI folgen der
neuen Struktur. Historische Pläne und Specs bleiben unverändert.

## Zusammenführung

- Die Sky-Historie wird mit `git filter-repo` in die Zielpfade umgeschrieben und mit
  `--allow-unrelated-histories` gemergt, in einer frischen Kopie. Die Originale bleiben unberührt.
- Der Cargo-Workspace nimmt die Sky-Crates, die App und das Plugin auf.
- Die Sky-Crates und die Sky-App behalten ihre bisherigen Lint-Regeln als eigene `[lints]`.
- `sensor-bridge` darf weiter `unsafe` nutzen.
- `default-members` lässt die Sky-App und ihr Plugin aus. Ein normales `cargo test` bleibt auf
  Linux ohne Audio-Sonderpakete nutzbar, soweit die Kern-Crates es erlauben.
- Die CI prüft die Sky-Kern-Crates. Die Sky-App kommt erst dazu, wenn sie auf Linux sauber baut.
- `CLAUDE.md` von Sky wandert nach `apps/sky/CLAUDE.md` und wird auf die neuen Pfade angepasst.

## Desktop-Daten

Beim Start von Base gilt: Existiert das neue Datenverzeichnis nicht, aber das alte
(`de.outdoor-app.desktop`), wird das alte per atomarem Umbenennen übernommen. Schlägt das fehl,
startet die App mit einer klaren Fehlermeldung und legt **kein** leeres neues Verzeichnis an.
Existieren beide, wird nichts verschoben. `OUTDOOR_DATA_DIR`-artige Überschreibungen bleiben
vorrangig. Die Logik bekommt Tests.

## iPhone

Trail bekommt durch die neue Kennung eine neue App. Die alte App bleibt installiert, bis Karten
und Routen in Trail per WLAN neu geladen sind. Die App-ID entsteht durch automatisches Signieren
beim ersten Build auf dem Mac. Dieser Schritt und alle Gerätetests laufen dort.

## Reihenfolge

1. Sky auf dem Mac vollständig pushen, dort pausieren.
2. Zusammenführung und Workspace auf dem Feature-Branch, Tests grün.
3. Base und Trail umbenennen, Datenübernahme bauen.
4. PR, Review, Merge. GitHub-Umbenennung `outdoor_app` → `zerogrid` nur nach ausdrücklichem Okay.
5. Mac: Repository neu anbinden, Trail und Sky bauen, auf dem iPhone prüfen. Sky-Release-Build
   mit dem gemeinsamen LTO-Profil einmal im Flugmodus-Test prüfen.
6. `zerogrid-sky` archivieren (nicht löschen), nur nach ausdrücklichem Okay.
7. WLAN-Test Base → Sky nach `docs/sky/handover-linux.md`.

## Dem Umsetzer überlassen

Genaue `filter-repo`-Pfadregeln, Aufteilung in Commits, Form der Fehlermeldung bei der
Datenübernahme und die Behandlung der 11 abweichenden Abhängigkeitsversionen im gemeinsamen
`Cargo.lock`. Dabei gilt: Versionen nur so weit ändern, wie Cargo es verlangt.
