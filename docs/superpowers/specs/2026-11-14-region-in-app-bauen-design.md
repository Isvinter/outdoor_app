# Entwurf: Region in der Desktop-App wählen und bauen

Stand: 2026-11-14 · Status: zur Freigabe

## Anlass und Ziel

Fertige Regionspakete lädt die App schon (Katalog, Download, Prüfsummen, Update, Löschen).
Ein neues Gebiet entsteht aber nur im Terminal: `regions/<id>.json` von Hand schreiben,
Geofabrik-Auszüge heraussuchen, `scripts/build-region.sh <id>` starten, Katalogordner wählen,
herunterladen.

Ziel: Am Desktop ein Gebiet auf der Karte aufziehen, benennen, „Bauen“ drücken – danach ist
die Region installiert und sichtbar. Der Haushalt soll ohne Server auskommen; die
Übertragung auf iOS/Android folgt als eigenes Vorhaben (Freigabe im Heimnetz, siehe unten).

## Ziele

- Rechteck auf der Karte wählen, Name vergeben, bauen, automatisch installieren.
- Geofabrik-Auszüge werden automatisch gewählt, auch über Ländergrenzen.
- Fortschritt in Schritten, Abbrechen, verständliche Fehlermeldung mit Protokoll.
- „Neu bauen“ einer selbst gebauten Region mit frischen OSM-Daten.
- Gebiete bis zur Größe „ganz Südtirol“ (ca. 15 500 km², geschätzt 0,5–1 GB) ohne Sonderweg;
  größere sind erlaubt, mit Rückfrage.

## Nicht-Ziele

- Freigabe im Heimnetz und mobile Clients (Vorhaben 2).
- Polygone statt Rechtecke; Wahl der Zoomstufen oder der MTB-Grenze in der Oberfläche.
- Windows/macOS, ein Docker-Image für die ganze Pipeline, mehrere Bauvorgänge gleichzeitig.
- Automatische Aktualisierung von Regionen.

## Anti-Ziele

- Eine halb gebaute oder ungeprüfte Region wird nie sichtbar.
- Die Oberfläche blockiert während des Baus nicht; Aufzeichnung und Karte laufen weiter.
- `build-region.sh` verhält sich ohne die neuen Optionen exakt wie bisher.
- Die mobilen Builds des Kerns (`cargo check` in der CI) bekommen keine neue Abhängigkeit.
- Kein doppelter Speicherverbrauch für fertige Pakete.

## Rahmenbedingungen

- Die Funktion gibt es nur, wenn die App aus dem Projektordner gebaut wurde und dort
  `scripts/build-region.sh` liegt, und nur unter Linux.
- Benötigt wie das Skript: bash, curl, python3, Java ≥ 21, Docker (laufender Daemon), cargo.
- Netz wird für OSM-Auszüge, Geofabrik-Index, Mapterhorn und Werkzeuge gebraucht.

## Betrachtete Wege

| Weg | Abwägung |
|---|---|
| **1. App startet das vorhandene Skript** (gewählt) | Erprobte Pipeline, Kommandozeile und späterer VPS nutzen dasselbe Skript. Bindet die Funktion an Projektordner und installierte Werkzeuge. |
| 2. Ganze Pipeline in einem Docker-Image | Nur Docker nötig, aber großes Image und Pflegeaufwand. Vom Nutzer zurückgestellt („evtl. später“), weil es Platz kostet. |
| 3. Pipeline in Rust in die App | Planetiler (Java) und Valhalla bleiben externe Programme; viel Aufwand ohne Gewinn. |

Entscheidung des Nutzers: Weg 1.

## Entwurf

### Ablauf für den Nutzer

```
Karten
 ┌ Neue Region ──────────────────────────┐
 │ [Gebiet auf Karte wählen]             │
 │ Name: [Dolomiten West            ]    │
 │ ca. 40 × 35 km · geschätzt 150 MB     │
 │ [Bauen]                               │
 └───────────────────────────────────────┘
 Dolomiten West   Schritt 3/7: Outdoor-Overlay …  [Abbrechen]
 Isarwinkel …     87 MB · installiert  [Anzeigen] [Neu bauen] [Löschen]
```

1. „Gebiet auf Karte wählen“ schaltet in den Auswahlmodus: Rechteck aufziehen, danach an
   Ecken und Kanten ziehbar. Für die Dauer der Auswahl zeigt die Karte den Online-Stil
   (außerhalb installierter Regionen zeigen die Offline-Stile nichts); danach der vorige Stil.
2. Unter dem Namen stehen Kantenlängen und geschätzte Paketgröße
   (Fläche × 40 MB je 1 000 km², aus der Testregion). Ab geschätzten 5 GB fragt „Bauen“ nach.
3. Während des Baus steht die Region mit Schritt „n/7: …“ in der Liste; „Abbrechen“ beendet
   den Bau. Ein erneutes „Bauen“ derselben Region setzt bei fertigen Schritten fort.
4. Nach dem Bau wird die Region geprüft, installiert und die Karte springt dorthin.
5. Fehler: Meldung mit den letzten Zeilen der Ausgabe und dem Pfad des Protokolls.
6. Fehlen Voraussetzungen, ist „Bauen“ gesperrt und ein Hinweis nennt, was fehlt.
7. „Neu bauen“ gibt es bei selbst gebauten Regionen; „Löschen“ entfernt dort auch die
   Beschreibung.

### Bausteine

**Neues Crate `crates/region-builder`** (nur Desktop, nicht Teil der mobilen CI-Prüfung):

- *Auszugswahl:* lädt `https://download.geofabrik.de/index-v1.json` (Polygone aller Auszüge)
  und wählt eine Menge von Auszügen, die das Rechteck überdeckt. Verfahren: Raster von
  Prüfpunkten im Rechteck; wiederholt den Auszug nehmen, der die kleinste Polygonfläche je
  neu überdecktem Punkt hat, bis alle überdeckbaren Punkte überdeckt sind. Punkte, die in
  keinem Auszug liegen (Meer), zählen nicht. Liegt kein Punkt in einem Auszug: Fehler.
- *Beschreibung:* schreibt `<Datenordner>/region-defs/<id>.json` im Format von `regions/*.json`.
  `id` ist der Name als Kleinbuchstaben-Slug (gültig nach `check_id` in `core-maps`), bei
  Kollision mit Zähler. `basemap_maxzoom` 14, `dem_maxzoom` 13. Attribution: OSM,
  OpenMapTiles und ein allgemeiner Mapterhorn-Hinweis mit Verweis auf deren Quellenliste.
- *Voraussetzungen:* prüft Projektordner, Werkzeuge, Java-Version, erreichbaren Docker-Daemon
  und liefert die Liste des Fehlenden.
- *Lauf:* startet das Skript als eigene Prozessgruppe, liest dessen Ausgabe, meldet Schritte,
  schreibt alles nach `<Datenordner>/region-build/<id>/build.log`, behält die letzten Zeilen
  für die Fehlermeldung. Abbrechen sendet SIGTERM an die Gruppe (damit `docker run` seine
  Container beendet) und wartet. Pfad des Skripts ist injizierbar (Tests mit Attrappe).
  Projektordner: beim Kompilieren festgehalten, per `OUTDOOR_REPO` überschreibbar.

**`scripts/build-region.sh` und `build-valhalla-tiles.sh`** – neue, optionale Schalter:

- `--config <datei>`: Beschreibung von dort statt aus `regions/<id>.json`.
- `--package-dir <ordner>`: Ausgabe nach `<ordner>/<id>/` statt `data/regions/<id>/`.
- `--refresh-osm`: lädt die OSM-Auszüge neu, statt die zwischengespeicherten zu nehmen.
- Vor jedem Schritt eine maschinenlesbare Zeile `== step <name> (<n>/<gesamt>)`.

Zwischenspeicher (`data/sources`, `data/tools`, `data/build`) bleiben im Projektordner und
werden mit der Kommandozeile geteilt.

**`core-maps::RegionStore::install_local(ordner)`**: installiert ein fertig gebautes Paket aus
einem lokalen Ordner – Manifest prüfen, Größe und SHA-256 jeder Datei prüfen, in den
Staging-Ordner **verschieben** (umbenennen; über Dateisystemgrenzen kopieren und löschen),
Archive öffnen, wie bisher aktivieren. Die App baut nach
`<Datenordner>/region-packages/`; nach der Installation ist das Paket dort weg, es liegt
also nur einmal auf der Platte.

**`core-app`**: `install_local_region(ordner)` als dünner Dienst; `RegionInfo` bekommt
`buildable: bool` (es gibt eine Beschreibung in `region-defs`). Beim Löschen einer solchen
Region wird die Beschreibung mit entfernt.

**Tauri-Hülle**: Commands `region_build_check`, `region_build_plan` (Rechteck → Auszüge,
Fläche, Schätzung), `region_build_start` (Name, Rechteck bzw. vorhandene `id` für „Neu
bauen“), `region_build_cancel`; Event `regions:build` (Region, Schritt, Anzahl, Zustand).
Ein Bau zur Zeit; beim Beenden der App wird ein laufender Bau abgebrochen.

**Frontend**: Auswahlmodus in `map.ts` (Rechteck als GeoJSON-Ebene mit Griffen), Formular
und Bauzeile in `regions.ts`, Texte in `i18n/de.ts`.

### Prüfung

- Tests: Auszugswahl gegen einen verkleinerten Index (ein Land, grenzüberschreitend, Meer,
  nichts überdeckt); Slug und Kollision; Lauf mit Attrappenskript (Schritte, Fehler, Abbruch);
  `install_local` (gut, falsche Prüfsumme, ersetzt ältere Version, Quelle danach leer).
- Layoutprüfung der Selbsttests um das neue Formular erweitern.
- Einmal echt: eine kleine neue Region und „ganz Südtirol“ über die Oberfläche bauen; Größe
  und Dauer ins `docs/MESSPROTOKOLL.md`, Schätzfaktor danach nachziehen.
- README (Abschnitt Offline-Karten) und PLAN.md ergänzen.

## Dem Umsetzer überlassen

- Geometrie (eigene Punkt-in-Polygon-Prüfung oder Crate), Rasterdichte der Auszugswahl.
- Genaue Gestaltung der Griffe und des Formulars im Rahmen des bestehenden Seitenleistenstils.
- Wortlaut und Link des Mapterhorn-Hinweises (gegen deren Seite prüfen).
- Ob der Abbruch per SIGTERM die Container zuverlässig beendet, ist zu prüfen; sonst
  Container benennen und gezielt stoppen.

## Ausblick: Vorhaben 2 (nicht Teil dieses Entwurfs)

Schalter „Karten im Heimnetz freigeben“: kleiner, nur lesender Webserver über die
**installierten** Regionen mit daraus erzeugtem `catalog.json`, Adresse und QR-Code. Mobile
Clients tragen die Adresse als Katalog ein und nutzen den vorhandenen Download.
