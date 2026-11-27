# Übergabe Suche mit Wegdistanz an den Mac (Task 4)

Stand: 2026-11-27 · Branch `feature/search-route-distance` · Plan
`docs/superpowers/plans/2026-11-27-suche-wegdistanz.md` · Spec: `Nachtrag 2026-11-27` in der Spec dazu

## Erledigt (Linux)

- Task 1 (`core-routing`): `Router::matrix` mit fester Suchgrenze je Profil (Valhalla `max_matrix_distance`:
  Fahrrad 300 km, Fußgänger 200 km, Auto 400 km). Ziele darüber werden `None`.
- Task 2 (`core-app`): `RouteFrom { position, leg }`, `App::search_from`, `App::search_category_from`;
  `SearchHit` + `centre_distance_m`, `route_m`, `route_s`, `no_route`; `SearchResults` + `ascent_ignored`.
- Task 3 (`core-ffi`): `RouteFrom` (Record mit `KnownPosition` und `LegInput`), dieselben neuen Felder, und
  `CoreApp.search(text:near:radiusM:limit:from:)` / `searchCategory(category:near:radiusM:limit:from:)` mit
  `from: RouteFrom?`. Ein `LegInput` mit unbekanntem Profil zählt wie `nil`.
- `cargo test -p core-ffi` grün, clippy sauber.

## Weiter

`apps/trail/scripts/build-core.sh` und `xcodegen generate` in `apps/trail`, dann **Task 4** von
`docs/superpowers/plans/2026-11-27-suche-wegdistanz.md` (Swift: Umschalter, Zeile mit Weg, Hinweis).
Bis dahin baut der Swift-Aufrufer nicht: die FFI-Signatur von `search` und `searchCategory` hat jetzt
den Parameter `from` (übergib vorerst `nil`).

## Entscheidung des Nutzers (Spec „Nachtrag 2026-11-27“)

- Die Matrix sucht nur bis zur festen Grenze je Profil, eingestellt auf etwa 300 ms im ungünstigen Fall.
- Ziele, die sie nicht erreicht, zeigen in der UI **„Umweg oder kein Weg“** (Valhalla unterscheidet
  „zu weit“ nicht von „unerreichbar“).
- Trekking bei 50 km bleibt oft unerreicht, das ist Absicht: dann zählen Karte und Höhenprofil, die Liste ist nur
  grobe Orientierung.

## Messwerte (Desktop, Release, 30 Ziele auf einem Ring um Bozen, Median ms / berechnet von 30)

Mit den eingecheckten Werten, Radius 10 / 25 / 50 km:

| Profil | alpenraum-ost (1,4 GB) | Südtirol |
|---|---|---|
| Hike | 24/30, 95/30, 267/30 | 26/30, 65/30, 102/30 |
| Trekking | 69/30, 132/27, 135/10 | 42/30, 75/27, 71/8 |
| Mtb | 41/30, 111/30, 169/26 | 29/30, 65/30, 82/26 |
| RoadBike | 94/30, 237/28, 243/17 | 54/30, 85/28, 84/16 |
| Car | 65/30, 96/28, 152/27 | 61/30, 91/28, 105/24 |

Mit einem unerreichbaren Ziel zusätzlich (50-km-Ring + 1): alpenraum-ost Trekking 138 ms (10 berechnet), Mtb
173 ms (25); Südtirol Trekking 72 ms (7), Mtb 81 ms (26).

## Offene Punkte für Mac und iPhone

a. Die Grenzen sind am Desktop eingestellt. Auf dem iPhone die gefühlte Wartezeit messen: Kategorie „Camping“ bei
   herausgezoomter 50-km-Ansicht (Hike bei 50 km waren am Desktop 267 ms).
b. Alle POIs haben denselben `match_rank`, deshalb wirkt „Treffgüte zuerst“ bei Position nur auf Orte.
c. Der Bisect-Pfad für ein nicht platzierbares Ziel (bis zu 2n−1 Valhalla-Aufrufe) ist ungemessen; auf
   alpenraum-ost wurde kein solches Ziel gefunden.
