# Vorabtest: Valhalla offline auf dem iPhone

Übergabe für die Sitzung auf dem MacBook. Stand 2026-11-15.

## Worum es geht

Phase 2 beginnt mit der iOS-App (Android später, es gibt gerade kein Testgerät).
**Hauptmotivation des Nutzers: Routenplanung offline, mit Topokarte und Hangneigung.**
Ortung, Aufzeichnung, Export und Speichern kommen später.

Festgelegt (PLAN.md 8.1): native SwiftUI-App, Karte mit MapLibre Native, Rust-Kern per UniFFI.
Die Karte ist kein Risiko (Hangneigung ist eine Rasterebene; Höhenlinien müssen vom Bauskript
als Vektor-PMTiles vorberechnet werden). **Das Risiko ist Valhalla auf iOS.** Dieser
Wegwerf-Test klärt es, bevor die App entworfen wird.

## Frage

Kann eine iOS-App eine Route mit unseren Routingdaten (`routing.tar`, gebaut mit
Valhalla **3.9.1**, siehe `routing.json` des Pakets) ohne Netz berechnen, und wie schnell?

## Wege, in dieser Reihenfolge

- **B (zuerst):** [Rallista/valhalla-mobile](https://github.com/Rallista/valhalla-mobile) als
  Swift-Paket. Aktuelle README prüfen (Versionen und Bauanleitung widersprechen sich in
  älteren Quellen). Wichtig: Liest dessen Valhalla-Version unsere `routing.tar` (3.9.1)?
- **A (falls B scheitert):** das Rust-Paket `valhalla` 0.7.6 (wie `crates/core-routing` mit
  Feature `valhalla`) für `aarch64-apple-ios` / `aarch64-apple-ios-sim` bauen. Braucht
  Valhalla-C++ mit Boost und Protobuf für iOS.

Wegwerfcode: Mini-App unter `spikes/ios-valhalla/` (nicht in `apps/`), keine Tests nötig.
Ein Knopf rechnet die beiden Referenzrouten unten und zeigt Länge, Punktzahl und Dauer.

## Daten

`data/` ist nicht im Git. Vom Linux-Rechner kopieren:
`data/regions/isarwinkel-achensee/routing.tar` (19 MB) und `routing.json`
(z. B. per `scp` oder AirDrop). In die App bündeln oder ins Dokumentenverzeichnis legen.

## Referenz vom Desktop (Valhalla 3.9.1, Rust-Paket 0.7.6, Release, Linux x86_64)

Öffnen der `routing.tar`: 4 ms.

Wandern, 3 Punkte – Erwartung **12 353 m, 718 Punkte**, 2–11 ms:
```json
{"costing":"pedestrian","costing_options":{"pedestrian":{"max_hiking_difficulty":3,"use_hills":0.8}},"directions_type":"none","locations":[{"lat":47.6803,"lon":11.5741},{"lat":47.6625,"lon":11.5233},{"lat":47.645,"lon":11.556}],"units":"kilometers"}
```

MTB, 2 Punkte – Erwartung **5 832 m, 260 Punkte**, 1 ms:
```json
{"costing":"bicycle","costing_options":{"bicycle":{"avoid_bad_surfaces":0.0,"bicycle_type":"mountain","use_hills":0.8,"use_roads":0.1}},"directions_type":"none","locations":[{"lat":47.645,"lon":11.556},{"lat":47.68,"lon":11.57}],"units":"kilometers"}
```
Die Punkte sind die dekodierte `shape` (polyline6) aller Teilstücke zusammen.

## Erfolg

1. Beide Routen im **Simulator** und auf dem **iPhone** im Flugmodus berechnet.
2. Länge stimmt mit der Referenz überein (kleine Abweichung nur mit Begründung).
3. Dauer und App-Größe notiert.

## Ergebnis

Stand 2026-11-15, MacBook Air (Xcode 27.2). **Bestanden: alle drei Erfolgskriterien erfüllt,
im Simulator und auf dem iPhone 13 Pro Max im Flugmodus.**

**Weg B funktioniert, Weg A war nicht nötig.** `Rallista/valhalla-mobile` **0.6.4** als
Swift-Paket, fertiges XCFramework aus dem Release, nichts selbst in C++ gebaut.

- Version: 0.6.4 bindet Valhalla am Commit `bafb699` ein, das ist genau der Tag **3.9.1**,
  also dieselbe Version wie unsere `routing.tar`. Das Abzeichen „3.9.0“ im README ist veraltet.
- Mini-App: `spikes/ios-valhalla/` (`xcodegen generate`, dann Schema `ValhallaSpike`).
  Die `routing.tar` wird aus `data/regions/isarwinkel-achensee/` ins Bundle kopiert und
  dort direkt geöffnet (kein Entpacken, kein Kopieren ins Dokumentenverzeichnis).
- Aufruf: `Valhalla(ValhallaConfig(tileExtractTar:))`, dann `route(rawRequest:)` mit genau
  dem JSON vom Desktop. Die Antwort ist Valhallas eigenes JSON, also passt
  `parse_valhalla_route` aus `core-routing` unverändert.

Messwerte Simulator (iPhone 17, iOS 27.2, Release, Apple-Silicon-Mac, 20 Läufe je Route):

| | Ergebnis | erster Lauf | Median | Minimum |
| --- | --- | --- | --- | --- |
| Öffnen (Konfiguration schreiben, tzdata, Akteur) | – | 76 ms | – | – |
| Wandern, 3 Punkte | 12 353 m, 718 Punkte – exakt | 34 ms | 4,9 ms | 2,9 ms |
| MTB, 2 Punkte | 5 832 m, 260 Punkte – exakt | 3,0 ms | 2,3 ms | 1,3 ms |

Messwerte iPhone 13 Pro Max (iOS 27.2, Release, Flugmodus, 20 Läufe je Route):

| | Ergebnis | erster Lauf | Median | Minimum |
| --- | --- | --- | --- | --- |
| Öffnen (Konfiguration schreiben, tzdata, Akteur) | – | 99 ms | – | – |
| Wandern, 3 Punkte | 12 353 m, 718 Punkte – exakt | 23 ms | 2,6 ms | 2,6 ms |
| MTB, 2 Punkte | 5 832 m, 260 Punkte – exakt | 1,6 ms | 1,2 ms | 1,2 ms |

Das Gerät rechnet damit so schnell wie der Desktop (2–11 ms bzw. 1 ms).

App-Größe (Release, arm64, unkomprimiert): **32 MB**, davon 19,1 MB `routing.tar`,
12,7 MB Programm (Valhalla statisch gelinkt), 1,4 MB Paket-Ressourcen (tzdata, Standardkonfiguration).
Valhalla selbst kostet also rund 14 MB.

Stolpersteine:

- Das Öffnen ist langsamer als am Desktop (76–99 ms statt 4 ms), weil das Paket bei jedem
  `init` die Konfiguration nach Application Support schreibt und `tzdata.tar` nach
  `Library/tzdata` entpackt. Einmal je App-Start, daher unkritisch; die Instanz wiederverwenden.
- Die Instanz ist nicht für mehrere Threads gedacht (das Paket serialisiert mit einem Lock).
- Valhalla schreibt jede Anfrage auf die Konsole (`algorithm::bidirectional_a*`) und warnt
  wegen fehlender `traffic.tar` aus der Standardkonfiguration. Harmlos, für die echte App
  in der Konfiguration abstellen.
- Mindestversion iOS 16.4. Signieren mit dem kostenlosen „Personal Team“ klappt
  (Profil gilt 7 Tage).
- `devicectl … launch` braucht `--` vor App-Argumenten und ein entsperrtes Gerät.

Empfehlung für den Entwurf: Valhalla kommt auf iOS über valhalla-mobile in die Swift-Schicht,
nicht über das Rust-Paket `valhalla`. Der Rust-Kern (UniFFI) baut weiter die Anfrage
(`costing_json`) und wertet die Antwort aus (`parse_valhalla_route`); dazwischen reicht Swift
den JSON-Text an `route(rawRequest:)` durch. So bleibt `core-routing` ohne Feature `valhalla`
für iOS baubar, und Boost/Protobuf müssen nie für iOS übersetzt werden. Zu beachten: die
Valhalla-Version des Bauskripts und die von valhalla-mobile müssen zusammenpassen
(`routing.json` beim Laden prüfen).

Danach geht es mit dem Entwurf weiter (Brainstorming).
Arbeitsweise des Nutzers: Push erst nach einem Meilenstein, einmal vorher fragen;
nach jedem Meilenstein anhalten und zeigen.
