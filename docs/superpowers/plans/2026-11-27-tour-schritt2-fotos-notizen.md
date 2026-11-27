# Tour Schritt 2: Fotos und Notizen Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Während einer Aufnahme in Trail Fotos, Sprachnotizen und Textnotizen festhalten, die automatisch Zeit, Ort und Abschnitt bekommen; im Modus „Abschnitte bearbeiten“ ansehen, ändern, löschen, Bilder vom Gerät einfügen und Sprache auf dem Gerät in Text umwandeln.

**Architecture:** `core-storage` bekommt die Tabelle `track_media` (Migration 5) mit Verweis auf die Segment-UUID. `core-app` ordnet ein Medium dem Abschnitt zu (laufend oder gewählt), prüft die Datei im Ordner `media/` neben der Datenbank, löscht Dateien mit dem Medium bzw. dem Track und liefert die Medien in `TrackDetail`. `core-ffi` reicht das durch. Trail nimmt Fotos (mit EXIF), Audio und Text auf, schreibt die Datei in `media/` und meldet sie dem Kern; die Detailansicht zeigt und bearbeitet sie.

**Tech Stack:** Rust, rusqlite, UniFFI, SwiftUI, AVFoundation, PhotosUI (`PHPickerViewController`), Photos (nur Hinzufügen), Speech, ImageIO, XCUITest.

**Spec:** `docs/superpowers/specs/2026-11-26-tour-fotos-notizen-export-design.md` („Schritt 2“, Nachtrag 2026-11-27, zugehörige Teile von „Entwurf“); Nachtrag am Ende von `docs/superpowers/plans/2026-11-26-tour-schritt1-abschnitte.md`.

## Global Constraints

- Branch `feature/tour-media-2` (von `main` nach PR #14). Am Ende PR, Merge lokal mit `--no-ff`. Push nur nach Rückfrage.
- **Anhalten und dem Nutzer zeigen:** nach Task 3 (Kern fertig, Linux; danach Push für den Mac nach Rückfrage) und nach Task 5 (iPhone-Test).
- Medien hängen an der **Aufzeichnung**, nie an der Route. Zuordnung unterwegs: laufender Abschnitt; während Rast der Rast-Abschnitt; während Pause der letzte Abschnitt; vor dem ersten Punkt „ganze Tour“ (`section = None`).
- **Entscheidung von Claude (neu):** „laufender Abschnitt“ = der letzte gespeicherte Abschnitt mit Punkten. Nach einem Sportwechsel gehört ein Foto vor dem nächsten Fix noch zum vorigen Abschnitt; nach `continue_interrupted` vor dem ersten neuen Fix zum letzten gespeicherten. So entstehen keine leeren Abschnitte.
- Eingefügte Bilder und nachträgliche Notizen: Zeit und Ort aus den Metadaten, falls vorhanden, sonst ohne Ort, Zeit = Beginn des Abschnitts (bei „ganze Tour“: Beginn des Tracks; ohne Zeiten im Track: keine Zeit).
- Arten und Herkunft (Werte wörtlich): `kind` = `photo` | `voice` | `text`; `source` = `camera` | `library` | `typed` | `spoken`. Erlaubt: `photo` mit `camera`/`library` und Datei; `voice` mit `spoken` und Datei; `text` mit `typed` und nicht leerem Text.
- Dateien liegen in `media/` im Datenordner der App (neben `outdoor.sqlite`); gespeichert und übergeben wird der Pfad relativ zum Datenordner, z. B. `media/0193….jpg`. Kein Verkleinern. Bilder aus der Mediathek behalten ihr Format (jpg/heic/png); ob das Paket in Schritt 3 umwandelt, entscheidet dessen Plan.
- Gespeichert wird sofort: erst die Datei (atomar), dann sofort der Eintrag im Kern. Löschen in Trail löscht nur Trails Kopie, nie das Bild in der Mediathek.
- Ändern, Löschen, Einfügen nur im Modus „Abschnitte bearbeiten“; die Detailansicht zeigt Medien auch sonst, ohne Knöpfe.
- Keine Netzwerkaufrufe; Umwandlung nur mit `requiresOnDeviceRecognition = true`. Kamera und Mikrofon laufen nur, wenn der Nutzer sie öffnet. Logik im Kern.
- Texte wörtlich (Deutsch): Knöpfe „Foto“, „Notiz“, „Text“; Textfeld-Titel „Notiz“, Knöpfe „Sichern“ / „Abbrechen“; „Ganze Tour“; „Bild einfügen“; „In Text umwandeln“, „Alle umwandeln“; Rückfrage „Foto löschen?“ / „Notiz löschen?“ mit „Löschen“; Einstellung „Fotos auch in der Mediathek sichern“ (Standard an); ohne Umwandlung auf dem Gerät: „Umwandeln geht für Deutsch auf diesem iPhone nur mit Netz. Das Audio bleibt.“; abgelehnte Berechtigung: „{Kamera|Mikrofon|Spracherkennung} in den Einstellungen erlauben“ mit Knopf „Einstellungen“; Verwerfen mit Medien: „Verwerfen löscht auch {n} Fotos und Notizen.“
- Swift lässt sich unter Linux nicht bauen. Tasks 4–5 auf dem Mac, vorher `apps/trail/scripts/build-core.sh` und `xcodegen generate` in `apps/trail`.
- Neue Ansichten funktionieren im OLED-Dunkelmodus; Knöpfe mit Handschuhen treffbar (mindestens 56 pt hoch). UI-Tests mit echten Eingaben; Kamera, Mikrofon, Mediathek und Spracherkennung im Simulator über Test-Ersatz. Echte Karten in `data/` nicht anfassen. Jeder Commit endet mit `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. Abschnittszuordnung, wenn Recorder-Index und gespeicherte Segmentfolge auseinanderlaufen (Pause → Weiter ohne Fix, Sportwechsel ohne Fix, Rast, fortgesetzte Aufnahme): das Medium landet im richtigen Abschnitt und erscheint in `TrackDetail` beim richtigen `index`. → Tests in Task 2.
2. Aufnahme ohne einen einzigen Fix (drinnen, kein GPS), aber mit Fotos oder Notizen, wird beendet: der Track bleibt mit seinen Medien erhalten statt gelöscht. → Test in Task 2.
3. Track löschen oder Aufnahme verwerfen: Medien-Einträge weich gelöscht, Dateien weg; eine schon fehlende Datei lässt das Löschen nicht scheitern. → Tests in Task 2.
4. Dateipfad außerhalb von `media/` (`../x`, absoluter Pfad, Unterordner) oder fehlende Datei: Fehler `media_file`, nichts gespeichert, nichts gelöscht. → Test in Task 2.
5. Alte Datenbank (Schema 4) öffnet; Tracks ohne Medien liefern eine leere Liste; ein Medium an einem Abschnitt bleibt beim Ändern der Sportart dieses Abschnitts erhalten. → Tests in Task 1 und 2.

---

### Task 1: `core-storage` — Tabelle `track_media`

**Files:**
- Modify: `crates/core-storage/src/migrations.rs` (Migration 5), `crates/core-storage/src/storage.rs` (`delete_track`, Segment-IDs), `crates/core-storage/src/lib.rs` (Exporte)
- Create: `crates/core-storage/src/media.rs` (`impl Storage` für Medien, Typen)
- Test: `crates/core-storage/tests/media.rs` (neu)

**Interfaces:**
- Produces:
  - `pub enum MediaKind { Photo, Voice, Text }`, `pub enum MediaSource { Camera, Library, Typed, Spoken }` — serde snake_case, `as_str()`/`FromStr` wie `Activity`.
  - `pub struct NewMedia<'a> { pub track_id: Uuid, pub segment_id: Option<Uuid>, pub kind: MediaKind, pub source: MediaSource, pub time_ms: Option<i64>, pub lat: Option<f64>, pub lon: Option<f64>, pub file: Option<&'a str>, pub text: Option<&'a str> }`
  - `pub struct MediaRecord { pub id: Uuid, pub track_id: Uuid, pub segment_id: Option<Uuid>, pub kind, pub source, pub time_ms: Option<i64>, pub lat: Option<f64>, pub lon: Option<f64>, pub file: Option<String>, pub text: Option<String>, pub transcribed: bool, pub created_at_ms: i64, pub updated_at_ms: i64, pub rev: i64 }` (Serialize, Deserialize).
  - `Storage::add_media(&mut self, media: &NewMedia, now_ms: i64) -> Result<Uuid, StorageError>` (UUID v7; `NotFound`, wenn Track gelöscht/fehlt oder das Segment nicht zu diesem Track gehört).
  - `Storage::media(&self, id: Uuid) -> Result<MediaRecord, StorageError>`; `Storage::list_media(&self, track: Uuid) -> Result<Vec<MediaRecord>, StorageError>` (ohne gelöschte; Reihenfolge `time_ms` aufsteigend, ohne Zeit zuletzt, dann `created_at`, `id`).
  - `Storage::set_media_text(&mut self, id: Uuid, text: Option<&str>, transcribed: bool, now_ms: i64) -> Result<(), StorageError>` (setzt `updated_at`, `rev + 1`).
  - `Storage::delete_media(&mut self, id: Uuid, now_ms: i64) -> Result<(), StorageError>` (weich: `deleted_at`, `updated_at`, `rev + 1`).
  - `Storage::segment_ids(&self, track: Uuid) -> Result<Vec<Uuid>, StorageError>` (nach `seq`; Position = Abschnittsindex wie in `load_track`).
  - `delete_track` löscht in derselben Transaktion alle Medien des Tracks weich.

- [x] **Step 1: Tests schreiben** (`tests/media.rs`)
  - `migration_5_opens_schema_4` (Datenbank mit Schema 4 wie die vorhandenen Migrationstests anlegen, öffnen → `schema_version() == 5`, `list_media(track)` leer).
  - `media_round_trip` (Foto mit Segment, Zeit, Ort, Datei; Text ohne Segment → `list_media` gibt beide mit allen Feldern, `transcribed == false`, `rev == 1`).
  - `list_orders_by_time_untimed_last`.
  - `segment_of_other_track_is_rejected` (→ `NotFound`, nichts gespeichert).
  - `set_media_text_bumps_rev` (Text und `transcribed = true` gesetzt, `rev == 2`, `updated_at` neu).
  - `deleted_media_is_not_listed` und `deleting_track_deletes_its_media` (Eintrag hat danach `deleted_at`, `media(id)` → `NotFound`).
  - `segment_ids_follow_seq` (drei `begin_segment` → IDs in dieser Reihenfolge).
  - `media_survives_section_activity_change` (`set_segment_activity` auf das Segment des Mediums → Medium unverändert gelistet).
- [x] **Step 2: Laufen lassen, schlagen fehl** — Run: `cargo test -p core-storage --test media` · Expected: Kompilierfehler für die neuen Namen.
- [x] **Step 3: Implementieren.** Migration 5 (nur anhängen):
  ```sql
  CREATE TABLE track_media (
      id          BLOB PRIMARY KEY NOT NULL,
      track_id    BLOB NOT NULL REFERENCES tracks (id),
      segment_id  BLOB REFERENCES track_segments (id),   -- NULL: ganze Tour
      kind        TEXT NOT NULL,
      source      TEXT NOT NULL,
      time_ms     INTEGER,
      lat         REAL,
      lon         REAL,
      file        TEXT,                                  -- relativ zum Datenordner
      text        TEXT,
      transcribed INTEGER NOT NULL DEFAULT 0,
      created_at  INTEGER NOT NULL,
      updated_at  INTEGER NOT NULL,
      deleted_at  INTEGER,
      rev         INTEGER NOT NULL DEFAULT 1,
      synced_rev  INTEGER NOT NULL DEFAULT 0
  ) STRICT;
  CREATE INDEX track_media_track ON track_media (track_id);
  ```
  Unbekannte `kind`/`source` beim Lesen → `StorageError::Corrupt`.
- [x] **Step 4: Tests laufen lassen** — Run: `cargo test -p core-storage` · Expected: alle PASS.
- [x] **Step 5: Commit** — `git commit -m "Store photos and notes of a track"`.

---

### Task 2: `core-app` — Medien anlegen, zuordnen, ändern, löschen

**Files:**
- Create: `crates/core-app/src/media.rs` (`impl App` für Medien, `MediaDetail`, `MediaInput`, Pfadprüfung)
- Modify: `crates/core-app/src/app.rs` (`Shared.data_dir`, `media/` beim Öffnen anlegen, `ActiveRecording` merkt den letzten gespeicherten Abschnitt, `finish` behält Tracks mit Medien, `delete_track` löscht Dateien, `TrackDetail.media`), `crates/core-app/src/error.rs`, `crates/core-app/src/lib.rs`, `docs/ALGORITHMEN.md` (in 6b: Zuordnung von Fotos und Notizen zum Abschnitt)
- Test: `crates/core-app/tests/media.rs` (neu, mit den Fix-Helfern aus `tests/common`)

**Interfaces:**
- Consumes: Task 1.
- Produces:
  - `pub struct MediaInput { pub kind: MediaKind, pub source: MediaSource, pub file: Option<String>, pub text: Option<String>, pub time_ms: Option<i64>, pub lat: Option<f64>, pub lon: Option<f64> }`
  - `pub struct MediaDetail { pub record: MediaRecord, pub section: Option<usize> }` (`section` = Index wie `SectionDetail::index`, `None` = ganze Tour).
  - `TrackDetail.media: Vec<MediaDetail>` (gleiche Reihenfolge wie `list_media`).
  - `App::media_dir(&self) -> PathBuf` (= `<Datenordner>/media`, beim `open` angelegt).
  - `App::add_recording_media(&self, kind: MediaKind, source: MediaSource, file: Option<&str>, text: Option<&str>) -> Result<MediaDetail, AppError>` — Zeit = jetzt, Ort = letzter Fix (`last_fix`, falls vorhanden), Abschnitt nach der Regel der Global Constraints; ohne Aufnahme `NoRecording`.
  - `App::add_track_media(&self, track: Uuid, section: Option<usize>, input: MediaInput) -> Result<MediaDetail, AppError>` — geht für fertige Tracks und die laufende Aufnahme; Rast-Abschnitte erlaubt; Index außerhalb → `InvalidSection`; fehlende Zeit nach der Regel der Global Constraints.
  - `App::set_media_text(&self, id: Uuid, text: &str) -> Result<MediaDetail, AppError>` (behält `transcribed`; leer → `None`, bei `text`-Notiz leer → `InvalidMedia`).
  - `App::set_media_transcript(&self, id: Uuid, text: &str) -> Result<MediaDetail, AppError>` (nur `voice`, sonst `InvalidMedia`; setzt `transcribed = true`).
  - `App::delete_media(&self, id: Uuid) -> Result<(), AppError>` (weich löschen, dann Datei entfernen; fehlende Datei ist kein Fehler).
  - `AppError::InvalidMedia` (Code `invalid_media`, unzulässige Kombination aus Art, Herkunft, Datei, Text), `AppError::MediaFile(String)` (Code `media_file`).

- [x] **Step 1: Tests schreiben** (`tests/media.rs`; Dateien mit `std::fs::write` in `app.media_dir()` anlegen)
  - `photo_during_recording_goes_to_running_section` (Aufnahme, 3 Fixe, Foto → `section == Some(0)`, Ort = letzter Fix, Zeit ≈ jetzt; erscheint in `track_detail(..).media`).
  - `before_first_fix_is_whole_tour` (→ `section == None`, kein Ort).
  - `during_pause_goes_to_last_section` (Fixe, Pause, Foto → `Some(0)`; Weiter, Fixe → das spätere Foto `Some(1)`).
  - `after_switch_before_fix_goes_to_previous_section` (Sportwechsel, sofort Notiz → `Some(0)`; nach dem nächsten Fix → `Some(1)`).
  - `during_rest_goes_to_rest_section` (`set_rest_detection(Some(DEFAULT))`, 6 min Stehen, Notiz → Abschnitt mit `reason == Rest`).
  - `continued_recording_uses_last_stored_section` (abbrechen wie in den Absturz-Tests von `tests/app.rs`, `continue_interrupted`, Notiz vor neuem Fix → Index des letzten gespeicherten Abschnitts; die Medien von vor dem Abbruch sind noch da).
  - `index_survives_empty_recorder_segments` (Pause → Weiter → Pause → Weiter ohne Fix dazwischen, dann Fixe und Foto → `section` in `TrackDetail` zeigt auf den Abschnitt, der die Punkte des Fotos enthält).
  - `library_photo_without_time_takes_section_start` und `library_photo_keeps_given_time_and_place`; `whole_tour_without_time_takes_track_start`.
  - `add_track_media_rejects_bad_index` (→ `invalid_section`) und `rejects_bad_combinations` (`text` ohne Text, `photo` ohne Datei, `voice` mit `camera` → `invalid_media`).
  - `rejects_files_outside_media_dir` (`../outdoor.sqlite`, `/etc/hosts`, `media/a/b.jpg`, `media/fehlt.jpg` → `media_file`; `list_media` leer).
  - `stop_keeps_track_with_media_but_no_points` (Aufnahme ohne Fix, Textnotiz, `stop_recording` → `Some(record)`, Notiz in `track_detail`); ohne Medien und Punkte weiter `None`.
  - `delete_media_removes_file` und `delete_media_with_missing_file_succeeds`; `delete_track_removes_media_files`.
  - `edit_text_keeps_transcribed` (`set_media_transcript` → `transcribed`; danach `set_media_text` → Text neu, `transcribed` bleibt `true`); `transcript_only_for_voice`.
- [x] **Step 2: Laufen lassen, schlagen fehl** — Run: `cargo test -p core-app --test media` · Expected: Kompilierfehler für die neuen Namen.
- [x] **Step 3: Implementieren.** `ActiveRecording` bekommt `last_stored_segment: Option<Uuid>`, gesetzt beim Anhängen eines Punkts; `continue_interrupted` setzt es auf die letzte ID aus `segment_ids`, deren Segment Punkte hat. Index für `MediaDetail` über die Position von `segment_id` in `segment_ids`. Pfadprüfung: genau zwei Komponenten `media/<name>`, kein `..`, Datei existiert unter dem Datenordner. `finish` löscht einen punktlosen Track nur, wenn `list_media` leer ist. `delete_track`: Dateien nach dem erfolgreichen Datenbank-Löschen entfernen, Fehler beim Entfernen ignorieren.
- [x] **Step 4: Tests laufen lassen** — Run: `cargo test -p core-app` · Expected: alle PASS.
- [x] **Step 5: Commit** — `git commit -m "Attach photos and notes to the sections of a recording"`.

---

### Task 3: `core-ffi` — Medien für Trail

**Files:**
- Modify: `crates/core-ffi/src/track.rs` (`TrackView.media`), `crates/core-ffi/src/lib.rs` (Modul)
- Create: `crates/core-ffi/src/media.rs`
- Test: `crates/core-ffi/tests/ffi.rs`

**Interfaces:**
- Consumes: Task 2.
- Produces (Swift-Namen):
  - `MediaItem { id: String, section: UInt32?, kind: String, source: String, timeMs: Int64?, lat: Double?, lon: Double?, file: String?, text: String?, transcribed: Bool, updatedMs: Int64 }`; `TrackView.media: [MediaItem]`.
  - `MediaInput { kind: String, source: String, file: String?, text: String?, timeMs: Int64?, lat: Double?, lon: Double? }`.
  - `mediaDir() -> String` (absolut).
  - `addRecordingMedia(kind: String, source: String, file: String?, text: String?) throws -> MediaItem`.
  - `addTrackMedia(trackId: String, section: UInt32?, input: MediaInput) throws -> MediaItem`.
  - `setMediaText(id: String, text: String) throws -> MediaItem`, `setMediaTranscript(id: String, text: String) throws -> MediaItem`, `deleteMedia(id: String) throws`.
  - Fehlercodes: `invalid_media`, `media_file`, `invalid_section`, `no_recording`, `media_id` (keine UUID), `unknown_media_kind`, `unknown_media_source`.

- [x] **Step 1: Tests schreiben** — `media_round_trip_through_ffi` (Aufnahme, Fix, Datei in `media_dir()`, `add_recording_media("photo", "camera", Some("media/x.jpg"), None)` → `section == Some(0)`; `track_detail(..).media` enthält es); `unknown_kind_has_code` (`"video"` → `unknown_media_kind`); `bad_media_id_has_code`.
- [x] **Step 2: Laufen lassen, schlagen fehl** — Run: `cargo test -p core-ffi` · Expected: FAIL.
- [x] **Step 3: Implementieren** wie die vorhandenen Umwandlungen in `track.rs` (keine Logik in der FFI).
- [x] **Step 4: Alles prüfen** — Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check` · Expected: alle grün.
- [x] **Step 5: Commit** — `git commit -m "Offer photos and notes to the apps"`.
- [x] **Step 6: Übergabe schreiben** — `docs/superpowers/plans/2026-11-27-uebergabe-tour-schritt2-mac.md` (erledigte Commits, Testzahl, Swift-Hinweise wie bei Schritt 1), committen. **Anhalten**, dem Nutzer zeigen; Push nach Rückfrage für die Mac-Sitzung.

---

### Task 4: Trail — Aufnehmen unterwegs (Foto, Sprachnotiz, Textnotiz)

**Files:**
- Create: `apps/trail/ZeroGridTrail/Media/MediaController.swift` (Datei schreiben, Kern rufen, Mediathek), `Media/CameraCapture.swift`, `Media/VoiceRecorder.swift`, `Media/NoteEditor.swift`, `Media/Permissions.swift`
- Modify: `apps/trail/ZeroGridTrail/Recording/RecordingBar.swift` (Knopfreihe), `Recording/RecordingController.swift` (Verwerfen mit Medien), `Map/MapToolbar.swift` (Kamera ohne Aufnahme, Einstellung), `Map/MapScreen.swift`, `ZeroGridTrailApp.swift` (Verdrahtung, Test-Ersatz per Startargument), `apps/trail/project.yml` (`NSCameraUsageDescription` auch für Fotos, `NSMicrophoneUsageDescription`, `NSPhotoLibraryAddUsageDescription`)
- Test: `apps/trail/ZeroGridTrailTests/MediaControllerTests.swift` (neu), `apps/trail/ZeroGridTrailUITests/MediaUITests.swift` (neu)

**Interfaces:**
- Consumes: FFI aus Task 3.
- Produces: `MediaController` (`ObservableObject`) mit `capturePhoto(_ image: Data)`, `finishVoice(at url: URL)`, `addText(_ text: String)`, `saveToLibrary: Bool` (`@AppStorage("savePhotosToLibrary")`, Standard `true`), `func newFileURL(ext: String) -> (url: URL, relative: String)`; Protokolle `CameraSource`, `AudioSource`, `LibrarySaver` mit Test-Ersatz (Startargument `-media-fakes`: festes JPEG, kurze m4a aus den Test-Ressourcen).

- [x] **Step 1: Tests schreiben**
  - Unit: `photoIsWrittenThenRegistered` (Ersatz-Kern: Datei liegt in `media/`, bevor `addRecordingMedia` gerufen wird; Pfad `media/<uuid>.jpg`); `photoCarriesExifTimeAndPlace` (geschriebenes JPEG hat `DateTimeOriginal` und GPS des letzten Fixes, gelesen mit `CGImageSource`); `libraryCopyFollowsSetting` (an → `LibrarySaver` gerufen, aus → nicht); `photoWithoutRecordingGoesOnlyToLibrary` (kein Kernaufruf, keine Datei in `media/`); `failedRegistrationKeepsFile` (Kern wirft → Datei bleibt, Fehler sichtbar); `emptyTextIsNotSaved`.
  - UI (`MediaUITests`, mit GPX-Ersatz und `-media-fakes`): `testBarShowsMediaButtons` (während Aufnahme „Foto“, „Notiz“, „Text“ sichtbar, je ≥ 56 pt hoch); `testPhotoDuringRecording` (Tipp „Foto“ → Auslösen → Status `media=1`); `testVoiceHoldAndTap` (Halten 2 s → eine Notiz; Tipp, Tipp → zweite Notiz); `testTextNote` („Text“ → „Notiz“-Feld, tippen, „Sichern“ → `media=+1`; „Abbrechen“ speichert nichts); `testCameraWithoutRecording` (Kamera-Knopf in der Kartenleiste, Foto → kein Medium im Kern); `testDiscardWarnsAboutMedia` (Aufnahme mit 2 Medien, Beenden → Verwerfen → „Verwerfen löscht auch 2 Fotos und Notizen.“).
- [x] **Step 2: Laufen lassen, schlagen fehl** — Run: `apps/trail/scripts/test.sh -only-testing:ZeroGridTrailTests/MediaControllerTests -only-testing:ZeroGridTrailUITests/MediaUITests` · Expected: neue FAIL.
- [x] **Step 3: Implementieren.** Knopfreihe der Leiste wie in der Spec (über „Pause“/„Beenden“). Kamera: API dem Bauenden überlassen; JPEG mit EXIF (`DateTimeOriginal`, GPS aus `LocationService.last`) über `CGImageDestination`, atomar nach `media/`, dann `addRecordingMedia("photo", "camera", …)`; Mediathek über `PHPhotoLibrary` (nur Hinzufügen) mit Ort. Sprachnotiz: `AVAudioRecorder` AAC/m4a direkt nach `media/`, Halten = sprechen (loslassen beendet), kurzer Tipp (< 0,5 s) startet, nächster Tipp beendet; dann `addRecordingMedia("voice", "spoken", …)`. Text: Blatt „Notiz“ mit Textfeld, `addRecordingMedia("text", "typed", nil, text)`. Berechtigungen erst beim ersten Gebrauch; abgelehnt → Hinweis mit Knopf „Einstellungen“. Status im Testmodus um `media=<n>` ergänzen.
- [x] **Step 4: Tests laufen lassen** — gleicher Befehl, dann `apps/trail/scripts/test.sh` · Expected: alle PASS; Hell/Dunkel ansehen.
- [x] **Step 5: Commit** — `git commit -m "Take photos and notes while recording"`.

---

### Task 5: Trail — Medien in der Detailansicht, Bearbeiten, Bilder einfügen, Umwandeln

**Files:**
- Create: `apps/trail/ZeroGridTrail/Media/MediaList.swift` (Medien eines Abschnitts bzw. der ganzen Tour), `Media/PhotoViewer.swift`, `Media/LibraryImport.swift` (`PHPickerViewController`, Metadaten lesen), `Media/Transcriber.swift` (`SFSpeechRecognizer`)
- Modify: `apps/trail/ZeroGridTrail/Recording/TrackDetailView.swift`, `Recording/TracksList.swift` (`TracksController`), `Map/MapScreen.swift` (Abschnittskarte der laufenden Aufnahme), `apps/trail/project.yml` (`NSSpeechRecognitionUsageDescription`)
- Test: `apps/trail/ZeroGridTrailTests/LibraryImportTests.swift`, `TranscriberTests.swift` (neu), `apps/trail/ZeroGridTrailUITests/MediaEditUITests.swift` (neu)

**Interfaces:**
- Consumes: `TrackView.media`, `addTrackMedia`, `setMediaText`, `setMediaTranscript`, `deleteMedia` (Task 3); `TracksController.selectedSection`, `MapMode` (Schritt 1, Task 6); `MediaController.newFileURL` (Task 4).
- Produces: `TracksController.media(for section: Int?) -> [MediaItem]`, `addNote(section:text:)`, `insertImages(section:results:)`, `editText(id:text:)`, `delete(id:)`, `transcribe(id:)`, `transcribeAll()`; `LibraryImport.metadata(of: URL) -> (timeMs: Int64?, lat: Double?, lon: Double?)`.

- [x] **Step 1: Tests schreiben**
  - Unit: `metadataFromExif` (Test-JPEG mit Datum und GPS → Werte; PNG-Screenshot ohne → alle `nil`); `importCopiesWithOriginalExtension` (HEIC bleibt `.heic`, Datei in `media/`, dann `addTrackMedia` mit gewähltem Abschnitt); `transcriberRefusesWithoutOnDevice` (Ersatz meldet `supportsOnDeviceRecognition == false` → Meldung wörtlich, kein Kernaufruf, Audio unverändert); `transcribeAllSkipsDone` (nur Notizen mit `transcribed == false` und ohne Text).
  - UI (`MediaEditUITests`, Track-Fixture mit drei Abschnitten und Medien): `testDetailShowsMediaWithoutEditButtons` (Modus „Wegpunkte setzen“: Fotos und Notizen unter ihren Abschnitten und unter „Ganze Tour“ sichtbar, kein „Löschen“/„Bild einfügen“; Tipp auf ein Vorschaubild öffnet `PhotoViewer` in voller Größe); `testEditModeAddsNoteToSelectedSection` (Modus „Abschnitte bearbeiten“, Abschnitt 2 wählen, Notiz anlegen → erscheint unter Abschnitt 2); `testInsertImageFromLibrary` (Ersatz-Picker liefert ein Bild → unter dem gewählten Abschnitt); `testDeletePhotoAsks` („Foto löschen?“ → „Löschen“ → weg); `testEditTranscribedText`; `testTranscribeVoiceNote` (Ersatz-Erkennung → Text erscheint, Audio-Knopf bleibt); `testRecordingSectionCardEditsMedia` (während Aufnahme, Abschnittskarte aus Task 6 → Notiz anlegen).
- [x] **Step 2: Laufen lassen, schlagen fehl** — Run: `apps/trail/scripts/test.sh -only-testing:ZeroGridTrailTests/LibraryImportTests -only-testing:ZeroGridTrailTests/TranscriberTests -only-testing:ZeroGridTrailUITests/MediaEditUITests` · Expected: neue FAIL.
- [x] **Step 3: Implementieren.** Unter jeder Abschnittszeile ihre Medien (Vorschaubilder, Notizsymbol mit Textanfang, Abspielen der Sprachnotiz), oben „Ganze Tour“. Im Modus „Abschnitte bearbeiten“ für den gewählten Abschnitt oder „Ganze Tour“: „Notiz“, „Bild einfügen“ (mehrere Bilder erlaubt), Text bearbeiten, Löschen mit Rückfrage, „In Text umwandeln“ je Sprachnotiz und „Alle umwandeln“ für die Tour. Picker: `PHPickerConfiguration()` ohne Mediathek-Berechtigung, `selectionLimit = 0`; Datei über `loadFileRepresentation` kopieren, Metadaten mit `CGImageSource` lesen. Umwandlung: `SFSpeechRecognizer(locale: de-DE)`, `requiresOnDeviceRecognition = true`, Ergebnis über `setMediaTranscript`.
- [x] **Step 4: Alle Trail-Tests** — Run: `apps/trail/scripts/test.sh` · Expected: alle PASS; Hell/Dunkel ansehen.
- [x] **Step 5: Commit** — `git commit -m "Show, edit and add photos and notes of a track"`.
- [ ] **Step 6: Anhalten**, dem Nutzer zum Test auf dem iPhone geben (Fotos mit Handschuhen, Sprachnotiz im Wind, Umwandeln offline im Flugmodus, Bild aus der Mediathek einfügen); danach Abschlussprüfung des Branchs und PR.
