# Übergabe Tour Schritt 2 an den Mac (Tasks 4–5)

Stand: 2026-11-27 · Branch `feature/tour-media-2` · Plan
`docs/superpowers/plans/2026-11-27-tour-schritt2-fotos-notizen.md`

## Erledigt (Linux)

- Task 1 (`core-storage`): 16767a6 — Migration 5, Tabelle `track_media`, Medien weich löschen mit dem Track.
- Task 2 (`core-app`): b497d7b — Zuordnung zum Abschnitt, Pfadprüfung, Dateien löschen, Track ohne Punkte
  mit Medien bleibt, `TrackDetail.media`, `docs/ALGORITHMEN.md` 6b.
- Task 3 (`core-ffi`): f65bf62 — `MediaItem`, `MediaInput`, `mediaDir`, `addRecordingMedia`, `addTrackMedia`,
  `setMediaText`, `setMediaTranscript`, `deleteMedia`, `TrackView.media`.
- `cargo test --workspace` 978 grün, clippy und fmt sauber.

## Weiter

Ausführung wie in der Linux-Sitzung: selbst, ohne Subagenten (Wahl des Nutzers). Vorher
`apps/trail/scripts/build-core.sh` und `xcodegen generate` in `apps/trail`, dann Task 4 und Task 5 nach Plan.
Danach Abschlussprüfung des ganzen Branchs, iPhone-Test durch den Nutzer, PR.

## Für Swift wichtig

- **Datei zuerst, dann melden:** Datei atomar nach `mediaDir()/<uuid>.<ext>` schreiben, dann
  `addRecordingMedia`/`addTrackMedia` mit `file: "media/<uuid>.<ext>"` (Pfad relativ zum Datenordner, den
  Trail an `CoreApp.open` gibt). Andere Pfade, Unterordner oder eine fehlende Datei → Code `media_file`.
- **Erlaubte Kombinationen:** `photo` + `camera`/`library` + Datei; `voice` + `spoken` + Datei; `text` + `typed`
  + nicht leerer Text, ohne Datei. Sonst `invalid_media`. Unbekannte Werte: `unknown_media_kind`,
  `unknown_media_source`; keine UUID: `media_id`.
- **Zeit und Ort unterwegs** setzt der Kern beim Aufruf (jetzt, letzter Fix). Also direkt nach dem Auslösen
  bzw. nach dem Ende der Sprachnotiz melden. Ins EXIF schreibt Trail dieselben Werte.
- **Abschnitt unterwegs:** der letzte mit einem gespeicherten Punkt (Rast → Rast-Abschnitt, Pause → voriger,
  direkt nach Sportwechsel bis zum nächsten Fix → voriger, vor dem ersten Punkt → `section == nil`).
- **Nachträglich** (`addTrackMedia`): `section: nil` = ganze Tour; ohne `timeMs` setzt der Kern den Beginn des
  Abschnitts bzw. des Tracks. Geht auch für die laufende Aufnahme (Abschnittskarte). Rast-Abschnitte erlaubt.
- `setMediaText` behält `transcribed`; leerer Text entfernt ihn, bei einer Textnotiz → `invalid_media` (dann
  löschen anbieten). `setMediaTranscript` nur für `voice`.
- `deleteMedia` und `deleteTrack` löschen Trails Kopie der Datei mit; nie das Bild in der Mediathek.
- Eine Aufnahme ohne Punkt, aber mit Medien, liefert bei `stopRecording` einen Track (bisher `nil`). Für
  „Verwerfen löscht auch {n} Fotos und Notizen.“ die Zahl aus `trackDetail(id:).media.count` der Aufnahme.
- `TrackView.media` ist nach Zeit sortiert, ohne Zeit zuletzt; `MediaItem.section` passt zu
  `TrackSection.index`.

## Zurückgestellte Kleinigkeiten (für die Abschlussprüfung)

- `setMediaTranscript` mit leerem Text speichert `""` mit `transcribed = true` (statt keinen Text).
- Unterwegs ist der Ort der letzte Fix, auch wenn er alt ist (lange ohne GPS); kein Alterslimit.
- Bleibt eine Datei liegen, weil das Melden scheitert oder die App dazwischen endet, räumt niemand sie auf.
