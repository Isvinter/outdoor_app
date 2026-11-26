# Übergabe Tour Schritt 1 an den Mac (Tasks 4–5)

Stand: 2026-11-26 · Branch `feature/tour-media` · Plan
`docs/superpowers/plans/2026-11-26-tour-schritt1-abschnitte.md`

## Erledigt (Linux, je Task geprüft)

- Task 1 (`core-track`): 8acc4ff, 67075c7 — Abschnitte mit Sportart und Grund, Rasterkennung,
  Statistik je Abschnitt.
- Task 2 (`core-storage`, `core-gpx`): 2c98a30, 30a30ce — Migration 4, GPX-Erweiterung
  `<zg:section>` nach den Punkten.
- Task 3 (`core-app`, `core-ffi`): f2510b5, 6ddff4c — Sportwechsel, Rast im Status, Abschnitte
  eines Tracks, `docs/ALGORITHMEN.md`.
- `cargo test --workspace` 941 grün, clippy und fmt sauber.

## Weiter

Ausführung: einzeln geprüft (Wahl des Nutzers). Vorher `apps/trail/scripts/build-core.sh` und
`xcodegen generate` in `apps/trail`, dann Task 4 und Task 5 nach Plan. Danach Abschlussprüfung
des ganzen Branchs, iPhone-Test durch den Nutzer, PR.

## Entscheidungen unterwegs (gelten weiter)

- Weg nach einer Rast: Luftlinie vom **ersten** Punkt des Rast-Abschnitts (Erkennungsfix) zum
  ersten Punkt danach.
- Gesamtzeit = erster bis letzter Fix minus Pausen von Hand; GPS-Lücken zählen zur Gesamtzeit.
- Während einer Rast bricht eine GPS-Lücke keinen Abschnitt (gedrosseltes GPS schweigt lange).
- Ändert sich die Rast-Einstellung während einer Rast, endet die Rast (Abschnitt `RestEnd`).

## Für Swift wichtig

- Die Rast-Einstellung speichert der Kern **nicht**: Trail ruft `setRestDetection` bei jedem
  Start und bei jeder Änderung (gleiche Werte noch einmal setzen ändert nichts).
- Ein Rast-Abschnitt behält intern eine Sportart; die Detailansicht zeigt bei `rest == true`
  keine Sportart und kein Sportart-Menü.
- `setSectionActivity` auf den laufenden Track gibt `RecordingActive`; Rast oder falscher Index
  gibt `invalid_section`.

## Zurückgestellte Kleinigkeiten (für die Abschlussprüfung)

- Luftlinie nach Rast zählt zur Länge, nicht zur Bewegungszeit; sie beginnt am rohen ersten
  Punkt, die Abschnittslänge am ersten gefilterten.
- Pause → Sportwechsel → Weiter speichert Grund `Resume` (Sportart stimmt); Wechsel auf dieselbe
  Sportart während Rast speichert `ActivityChange`.
- GPX-Leser prüft den Namensraum von `section` nicht; keine Tests für unbekannte Werte und fremde
  Erweiterungen.
- `set_track_activity` schreibt Abschnitte ohne gemeinsame Transaktion; `import_gpx` setzt die
  gewählte Sportart als Track-Sportart, auch wenn der erste Abschnitt der Datei anders ist.
- Kein Test für Sportwechsel vor dem ersten Fix und für geänderte Rast-Werte während der Rast.
- `RestSettings` wird an der FFI-Grenze nicht geprüft (NaN, negativ, 0).
