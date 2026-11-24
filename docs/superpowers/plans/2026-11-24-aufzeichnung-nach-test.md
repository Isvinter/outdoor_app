# Trail: Aufzeichnung nach dem ersten Test

> Fortsetzung von `2026-11-24-aufzeichnung-live-routing.md`. Grundlage ist der Abschnitt „Nach dem ersten Test auf dem iPhone“ im Entwurf `docs/superpowers/specs/2026-11-24-aufzeichnung-live-routing-design.md`.

**Ziel:** Ein Knopf startet die Aufnahme, die Sportart kommt beim Speichern. Abbrüche und GPS-Lücken verlieren nichts. Aufnahme und Führung sind getrennt. Ein Tipp auf die Karte bietet „Route hierher“ an.

## Global Constraints

- Branch `feature/recording-live-routing`, Commits einzeln je Task, Push und Merge nur nach Rückfrage.
- Desktop (Base) baut auf dem Mac nicht. Kernänderungen, die Base berühren, bleiben klein und werden unter Linux mitgeprüft. Base bekommt nur die neue Aktivität in Typ und Übersetzung.
- Sichtbare Texte wörtlich: „Track speichern?“, „Keine Angabe“, „Speichern“, „Verwerfen“, „Track verwerfen?“, „Eine Aufzeichnung wurde unterbrochen und als Track gespeichert.“, „Kein GPS für {n} min. Die Aufzeichnung läuft weiter.“, „Route hierher“, „Hierher führen“, „Punkt auf der Karte“.
- Werte: Abstand der Punkte 1 s für alle Aufnahmen. Neues Teilstück nach mehr als 120 s ohne Fix. In `docs/ALGORITHMEN.md` festhalten.
- Jeder Commit endet mit `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Teil A: Kern

### Task 12: Aktivität „Keine Angabe“
- `core-track`: `Activity::Other` (`"other"`), Abstand 1 s, Statistik mit den Grundwerten. GPX schreibt `<type>other</type>` bzw. keinen Typ und liest unbekannte Typen nicht als `other`.
- Base: `"other"` in `api.ts` und `de.ts` („Keine Angabe“).
- Tests: Rundweg `as_str`/`from_str`, Abstand, Statistik.

### Task 13: Sportart nach der Aufnahme
- `App::set_track_activity(id, activity)`: setzt die Aktivität eines fertigen Tracks und rechnet die gespeicherte Statistik neu. FFI `set_track_activity(id, activity)`.
- Test: Aufnahme als `other`, danach `hike` → Statistik mit Wanderwerten, `list_tracks` zeigt `hike`.

### Task 14: Neues Teilstück nach GPS-Lücke
- In `Inner::record`: Liegt der neue Fix mehr als 120 s hinter dem letzten gespeicherten, beginnt ein neues Teilstück (wie nach „Weiter“). Die Lücke zählt weder als Strecke noch als Bewegungszeit.
- Test: Fixes, 5 min Pause, Fixes → zwei Teilstücke, Strecke ohne den Sprung.

### Task 15: Führung unabhängig von der Aufnahme
- `guide_recording` entfällt. `plan_changed` setzt nur eine laufende Führung neu an. `OFF_TRACK_KEY` und das Speichern in `stop_guidance` entfallen.
- Tests: `recording_starts_guidance_with_route` wird zu `recording_does_not_start_guidance`, `switched_off_survives_restart` entfällt, neuer Test: Führung läuft nach Ende der Aufnahme weiter und Aufnahme nach Ende der Führung.
- `docs/ALGORITHMEN.md` Abschnitt 7 anpassen.

## Teil B: Trail

### Task 16: Aufnahme mit einem Knopf, Sportart beim Speichern
- Knopf startet sofort (`start(activity: "other")`), `ActivityPicker` und `lastActivity` entfallen.
- „Beenden“ ohne Rückfrage, dann Panel „Track speichern?“ mit Sportart-Auswahl („Keine Angabe“ vorausgewählt), „Speichern“ (setzt Sportart und Namen) und „Verwerfen“ (fragt „Track verwerfen?“, löscht).
- Name `yyyy-MM-dd HH:mm` plus Sportart, beim Beenden schon gesetzt.
- Tests: Unit für Name und Speichern/Verwerfen; UI `test_record_stop_save` und `test_record_stop_discard`.

### Task 17: Abbruch und GPS-Lücke
- Beim Start: unterbrochene Aufnahme automatisch abschließen, benennen, Hinweis zeigen. `InterruptedRecordingPrompt` entfällt.
- Während der Aufnahme: kommt ein Fix nach mehr als 120 s ohne Fix, Hinweis „Kein GPS für {n} min. Die Aufzeichnung läuft weiter.“
- Tests: Unit für beides, UI `test_interrupted_recording_is_saved`.

### Task 18: Tipp auf die Karte bietet „Route hierher“
- Mit bekannter Position, „+ Punkt“ aus und ohne Suchtreffer am Tippunkt: Karte „Punkt auf der Karte“ mit „Route hierher“ und „Hierher führen“ (plant von der Position und startet danach die Führung). Ohne Position bleibt alles wie heute.
- Tests: Unit im Ablauf von `RouteHereFlow`, UI mit `-replayGPX`.

### Task 19: Live-Routing auf dem iPhone
- Den Fehler aus dem Test nachvollziehen (Angaben des Nutzers ausstehend), beheben, Test dazu.

### Task 20: Test auf dem iPhone
- Release-Build aufs iPhone, Handtest wie Task 11. Ergebnis in `docs/MESSPROTOKOLL.md`. Danach anhalten.
