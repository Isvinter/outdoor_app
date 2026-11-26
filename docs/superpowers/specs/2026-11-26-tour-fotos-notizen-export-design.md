# Entwurf: Tour mit Abschnitten, Fotos, Notizen und Export (Trail)

Stand: 2026-11-26 · Status: zur Freigabe · Größe: Projekt, in drei Schritten

## Anlass und Ziel

Der Nutzer will aus seinen Touren später mit einem multimodalen LLM auf seinem Linux-Rechner
(Base; 7950X3D + RTX 5090) Tourberichte und Fotobücher erzeugen lassen. Dafür muss unterwegs
der Rohstoff entstehen: eine nach Sportart und Pausen gegliederte Aufzeichnung, Fotos und
Notizen, getippt oder gesprochen („tippen kann unterwegs nervig / schwer möglich sein“). Am
Ende steht ein Paket, das sich einfach weitergeben lässt und das die spätere Datenbank auf Base
ohne Umbau einlesen kann. Vorbild für die Fotos während der Aufnahme: komoot.

Fotos und Notizen hängen an der **Aufzeichnung** (Ort und Zeit), nicht an der geplanten Route
(Entscheidung des Nutzers, 2026-11-26).

## Ziele

### Schritt 1: Aufnahme mit Abschnitten

- **Start mit Sportart:** Der Aufnahmeknopf öffnet eine kurze Auswahl (Rad, MTB, Wandern,
  Skitour, „Keine Angabe“), die zuletzt genutzte ist vorausgewählt, ein Tipp startet.
  Ersetzt die Wahl der Sportart beim Beenden (Entwurf 2026-11-24, Nachtrag); „Speichern“ /
  „Verwerfen“ am Ende bleibt, ohne Sportartwahl.
- **Sportart wechseln** während der Aufnahme nur von Hand (Knopf in der Aufnahmeleiste); jeder
  Wechsel beginnt einen neuen Abschnitt mit der neuen Sportart.
- **Abschnitt = GPX-Segment mit eigener Sportart.** Neue Abschnitte entstehen bei
  Sportwechsel, Pause/Weiter (wie heute), einer GPS-Lücke über 2 min (wie heute) und bei
  erkannter Rast (unten). Abschnitte aus Rast, Pause oder Lücke behalten die Sportart.
- **Rasterkennung** (Entscheidung des Nutzers): Bleibt man **5 min** innerhalb von **50 m**,
  endet dort der laufende Abschnitt und ein **Rast-Abschnitt** beginnt (eigene Zeile in der
  Liste, etwa „Rast 12:10–13:05“; Entscheidung des Nutzers). Bewegt man sich mehr als 50 m vom
  Rastpunkt weg, endet die Rast und ein neuer Abschnitt mit der Sportart von vor der Rast
  beginnt. Beide Werte sind in den Einstellungen änderbar.
  - Der Schnitt fällt beim Erkennen; die 5 min davor bleiben im vorigen Abschnitt, gespeicherte
    Punkte werden nie umgeschrieben (Entscheidung von Claude).
  - Die Punkte während der Rast gehören zum Rast-Abschnitt (in GPX ein eigenes `<trkseg>`,
    Sportart-Erweiterung `rest`, siehe Export).
- **Drosseln bei Rast**, Einstellung mit drei Stufen (Entscheidung des Nutzers), Voreinstellung
  **Stark**, auch **während der laufenden Aufnahme** umstellbar (Menü an der Aufnahmeleiste):
  - **Stark:** grobe Ortung (etwa 100 m, `kCLLocationAccuracyHundredMeters`); spart am meisten,
    nach dem Losfahren können bis etwa 150 m fehlen. Für Radtouren.
  - **Mäßig:** etwa 10–30 m genau, seltener Punkte; höchstens etwa 50 m fehlen.
  - **Aus:** Rast wird nur markiert, das GPS bleibt voll genau (z. B. langsames Vorankommen im
    Whiteout auf dem Gletscher).
  - Das Ende der Rast muss auch bei grober Ortung erkannt werden: ein Fix zählt als „weg“, wenn
    er weiter als 50 m plus seine Genauigkeit vom Rastpunkt liegt (Entscheidung von Claude;
    Feinheiten in `docs/ALGORITHMEN.md`). Die Lücke vom Rastpunkt zum ersten Punkt danach
    schätzt die Statistik als gerade Linie.
- **Statistik** je Abschnitt; die der ganzen Tour ist die Summe der Abschnitte, jeder mit den
  Parametern seiner Sportart (Entscheidung von Claude). Rast zählt zur Gesamtzeit, nicht zur
  Bewegungszeit, und trägt weder Länge noch Höhenmeter bei. Der Punktabstand richtet sich wieder nach der Sportart des laufenden Abschnitts
  (`Activity::location_interval_ms`), da sie ab dem Start bekannt ist.
- **Sportart nachträglich ändern:** je Abschnitt in der Detailansicht (falls das Umschalten
  vergessen wurde); die bestehende Änderung für den ganzen Track setzt alle Abschnitte außer
  Rast (Entscheidung von Claude). Danach wird die Statistik neu gerechnet.
- **Nur Trail:** Rasterkennung ist im Kern standardmäßig aus und wird von Trail eingeschaltet;
  Base nimmt weiter auf wie bisher (Entscheidung von Claude).
- Der Name bleibt systematisch: `2026-11-24 10:05 Rad`, mit mehreren Sportarten die des ersten
  Abschnitts (Entscheidung von Claude; Umbenennen bleibt möglich).

### Schritt 2: Fotos und Notizen

- **Unterwegs** bekommt die Aufnahmeleiste große, mit Handschuhen treffbare Knöpfe:
  ```
  ┌──────────────────────────────────────┐
  │ ● 12,4 km  1:52 h  ↑ 430 m    Rad ▾  │
  │ [ 📷 Foto ] [ 🎤 Notiz ] [ ✎ Text ]  │
  │ [ Pause ]                [ Beenden ] │
  └──────────────────────────────────────┘
  ```
- **📷 während einer Aufnahme:** Kamera in Trail; das Foto bekommt Zeit, Ort (letzter Fix) und
  den laufenden Abschnitt, während einer Rast den Rast-Abschnitt, während Pause (von Hand) den
  letzten Abschnitt; vor dem ersten Punkt „ganze Tour“ (Entscheidung von Claude). Trail behält eine eigene Kopie in voller Auflösung (Originale in
  iCloud wären unterwegs nicht verfügbar). Zusätzlich in die Fotomediathek sichern ist eine
  Einstellung, Standard an (Entscheidung von Claude). Löschen in Trail löscht nur Trails Kopie.
- **📷 ohne Aufnahme:** Nur die Kamera; das Foto geht in die Fotomediathek, sonst nichts.
  (iOS kann die System-Kamera-App nicht direkt öffnen; eine Kamera in Trail ist der Ersatz.)
- **🎤 Sprachnotiz:** Aufnahme als Audio (Halten = sprechen, oder Tipp – Tipp zum Beenden);
  Zeit, Ort, Abschnitt wie beim Foto. **Umwandeln in Text** ist optional und auf dem iPhone,
  wann der Nutzer will (z. B. abends am Kabel), per Knopf je Notiz oder für alle einer Tour,
  nur mit Erkennung auf dem Gerät (keine Netzwerkaufrufe). Das Audio bleibt erhalten.
- **✎ Textnotiz:** Textfeld (die Diktiertaste der Tastatur geht dort ohnehin).
- **Nach der Tour** in der Detailansicht des Tracks: Abschnitte als Liste (Sportart, Zeit,
  Länge); Notizen je Abschnitt oder für die ganze Tour anlegen und ändern; unterwegs
  entstandene Notizen sind ihrem Abschnitt zugeordnet und änderbar (Text bearbeiten, Audio
  bleibt); Fotos ansehen und löschen (fragt nach); Notizen löschen.

### Schritt 3: Export als Paket

- In der Detailansicht „Tour exportieren“: Trail baut eine ZIP und gibt sie über „Teilen“
  weiter (Dateien, Kabel, Cloud, Syncthing; Linux hat kein AirDrop). Die GPX-Weitergabe wie
  heute bleibt.
- Inhalt:
  - `track.gpx` – jeder Abschnitt ein `<trkseg>`, die Sportart je Segment als GPX-Erweiterung
    in eigenem Namensraum; andere Programme lesen die Datei normal.
  - `fotos/<id>.jpg` – Originale, Zeit und Ort im EXIF.
  - `notizen/<id>.m4a` – Sprachnotizen.
  - `tour.json` – maschinenlesbar: Formatversion, Tour-ID, Name, Sportarten, Statistik,
    Abschnitte (ID, Sportart oder `rest`, Start, Ende, Länge, Grund des Beginns: Start,
    Sportwechsel, Pause, Rast, Rastende, Lücke), Fotos und Notizen (ID, Zeit, Ort, Abschnitt oder „ganze Tour“, Datei,
    Text, ob der Text aus Audio umgewandelt wurde, letzte Änderung).
  - `notizen.md` – dieselben Notizen lesbar, nach Abschnitten, mit Verweisen auf die Fotos.
- **Feste IDs** (UUID) für Tour, Abschnitte, Fotos, Notizen, damit die spätere Datenbank auf
  Base dieselbe Tour beim zweiten Import erkennt. Gelöschte Fotos und Notizen fehlen einfach.

## Nicht-Ziele

- Übertragung direkt zwischen Trail und Base bzw. einem Server (später, Nutzer: „evtl. schon“).
- Datenbank für Fotos und Notizen in Base (sicher geplant, eigenes Vorhaben).
- Tourberichte, Fotobücher; ob in Base oder als eigene App, Ollama oder eingebautes Modell:
  offen, eigenes Vorhaben.
- Bildklassifizierung, Personen erkennen per Few-Shot: zurückgestellt, nur Base.
- Fotos aus der Mediathek nachträglich einer Tour zuordnen (komoot kann das; hier nicht
  verlangt, später denkbar über Zeit/Ort).
- Base (Desktop) zeigt Fotos und Notizen nicht an; seine Aufnahme bleibt, wie sie ist.
- Tourenbuch (Statistik über Zeiträume, Reisen aus mehreren Tracks).

## Anti-Ziele

- Ein Foto oder eine Notiz geht nie verloren, weil die App endet oder abstürzt: gespeichert
  wird sofort beim Aufnehmen, wie die Punkte der Aufzeichnung.
- Bestehende Tracks öffnen und exportieren weiter (sie werden ein Abschnitt bzw. ihre
  Segmente mit der Sportart des Tracks).
- Die Drosselung kostet nie die Führung: Läuft die Führung, wird nicht gedrosselt.
- Keine Netzwerkaufrufe, auch nicht für die Umwandlung in Text.
- Die Logik liegt im Rust-Kern, nicht in Swift (Base und eine spätere Android-App bekommen
  Format und Logik ohne Nachbau).

## Rahmenbedingungen

- Akku ist Hauptziel der App (mehrtägige Touren ohne Strom). Kamera und Mikrofon laufen nur,
  wenn der Nutzer sie öffnet.
- Speicher: grob 3–5 MB je Foto; eine Woche mit 300 Fotos etwa 1,5 GB. Kein Verkleinern.
- iOS 16.4 ist die Mindestversion (wie bisher). Umwandlung in Text mit `SFSpeechRecognizer`
  und `requiresOnDeviceRecognition`; geht das für Deutsch auf dem Gerät nicht, sagt Trail das
  und lässt das Audio stehen.
- Datenbank `core-storage`: heute Migrationen 1–3 (`tracks`, `track_segments`,
  `track_points`, Routen). Neue Migration ergänzt, ändert nichts Bestehendes.
- Swift lässt sich unter Linux nicht bauen: Kern unter Linux, Trail auf dem Mac (wie bisher).

## Betrachtete Wege

| Weg | Abwägung |
|---|---|
| **1. Der Kern macht fast alles** (gewählt) | Kern: Datenbank (Sportart je Abschnitt, Fotos, Notizen), Rasterkennung im Aufzeichner, Paket schreiben. Swift: Kamera, Mikrofon, Umwandlung in Text, GPS drosseln, Teilen. Base und spätere Android-App bekommen Format und Logik geschenkt; unter Linux testbar. Mehr Arbeit über die Rust/Swift-Grenze. |
| 2. Fotos, Notizen, Export nur in Swift | Schneller; Format und Logik müsste Base nachbauen, widerspricht „Logik im Kern“. |

Entscheidung des Nutzers: Weg 1, Reihenfolge Schritt 1 → 2 → 3, jeder mit eigenem Stopp.
Export als Datei (A) statt Heimnetz (B); B kommt später mit der Datenbank auf Base.

## Entwurf

### Kern

- **Datenbank (neue Migration):** `track_segments` bekommt `activity` (Standard: die des
  Tracks) und `reason` (warum der Abschnitt begann). Neue Tabelle `track_media`: ID, Track,
  Abschnitt (oder keiner = ganze Tour), Art (`photo`, `voice`, `text`), Zeit, Ort, Datei
  (relativer Pfad im Datenordner der App, für Foto und Audio), Text, `transcribed`,
  `created_at`, `updated_at`, `deleted_at`, `rev` (wie `tracks`, Grundlage späterer Sync).
  `tracks.activity` bleibt (Sportart des ersten Abschnitts, für Liste und Name).
- **Aufzeichner (`core-track`):** Sportart je Abschnitt (Rast als eigene Art `rest`),
  `switch_activity(activity)` beginnt einen Abschnitt; Rasterkennung als reiner Zustandsautomat
  über die Fixe (Rastpunkt, Verweildauer, Radius, Genauigkeit), meldet „Rast begonnen“ /
  „Rast beendet“ und beginnt die Abschnitte selbst. Ein- und ausschaltbar. GPS-Zittern im Stand darf
  keine Bewegung vortäuschen: nur Fixe mit brauchbarer Genauigkeit zählen; die genaue Regel
  begründet der Bauende in `docs/ALGORITHMEN.md`.
- **App (`core-app`) / FFI:** Status der Aufnahme sagt zusätzlich Sportart des Abschnitts und
  ob gerade Rast ist (Swift drosselt danach). Funktionen zum Anlegen, Ändern, Löschen von
  Fotos und Notizen; Liste der Abschnitte und Medien eines Tracks; Export des Pakets in eine
  Datei (Pfad zurück an Swift). Dateien von Fotos/Audio übergibt Swift als fertige Datei im
  Datenordner; der Kern verwaltet nur Metadaten und packt beim Export.
- **GPX (`core-gpx`):** Schreiben und Lesen der Sportart-Erweiterung je Segment; beim Import
  ohne Erweiterung gilt die Sportart des Tracks.

### Trail

- Startauswahl der Sportart, Sportwechsel und Drosselstufe in der Aufnahmeleiste, Drosseln des
  GPS (`LocationService`: Profile „stark“ und „mäßig“ neben `standard`, nur während Rast und nur
  wenn keine Führung läuft).
- Kamera (`UIImagePickerController` oder AVFoundation, dem Bauenden überlassen), Audio
  (`AVAudioRecorder`, AAC/m4a), Umwandlung (`SFSpeechRecognizer`, nur auf dem Gerät).
  Berechtigungen (Kamera, Mikrofon, Spracherkennung, Fotomediathek hinzufügen) erst beim
  ersten Gebrauch erfragen; abgelehnt heißt: der Knopf sagt, wo man es erlaubt.
- Detailansicht: Abschnitte, Notizen, Fotos, Export; Teilen über `ShareLink`.
- Alles auch im OLED-Dunkelmodus. Selbsttests mit echten Eingaben, wo es geht; Kamera und
  Mikrofon im Simulator über Test-Ersatz.

## Dem Bauenden überlassen

- Genaue Gestalt der Knöpfe, der Startauswahl und der Detailansicht.
- Genaue Werte der Stufen „stark“ und „mäßig“ (`distanceFilter` usw.; mit Messung
  nachjustieren).
- Kamera-API, Dateinamen im Datenordner, Kompression der ZIP.
- Ob „Rast“ in der Aufnahmeleiste sichtbar ist (Empfehlung: kleiner Hinweis „Rast – GPS
  gedrosselt“).
