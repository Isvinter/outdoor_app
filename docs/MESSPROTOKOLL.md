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
| iPhone 13 Pro Max, `isarwinkel-achensee`: Import | offen |
| iPhone 13 Pro Max, `isarwinkel-achensee`: App-Start bis Karte gezeichnet | offen |

- Gemessen ab dem Start des App-Codes (`OutdoorApp.init`), nicht ab dem Antippen des Symbols.
- Die Werte liefert der Selbsttest:
  `xcrun devicectl device process launch --console --device <id> de.zeibig.outdoor -- -selftest`
  schreibt nach dem ersten vollständigen Bild eine Zeile mit `import_ms` und `rendered_ms`.
- Die iPhone-Werte stehen aus: Die App ist installiert, ließ sich aber nicht starten, weil
  das Gerät gesperrt war, und das Regionspaket liegt noch auf dem Linux-Rechner.

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
