# Übergabe Tour Schritt 3 an den Mac (Task 3)

Stand: 2026-11-27 · Branch `feature/tour-export` · Plan
`docs/superpowers/plans/2026-11-27-tour-schritt3-export.md`

## Erledigt (Linux)

- Task 1 (`core-app`): 8ed80b8 — `TourManifest` (`tour.json`, Format `zerogrid-tour`, Version 1),
  `notes_markdown` (`notizen.md`), `package_path`, `App::section_ids`.
- Task 2 (`core-app`, `core-ffi`): c899c8e — `App::export_tour` schreibt die ZIP über `<ziel>.part`,
  Fehlercodes `export_no_space` / `export_failed`; FFI `exportTour(id:path:utcOffsetMin:)`.
- Neue Abhängigkeit `zip` 9.8 (nur Deflate über `flate2`, reines Rust).
- `cargo test --workspace` 1004 grün, clippy und fmt sauber.

## Weiter

Ausführung wie in der Linux-Sitzung: selbst, ohne Subagenten (Wahl des Nutzers). Vorher
`apps/trail/scripts/build-core.sh` und `xcodegen generate` in `apps/trail`, dann Task 3 nach Plan.
Danach Abschlussprüfung des ganzen Branchs, iPhone-Test durch den Nutzer, PR.

## Für Swift wichtig

- `exportTour(id:path:utcOffsetMin:)` schreibt genau an `path` (eine vorhandene Datei wird ersetzt).
  Der Ordner muss existieren; sonst `export_failed`. Nach einem Fehler liegt weder die Datei noch
  `<path>.part` da.
- **Dauer:** Fotos und Audio werden unverändert kopiert; bei 1,5 GB einige Sekunden. Immer in
  `Task.detached`, nie auf dem Main-Thread. Die Aufnahme eines anderen Tracks läuft währenddessen weiter.
- `utcOffsetMin` = `TimeZone.current.secondsFromGMT(for: Start der Tour) / 60` (nur für die Uhrzeiten in
  `notizen.md`; `tour.json` speichert Millisekunden in UTC und den Versatz).
- Codes: `export_no_space` → „Nicht genug freier Speicher für das Paket.“; `export_failed` und alle
  anderen → „Das Paket konnte nicht gebaut werden.“; `recording_active` für die laufende Aufnahme
  (Knopf dort gar nicht zeigen); `track_id` für eine kaputte ID.
- Eine fehlende Mediendatei bricht nichts ab: das Medium steht ohne Datei in `tour.json`, in
  `notizen.md` „(Datei fehlt)“.
- **Aufräumen:** Die ZIP ist so groß wie alle Fotos zusammen. Nach dem Schließen des Teilen-Blatts
  `tmp/tours/<track-id>/` löschen, beim App-Start `tmp/tours` ganz.
- Dateiname wie beim GPX (`TracksController.fileName`), mit `.zip`.
