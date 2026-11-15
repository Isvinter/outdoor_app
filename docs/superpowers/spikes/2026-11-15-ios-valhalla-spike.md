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

Hier eintragen: welcher Weg, was gebaut werden musste, Stolpersteine, Messwerte,
Empfehlung für den Entwurf der iOS-App. Danach geht es mit dem Entwurf weiter (Brainstorming).
Arbeitsweise des Nutzers: Push erst nach einem Meilenstein, einmal vorher fragen;
nach jedem Meilenstein anhalten und zeigen.
