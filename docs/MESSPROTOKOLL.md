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
