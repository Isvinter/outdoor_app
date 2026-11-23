# Messprotokoll

Gemessen am 2026-11-14 mit `scripts/measure-performance.sh` (Release-Build mit `lto = "fat"`,
Stand Meilenstein 6). Das Skript wiederholt alle Messungen; es braucht die Release-Binaries,
die gebaute Testregion und eine Desktop-Sitzung.

| | |
|---|---|
| Rechner | AMD Ryzen 9 7950X3D (32 Threads), 60 GB RAM |
| System | Ubuntu 26.04, Kernel 7.0, Wayland, WebKitGTK 4.1 |
| Fenster / Karte | 1060 × 900 Pixel Kartenfläche |
| Region | `isarwinkel-achensee` (90 MB), Stil „Topo Sommer“ |

CPU und Speicher gelten für die App **mit** den Prozessen ihres WebViews (WebKit lagert
Darstellung und Netz in eigene Prozesse aus); Speicher ist der anteilige Verbrauch (PSS).

## Ergebnisse

| Messung | Ergebnis |
|---|---|
| Start: Seite bereit (Oberfläche bedienbar) | 0,46–0,59 s nach Laden der Seite |
| Start: Offline-Karte vollständig gezeichnet | 0,99–1,07 s |
| Start bis Prozessende im Messlauf (inkl. Beenden) | 2,6–2,7 s |
| CPU im Leerlauf (Karte sichtbar, keine Positionsquelle) | 0,1–0,3 % eines Kerns |
| CPU bei Aufzeichnung (1 Fix/s, Karte folgt) | 9,5 % eines Kerns |
| Speicher im Leerlauf | 420–450 MB |
| Speicher bei Aufzeichnung | 510–580 MB |
| Kartenanimation (10 s Zoom, Drehung, Neigung) | 61,8 Bilder/s; Median 16 ms, 95 % unter 21 ms, längstes Bild 80 ms |
| Route Wandern Lenggries → Brauneck (5,8 km) inkl. Höhenprofil | 5–18 ms je Berechnung |
| Statistik eines Tracks mit 30 840 Punkten (`track-cli stats`) | 21 ms inkl. GPX lesen |
| Derselbe Track in die Datenbank aufzeichnen (`track-cli record`) | 47 ms |

## Befunde

- **Kartennachführung kostete CPU.** Die erste Messung ergab 33,5 % eines Kerns bei
  Aufzeichnung: Jeder Fix startete eine 500-ms-Animation der Karte, die Karte wurde also die
  halbe Zeit mit voller Bildrate neu gezeichnet. Jetzt springt die Karte bei kleinen Schritten
  (unter 60 Pixel) direkt und gleitet nur bei großen. Ergebnis 9,5 %. Der Rest ist ein
  Kartenbild pro Fix (Schummerung, Höhenlinien, Live-Linie) – ohne „Karte folgt der Position“
  entfällt auch das.
- **Leerlauf ist Leerlauf:** keine Polling-Schleifen; ohne Ereignisse zeichnet die Karte nicht.
- **Speicher** wird fast ganz vom WebView und den Kartenkacheln im Grafikspeicher bestimmt;
  der Rust-Kern hält nur die Verzeichnisse der PMTiles-Archive, einen kleinen Cache für
  Höhenkacheln und je Region einen geöffneten Router.
- Die Angabe `renderer` der Bildratenmessung (`Apple GPU`) ist ein fester Wert von WebKit und
  sagt nichts über die Grafikkarte.
- Nicht gemessen: Akkuverbrauch (Desktop-Rechner) und das Verhalten mit GPS-Hardware; das
  gehört zu Phase 2.

## Regionen in der App bauen

Gemessen am 2026-11-14 über die Oberfläche (`OUTDOOR_SELFTEST_BUILD`), gleicher Rechner.
Die Dauer gilt mit schon heruntergeladenen OSM-Auszügen; Höhendaten wurden während des
Baus geladen.

| Gebiet | Rechteck | OSM-Auszüge | Dauer | Paket | Schätzung |
|---|---|---|---|---|---|
| Testgebiet Lenggries | 11,60/47,60 – 11,75/47,70 (11 × 11 km) | oberbayern, austria | 41 s | 10 MB | 5 MB |
| Südtirol | 10,38/46,22 – 12,48/47,09 (160 × 97 km) | italy/nord-est, austria, italy/nord-ovest, switzerland | 1:28 min | 545 MB | 620 MB |

- Südtirol im Einzelnen: Grundkarte 82 MB, Outdoor-Ebene 28 MB, Gelände 283 MB, Hangneigung
  76 MB, Routing 76 MB. Höchster Speicherbedarf 5 GB (Planetiler).
- Rohdaten im Zwischenspeicher `data/sources`: die vier OSM-Auszüge zusammen 2,6 GB, dazu je
  Grad-Kachel 26 MB Höhendaten (Südtirol: 6 Kacheln). Sie bleiben für weitere Bauten liegen.
- Der Schätzfaktor (40 MB je 1 000 km²) trifft große Gebiete auf 12 %; kleine Gebiete werden
  größer als geschätzt, weil Grundkarte und Routing einen festen Anteil haben. Er bleibt.
- **Befund beim ersten Versuch:** Auszüge von verschiedenen Tagen (Österreich aus dem
  Zwischenspeicher vom Vortag, Italien frisch) enthielten dieselben Knoten in zwei Versionen;
  Planetiler brach mit „Nodes must be sorted ascending by ID“ ab. Seitdem behält
  `build-region.sh` nach dem Zusammenführen mit `osmium time-filter` nur die neueste Version.

## Lange Routen aus Teilstücken

Gemessen am 2026-11-15 mit `cargo run --release -p core-app --features valhalla --example
long_route -- <Datenordner> 300` auf der Region `alpenraum-ost` (4,7 GB, Ostalpen bis zur
Adria), gleicher Rechner. 300 Wegpunkte gleichmäßig auf der Linie München – Garmisch –
Innsbruck – Bozen – Trient – Belluno – Cortina – Lienz – Villach – Salzburg, Trekkingrad,
zwei Etappenenden.

| Messung | Ergebnis |
|---|---|
| Route | 2 048 km, 92 400 m Aufstieg, 299 Teilstücke, davon 4 ohne Weg |
| Ganze Route frisch (alle Teilstücke) | 2,5 s |
| Ein weiterer Wegpunkt (Rest aus dem Speicher) | 0,38 s |
| Antwort vollständig (138 000 Höhenproben, 102 000 Linienpunkte) | 16,4 MB |
| Antwort an die Oberfläche (Profil auf 2 000 Proben) | 3,8 MB, Ausdünnen und Umwandeln 5 ms |

- Vorher (eine Anfrage für alle Punkte) lehnte Valhalla Wanderrouten über 250 km
  Luftliniensumme ab; eine Route über 243 km mit 8 Punkten brauchte 0,14 s.
- Die 0,38 s für einen weiteren Punkt sind fast nur Zusammensetzen der 138 000 Höhenproben
  und Kopieren des Plans; das einzelne Teilstück braucht Millisekunden.
- **Befund:** Zwei Punkte, die auf dieselbe Stelle des Wegs einrasten, ergaben ein Teilstück
  ohne Länge und mit nur einer Höhenprobe; das galt als „Höhen unbekannt“ und blendete den
  Aufstieg der ganzen Route aus. Ein solches Teilstück hat jetzt 0 m.

## iOS I1: Offline-Karte

Stand 2026-11-15, MacBook Air (Xcode 27.2), MapLibre Native 7.0.0, Release-Build.

| Messung | Ergebnis |
|---|---|
| App-Größe (Release, arm64, unkomprimiert, ohne Regionen) | 14 MB: 7,9 MB MapLibre, 5,0 MB Programm mit Rust-Kern, 1,4 MB Schriften |
| Simulator (iPhone 17, iOS 27.2), Region `mini` (0,8 MB): Import | 5 ms |
| Simulator, Region `mini`: App-Start bis Karte vollständig gezeichnet | 290–320 ms (3 Läufe) |
| iPhone 13 Pro Max (iOS 27.2), `isarwinkel-achensee` (107 MB): Import | 92 ms |
| iPhone, `isarwinkel-achensee`: erster Start mit Import bis Karte vollständig gezeichnet | 1 205 ms |
| iPhone, `isarwinkel-achensee`: weitere Starts bis Karte gezeichnet | 162–174 ms (3 Läufe) |
| iPhone, ohne Region: Start bis Hintergrund gezeichnet | 154 ms |

- Gemessen ab dem Start des App-Codes (`ZeroGridTrailApp.init`), nicht ab dem Antippen des Symbols.
- Die Werte liefert der Selbsttest:
  `xcrun devicectl device process launch --console --device <id> de.zeibig.zerogrid.trail -- -selftest`
  schreibt nach dem ersten vollständigen Bild eine Zeile mit `import_ms` und `rendered_ms`.
- Der Import prüft die SHA-256-Summen aller Dateien (rund 1 GB/s) und verschiebt sie dann
  nur. Für das erste Bild holte die Karte 23 Kacheln und Schriftbereiche aus dem Kern.
- **Befund:** Das Paket lag zuerst direkt in `Import` statt in einem Unterordner, und die App
  übersah es. Sie nimmt jetzt beides. Danach lehnte sie das Paket zu Recht ab, weil die im
  Manifest genannte `routing.tar` fehlte; mit der Datei lief der Import durch.

## iOS I3: Planung

Stand 2026-11-16, MacBook Air (Xcode 27.2), valhalla-mobile 0.6.4 (Valhalla 3.9.1).

| Messung | Ergebnis |
|---|---|
| App-Größe (Release, arm64, unkomprimiert, ohne Regionen) | 30 MB: 19,4 MB Programm mit Rust-Kern und Valhalla, 8,1 MB MapLibre, 1,4 MB Valhalla-Ressourcen (tzdata, Standardkonfiguration), 1,4 MB Schriften |
| Zuwachs gegenüber I1 | 16 MB, davon 14,4 MB Programm (Valhalla statisch gelinkt) |
| Simulator (iPhone 17): Referenzwanderung, drei Punkte getippt | 12 352 m (Desktop 12 353 m), kein fehlgeschlagenes Teilstück |
| Simulator: erstes Teilstück, vom Tippen bis zur gezeichneten Route | 113 ms, davon rund 100 ms Öffnen von Valhalla (einmal je App-Start) |
| Simulator: Dauer je neues Teilstück, Median über 10 Wander-Teilstücke | 18 ms |
| Simulator, volles Paket (mit Höhenprofil): Median über 10 Wander-Teilstücke | 24 ms |
| iPhone 13 Pro Max (iOS 27.2), volles Paket: Referenzwanderung, drei Punkte getippt | 12 352 m, kein fehlgeschlagenes Teilstück |
| iPhone: erstes Teilstück nach dem App-Start | 129 ms (mit Öffnen von Valhalla) |
| iPhone: Dauer je neues Teilstück, Median über 10 Wander-Teilstücke | **41 ms** (zusammen 11,2 km, mit Höhenprofil) |

- `plan_ms` misst vom Eingang der Eingabe (Tipp, Loslassen eines Wegpunkts) bis die Route an
  die Karte übergeben ist. Nicht enthalten sind 0,3 s davor: Ein Tipp wartet so lange auf einen
  zweiten, damit ein Doppeltipp zoomt und keine zwei Punkte setzt. Diese Wartezeit kommt bei
  jedem getippten Punkt dazu.
- Die Werte im Simulator stammen aus einem Paket **nur mit Routing** (`routing.tar` aus dem
  Vorabtest, von `apps/trail/scripts/test.sh` zu einem Paket gemacht), also ohne Höhenprofil.
  Auf dem iPhone lief das volle Paket `isarwinkel-achensee`.
- Auf dem iPhone gemessen mit
  `DEVICE=<udid> apps/trail/scripts/test.sh -only-testing:ZeroGridTrailUITests/AcceptanceUITests`
  (`testReferenceHikeByTapping` und `testLegTimes`). Der Lauf nutzt die installierte Region,
  behält die App-Daten und löscht die Route, die er speichert. Der Test-Runner zählt als App:
  Das kostenlose Entwicklerprofil erlaubt drei auf dem Gerät.
- Valhalla schreibt weiter eine Zeile je Anfrage auf die Konsole. valhalla-mobile reicht den
  Abschnitt `logging` der Konfiguration nicht an Valhalla weiter, abstellen lässt es sich
  dort also nicht. Die Warnung zur fehlenden `traffic.tar` ist weg.

## iOS I4: Freigabe

Stand 2026-11-16, MacBook Air (Xcode 27.2), Simulator iPhone 17. Die Freigabe ist
`share-serve` auf demselben Mac, mit der Testregion `mini` (843 KiB) und einer Route.

| Messung | Ergebnis |
|---|---|
| App-Größe (Release, arm64, unkomprimiert, ohne Regionen) | 31 MB, davon 19,7 MB Programm und 8,0 MB MapLibre. Zuwachs gegenüber I3: rund 1 MB (Regionsverwaltung, Übernahme, Scanner; VisionKit gehört zum System) |
| Simulator: Tests | 66 grün (29 Unit, 37 UI), davon neu 7 für die Regionen, 3 für die Routen vom Desktop, 2 für den Scanner und die getippte Adresse |
| Simulator: Start mit nicht erreichbarer Quelle (`http://127.0.0.1:9/`) | Karte in unter 5 s gezeichnet, der Start liest den Katalog nicht (`testDesktopUnreachable`) |
| Simulator: Download bei 200 KiB/s abgebrochen und fortgesetzt | Die erste Meldung des zweiten Laufs steht bei ≥ 15 %, die Region wird danach installiert (`testInterruptedDownloadResumes`) |
| iPhone 13 Pro Max (iOS 27.2), `share-serve` auf dem Mac im WLAN: UI-Tests der Regionen und der Routen vom Desktop | 9 grün, 1 übersprungen (Scanner-Hinweis, nur Simulator). Beim allerersten Lauf scheiterten drei Tests, solange iOS nach dem Zugriff auf das lokale Netzwerk fragte; danach liefen sie durch |
| iPhone 13 Pro Max: Abnahme von Hand durch den Nutzer (App neu installiert, QR-Code gescannt, Region geladen, Route übernommen, im Flugmodus geladen, umgeplant und gespeichert, Region deaktiviert, aktiviert und gelöscht) | fehlerfrei, 2026-11-16 |
| iPhone: Download-Geschwindigkeit einer großen Region im WLAN | nicht gemessen |

- Der Simulator lädt von `127.0.0.1`. Das sagt nichts über die Geschwindigkeit im WLAN und
  fragt nicht nach dem lokalen Netzwerk.
- Den Scanner kann der Simulator nicht prüfen (keine Kamera). Dort ist nur geprüft, dass der
  Hinweis erscheint und „Adresse eingeben“ bleibt.

## Höhenlinien im Regionspaket

Gemessen am 2026-11-15 mit `scripts/build-region.sh <id> --steps contours,manifest`
(`tools/contour-tiles`, Release, alle Kerne), gleicher Rechner.

| Region | Höhendaten | Höhenlinien z11–14 | Anteil am Paket | Kacheln | Dauer | Speicher |
|---|---|---|---|---|---|---|
| isarwinkel-achensee | 42,5 MB | 16,9 MB (40 % der Höhendaten) | 16 % von 107 MB | 1 194 | 20 s | 0,4 GB |
| alpenraum-ost | 1 912 MB | 762 MB (40 %) | 13 % von 5,7 GB | 61 535 | 17:26 min | 1,4 GB |

- Je Zoomstufe bei `isarwinkel-achensee`: z11 0,4 MB, z12 1,1 MB, z13 5,6 MB, z14 9,9 MB.
  **z15 allein wären 34,2 MB** (zwei Drittel von 51 MB). Weil die Höhendaten bei z13 enden
  (rund 10 m je Pixel), bringt z15 nur interpolierte Linien. Daher hört das Paket bei z14 auf
  (Nutzerentscheidung), und die Karte vergrößert darüber.
- Die Kacheln liegen unkomprimiert im Archiv. gzip der ganzen Datei mit z15 brachte 51 → 36 MB.
  Das ist der nächste Hebel, falls Speicher auf dem Handy knapp wird.
- Nach dem Ergänzen von `alpenraum-ost`: Routen-Selbsttest (`OUTDOOR_SELFTEST_ROUTE`) mit den
  Referenzpunkten ok, Karte und Routing unverändert.
- Neu gebaut nach der Korrektur doppelter Punkte (MVT 2.1 verbietet LineTo ohne Länge); die
  Größen änderten sich nur um wenige Kilobyte.

## Kartenvorlagen und Suchindex

Echte Builds aller drei Vorlagen mit `scripts/build-region.sh <id> --package-dir
data/template-test`. Die beiden `urlaub`-Pakete wurden zuerst ganz gebaut und nach den Korrekturen
an Straßen und Orten nur mit `--steps search,manifest --force` neu erzeugt; `cusco` und
`cusco-fliegen` wurden ganz gebaut, mit den schon vorhandenen OSM-Quelldaten. Die installierte Region
`alpenraum-ost` und `data/regions/catalog.json` blieben unverändert: SHA-256 vorher und nachher
gleich (`c5e956e5...` für `manifest.json`, `af273705...` für `catalog.json`).

Größen je Datei und Vorlage (MB):

| Paket | Vorlage | basemap | outdoor | dem | slope | contours | search | routing | gesamt |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `alpenraum-ost-urlaub` (10 Auszüge) | urlaub | 984,2 | – | – | – | – | 87,8 | 1415,7 | 2487,7 |
| `cusco-urlaub` | urlaub | 3,46 | – | – | – | – | 0,55 | 6,40 | 10,41 |
| `cusco` | outdoor | 3,46 | 0,44 | 6,06 | 0,60 | 4,20 | 0,55 | 6,40 | 21,72 |
| `cusco-fliegen` | fliegen | 3,46 | 0,44 | 6,06 | – | 4,20 | 0,55 | – | 14,72 |

Cusco: bbox -72.10,-13.65,-71.80,-13.40, Geofabrik Peru.

- Grenzen: Paket höchstens 2,6 GB (2487,7 MB), `search.sqlite` höchstens 100 MB (87,8 MB). Beide
  eingehalten.
- Bauzeit und Spitzenspeicher (`/usr/bin/time -v` um das Skript): `cusco` 40,8 s und 4,1 GB,
  `cusco-fliegen` 37,9 s und 2,7 GB. Nur `search,manifest`: Alpenraum 1 min 38 s und 515 MB,
  `cusco-urlaub` 16,1 s und 77 MB. Erster ganzer Lauf der `urlaub`-Pakete: Alpenraum 7 min 16 s
  und 11,9 GB, Cusco 6 min 37 s und 3,4 GB.
- Zählung im Suchindex: Alpenraum 45 974 Orte, 305 709 Straßen, 432 852 POIs (davon 244 568 mit
  Namen); Cusco 217 Orte, 1215 Straßen, 2838 POIs (davon 2304 mit Namen).

Suchabfragen gegen den neuen Alpenraum-Index (`SearchIndex::search`, Release-Build, Limit 5, ein
Lauf je Abfrage, Position Innsbruck 47.2654, 11.3933):

| Abfrage | Ergebnis | Zeit |
| --- | --- | --- |
| `Maximilianstraße München` | Maximilianstraße in Neugermering und in Lehel | 5,5 ms |
| `Maximilianstraße Munich` | dieselben zwei Straßen | 4,5 ms |
| `Sonnenstraße München` | Hackenviertel, Feldkirchen, Aschheim, Oberschleißheim, Pullach | 4,7 ms |
| `Museumstraße Innsbruck` | Museumstraße, Dreiheiligen | 1,2 ms |
| `Erlerstraße Innsbruck` ohne Position | Erlerstraße in Innsbruck und in Hall in Tirol | 1,1 ms |
| `Erlerstraße Innsbruck` mit Position | Innsbruck 219 m, Hall in Tirol 8595 m | 0,5 ms |
| `Innsbruck Erlerstraße` | Erlerstraße in Innsbruck und in Hall in Tirol | 1,2 ms |
| `Erlerstr Innsb` | Erlerstraße in Innsbruck und in Hall in Tirol | 1,1 ms |
| `Innsbruck` | Ort Innsbruck, Innsbruckblick, Innsbrucker Straße (Kreuzbichl), pm Innsbruck, Innsbrucker Straße (Axams); keine Straße nur wegen des Ortes | 1,7 ms |
| `Bozen` ohne Position | Ort Bolzano - Bozen, Bozenhaldenstraße, Bozener Straße, Bozener Platz, Bozenau | 1,1 ms |
| `Bozen` mit Position in Bozen | Ort Bolzano - Bozen 20 m, dann POIs bis 242 m | 0,4 ms |
| `Bolzano` ohne Position | Orte Bolzano, Bolzano - Bozen, Bolzano Bellunese, Via Bolzano, Viale Bolzano | 1,1 ms |
| `Hostel Bergblick` | Hostel Bergblick, Kochel am See | 0,2 ms |
| `ha` mit Position | daniel's haare 31 m, La Habana 75 m, Chaos Hairconcept 179 m, Buchhandlung Haymon 184 m, Schmuck Julius Hampl 191 m | 20,0 ms |
| `ha` ohne Position | Orte Häusern, Häuser, Hauserdörfl, Haidenöster, Häggenschwil | 30,0 ms |
| nächste Apotheken (`by_category`) | Zentral-Apotheke 33 m, St.-Anna-Apotheke 111 m, Stadtapotheke 294 m | 0,2 ms |

- Cusco (`cusco-urlaub`, Position 13.5170 S, 71.9785 W, Limit 5): `Plaza de Armas` 40 m (0,2 ms),
  `hostal` 182 bis 321 m (0,1 ms); `Cusco` ohne Position (0,4 ms) zeigt zuerst POIs mit Cusco im
  Namen und keinen Ort, weil der Ort in OSM `Cuzco` heißt.

Base mit fehlenden Höhendaten (Release-Build, temporäres Datenverzeichnis, Kartenausschnitt und
10 s Kartenbewegung über die vorhandenen Diagnosen, Anfragen vorübergehend mitprotokolliert):

| Fall | vorher: Fehler, Höhenanfragen, gleiche Kachel höchstens | nachher |
| --- | --- | --- |
| Urlaubspaket, Ausschnitt | 8, 15, 2-mal | 0, 0, – |
| Urlaubspaket, Kartenbewegung | 71, 95, 2-mal | 0, 0, – |
| gemischt (`mini` und `cusco-urlaub`), Ausschnitt | 6, 28, 1-mal | 0, 26, 1-mal |
| Rand der Höhendaten (`mini`), Kartenbewegung | 30, 41, 3-mal | 0, 27, 1-mal |

## Rohdaten

```
== start (5 runs: process start to exit, page ready, map drawn)
  2719 ms total  {"map_drawn_ms":1070,"page_ready_ms":593}
  2579 ms total  {"map_drawn_ms":988,"page_ready_ms":466}
  2619 ms total  {"map_drawn_ms":1007,"page_ready_ms":466}
  2616 ms total  {"map_drawn_ms":990,"page_ready_ms":460}
  2595 ms total  {"map_drawn_ms":992,"page_ready_ms":461}
== running (vor der Korrektur der Kartennachführung)
idle (map shown)             CPU   0.1 % of one core, memory 422 MB
recording (1 fix per second) CPU  33.5 % of one core, memory 583 MB
== running (danach)
idle (map shown)             CPU   0.3 % of one core, memory 446 MB
recording (1 fix per second) CPU   9.5 % of one core, memory 507 MB
== map animation (10 s zoom, rotation and tilt)
{"canvas":"1060x900","fps_avg":61.76,"frame_ms_max":80,"frame_ms_median":16,"frame_ms_p95":21,"frames":618}
== route: "plan_ms":[5,18,15,14,17]
== long track: 30840 points; track-cli stats: 21 ms; track-cli record: 47 ms
```
