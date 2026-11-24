# Suche: Umkreis, Treffer auf der Karte und „Zurück“ – Umsetzungsplan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Die Base-Suche bezieht sich auf den sichtbaren Umkreis, zeigt alle Treffer als Punkte auf der Karte und führt vom gewählten Treffer per „Zurück“ zur Liste.

**Architecture:** Der Kern (`core-app`) wendet den Radius an, damit Base und später Trail dieselbe Regel haben. `MapView` kennt Punkte, Einpassen und Kameraposition, `SearchPanel` hält den Zustand der Suche (Bezugspunkt, Treffer, Überblick).

**Tech Stack:** Rust, SQLite/FTS5, Tauri, UniFFI, TypeScript, MapLibre GL.

**Spec:** `docs/superpowers/specs/2026-11-23-suche-umkreis-trefferkarte-design.md`

## Global Constraints

- Radius: halbe Diagonale des Ausschnitts, begrenzt auf **2 000 bis 50 000 m**, im Kern.
- Kein neues Indexformat, `SEARCH_FORMAT` bleibt 1, kein Neubau der Karten.
- Höchstens 30 Treffer im UI (`LIMIT` bleibt).
- Texte exakt: `search.back` = „Zurück“, `search.widened` = „Nichts im Umkreis, nächste Treffer weiter entfernt.“, `search.none_nearby` = „Keine Treffer im Umkreis. Zoome heraus.“
- Tippen: einpassen 500 ms nach dem Ergebnis. Kategorie: sofort.
- UI-Selbsttests nutzen echte Mausereignisse und fassen keine echten Daten an (`OUTDOOR_DATA_DIR` und `OUTDOOR_DATA` setzen).
- Trail-Oberfläche bleibt unverändert. Nichts pushen.
- `failure_carries_the_tail_of_the_output` in `region-builder` schlägt auch auf `main` zufällig fehl und ist kein Befund dieses Plans.

## Review Focus

1. Treffer anklicken, während das Einpassen noch läuft: „Zurück“ führt zum vollständigen Überblick, nicht zu einem Zwischenbild der Animation.
2. Nacheinander mehrere Punkte öffnen: „Zurück“ führt zur Liste und zum Überblick von vor dem ersten Treffer.
3. Kartenstil wechseln, während Treffer angezeigt werden: Die Punkte bleiben sichtbar und anklickbar.
4. Ungültiger Radius (`NaN`, unendlich, negativ): Der Kern rechnet mit einem gültigen Radius, statt alle Treffer zu verlieren.
5. Ein ferner Ort mit ähnlichem Namen und ein ferner POI ohne Treffer im Umkreis: Beide erscheinen, der POI wird nicht verdeckt.

---

### Task 1: Radius im Kern und bis ins Frontend

**Files:**
- Modify: `crates/core-search/src/read.rs`, `crates/core-search/src/lib.rs`
- Modify: `crates/core-app/src/search.rs`
- Modify: `crates/core-ffi/src/search.rs`
- Modify: `apps/base/src-tauri/src/commands.rs:77-97`
- Modify: `apps/base/ui/src/api.ts`, `apps/base/ui/src/map.ts`, `apps/base/ui/src/search.ts`
- Test: `crates/core-search/tests/search.rs`, `crates/core-app/tests/search.rs`

**Interfaces:**
- Produces (Rust):
  - `SearchIndex::by_category_within(&self, category: Category, near: (f64, f64), max_m: f64, limit: usize) -> Result<Vec<Hit>, SearchError>`
  - `Hit::is_whole_word_place(&self) -> bool`
  - `App::search(&self, text: &str, near: LatLon, radius_m: f64, limit: usize) -> SearchResults`
  - `App::search_category(&self, category: &str, near: LatLon, radius_m: f64, limit: usize) -> Result<SearchResults, AppError>`
  - `SearchResults { hits, unsearchable, widened: bool, radius_m: f64 }`, ebenso in `core-ffi`
- Produces (TS):
  - `SearchResults.widened: boolean`, `SearchResults.radius_m: number`
  - `api.searchText(text, near, radiusM, limit)`, `api.searchCategory(category, near, radiusM, limit)`; Tauri-Argument `radiusM`
  - `MapView.searchRadiusM(): number`

- [ ] **Step 1: Spec und Plan committen**

```bash
git add docs/superpowers/specs/2026-11-23-suche-umkreis-trefferkarte-design.md docs/superpowers/plans/2026-11-23-suche-umkreis-trefferkarte.md
git commit -m "Plan search radius, hits on the map and a way back to the list"
```

- [ ] **Step 2: Failing Tests schreiben**

In `crates/core-search/tests/search.rs`:

```rust
#[test]
fn category_within_stops_at_the_radius() {
    let dir = tempfile_dir("category-within");
    let index = SearchIndex::open(&build(&dir, &sample())).unwrap();
    let hits = index
        .by_category_within(Category::Parking, (47.0, 11.0), 5_000.0, 10)
        .unwrap();
    assert_eq!(hits.len(), 2);
    assert!(hits.iter().all(|h| h.distance_m.unwrap() <= 5_000.0));
}
```

In `crates/core-app/tests/search.rs` (Hilfsfunktion `place(name, lat, lon)` = `entry(Kind::Place, None, …)`; die vorhandenen Aufrufe bekommen `50_000.0` als Radius):

```rust
#[test]
fn radius_keeps_near_hits_and_places_everywhere() {
    let dir = common::temp_dir("search-radius");
    region(&dir, "a", "A", Some(&[
        lodging("Hostel Nah", -13.51, -72.0),
        lodging("Hostel Fern", -13.9, -72.0),
        place("Hostelito", -13.95, -72.0),
    ]));
    let results = common::open(&dir).search("Hostel", NEAR, 10_000.0, 30);
    let names: Vec<_> = results.hits.iter().map(|h| h.name.as_str()).collect();
    assert!(names.contains(&"Hostel Nah") && names.contains(&"Hostelito"));
    assert!(!names.contains(&"Hostel Fern"));
    assert!(!results.widened);
}

#[test]
fn nothing_in_radius_shows_the_nearest_with_a_note() {
    let dir = common::temp_dir("search-widened");
    region(&dir, "a", "A", Some(&[
        lodging("Hostel Fern", -13.9, -72.0),
        place("Hostelito", -13.95, -72.0),
    ]));
    let results = common::open(&dir).search("Hostel", NEAR, 10_000.0, 30);
    assert_eq!(results.hits.len(), 2);
    assert!(results.widened);
}

#[test]
fn a_far_place_with_the_whole_word_is_no_widening() {
    let dir = common::temp_dir("search-far-place");
    region(&dir, "a", "A", Some(&[
        place("Cusco", -13.9, -72.0),
        lodging("Cusco Hostel", -13.91, -72.0),
    ]));
    let results = common::open(&dir).search("Cusco", NEAR, 10_000.0, 30);
    assert_eq!(results.hits[0].name, "Cusco");
    assert_eq!(results.hits.len(), 2);
    assert!(!results.widened);
}

#[test]
fn category_stays_inside_the_radius() {
    let dir = common::temp_dir("search-category-radius");
    region(&dir, "a", "A", Some(&[lodging("Fern", -13.9, -72.0)]));
    let results = common::open(&dir)
        .search_category("lodging", NEAR, 10_000.0, 30)
        .unwrap();
    assert!(results.hits.is_empty());
    assert!(!results.widened);
}

#[test]
fn radius_applies_across_regions() {
    let dir = common::temp_dir("search-radius-regions");
    region(&dir, "a", "A", Some(&[lodging("Hostel Nah", -13.51, -72.0)]));
    region(&dir, "b", "B", Some(&[lodging("Hostel Fern", -13.9, -72.0)]));
    let results = common::open(&dir).search("Hostel", NEAR, 10_000.0, 30);
    assert_eq!(results.hits.len(), 1);
    assert_eq!(results.hits[0].name, "Hostel Nah");
}

#[test]
fn radius_is_held_between_2_and_50_km() {
    let dir = common::temp_dir("search-radius-limits");
    region(&dir, "a", "A", Some(&[lodging("Hostel", -13.51, -72.0)]));
    let app = common::open(&dir);
    assert_eq!(app.search("Hostel", NEAR, 1.0, 30).radius_m, 2_000.0);
    assert_eq!(app.search("Hostel", NEAR, 1e9, 30).radius_m, 50_000.0);
    assert_eq!(app.search("Hostel", NEAR, -5.0, 30).radius_m, 2_000.0);
    for invalid in [f64::NAN, f64::INFINITY] {
        assert_eq!(app.search("Hostel", NEAR, invalid, 30).radius_m, 50_000.0);
    }
}
```

Im Test `search_results_survive_the_bindings` (`core-ffi`) `widened: true, radius_m: 5_000.0` setzen und beide prüfen.

- [ ] **Step 3: Tests laufen lassen**

Run: `cargo test -p core-search -p core-app -p core-ffi`
Expected: FAIL beim Kompilieren, `by_category_within` fehlt und die Signaturen passen nicht.

- [ ] **Step 4: Kern umsetzen**

- `read.rs`: `by_category` und `by_category_within` teilen sich eine private Funktion mit `max_m: Option<f64>`. Mit Grenze wächst das Band höchstens bis `max_m / METRES_PER_LAT_DEGREE`, danach wird nach `distance_m <= max_m` gefiltert.
- `Hit::is_whole_word_place`: `match_rank < OTHER_RANK`.
- `core-app`: Konstanten `MIN_RADIUS_M = 2_000.0`, `MAX_RADIUS_M = 50_000.0`. Nicht endliche Werte gelten als Maximum, sonst `clamp`.
- `App::search` wendet nach dem Zusammenführen und Entfernen der Doppelten an:

```text
in_radius = Treffer mit distance_m <= radius
wenn in_radius nicht leer: behalte Treffer im Radius und alle Orte, widened = false
sonst: behalte alle, widened = (mindestens ein Treffer) und (kein is_whole_word_place)
```

Das Limit greift erst nach dieser Auswahl auf die übrig gebliebenen Treffer. `search_regions` darf deshalb nicht vorher auf `limit` kürzen: Es bekommt eine Filterfunktion oder liefert die sortierten, eindeutigen Treffer an die beiden Aufrufer zurück.

- `App::search_category` nutzt `by_category_within`, `widened` bleibt `false`.
- `core-ffi` und `commands.rs` reichen `radius_m` und die neuen Felder durch.

- [ ] **Step 5: Frontend anbinden**

`api.ts` und `search.ts` übergeben `view.searchRadiusM()`. Diese Funktion liefert die Entfernung von Kartenmitte zur Nordostecke des Ausschnitts (`getCenter().distanceTo(getBounds().getNorthEast())`).

- [ ] **Step 6: Prüfen**

Run: `cargo test -p core-search -p core-app -p core-ffi && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check && (cd apps/base/ui && pnpm typecheck)`
Expected: alles grün.

- [ ] **Step 7: Commit**

```bash
git add crates apps/base/src-tauri/src/commands.rs apps/base/ui/src
git commit -m "Search around what the map shows, with places found everywhere"
```

---

### Task 2: Treffer auf der Karte und Einpassen

**Files:**
- Modify: `apps/base/ui/src/map.ts`, `apps/base/ui/src/search.ts`, `apps/base/ui/src/i18n/de.ts`, `apps/base/ui/src/diagnostics.ts`
- Modify: i18n-Dateien neben `de.ts`, falls vorhanden

**Interfaces:**
- Consumes: `SearchResults.widened`, `SearchResults.radius_m`, `MapView.searchRadiusM()`
- Produces (`MapView`):
  - `type MapCamera = { center: [number, number]; zoom: number }`
  - `showSearchHits(points: LatLon[]): void`
  - `searchHitPositions(): LatLon[]` (für Selbsttests)
  - `onSearchHitClick(handler: (index: number) => void): void`
  - `onUserMove(handler: () => void): void`
  - `fitSearchHits(points: LatLon[]): MapCamera | null`
  - `camera(): MapCamera`, `moveTo(camera: MapCamera): void`
- Produces (`SearchPanel`, privat): `anchor`, `results`, `overview: MapCamera | null`, `fitTimer`

- [ ] **Step 1: Selbsttest ergänzen**

In `searchSelfTest` vor dem ersten `routeToHit` mit echten Mausereignissen auf dem Canvas. Dafür das Klicken aus `clickMap` in `clickCanvas(at)` ohne `jumpTo` auslagern. Neue Berichtswerte, alle in die `ok`-Liste:

```ts
report.dots_match_list          // searchHitPositions().length === hits().length > 0
report.dots_fit_in_view         // nach dem Einpassen liegen alle Treffer im Radius in getBounds()
report.typing_keeps_anchor      // zweite Eingabe nach dem Einpassen liefert dieselbe Trefferliste
report.dot_click_opens_hit      // clickCanvas(Punkt 1) zeigt die Karte mit dem Titel von Zeile 1
report.dots_survive_style_change // nach einem Stilwechsel bleiben die Punkte und ein Klick wirkt
report.picking_ignores_dot      // bei aktivem „Wegpunkte setzen“ setzt der Klick einen Wegpunkt, die Karte öffnet sich nicht
report.dots_gone_after_route    // nach „Route hierher“ ist searchHitPositions() leer
```

- [ ] **Step 2: Selbsttest laufen lassen**

Run (Testregion vorher nur als Kopie aus `data/template-test/cusco-urlaub` nach `$T/regions/cusco-urlaub` legen):

```bash
T=$(mktemp -d) && mkdir -p "$T/regions" && cp -r data/template-test/cusco-urlaub "$T/regions/" && (cd apps/base/ui && pnpm build) && OUTDOOR_DATA_DIR="$T" OUTDOOR_DATA="$T/data" OUTDOOR_SELFTEST_SEARCH="Hostel;-13.5170,-71.9785" cargo run -p zerogrid-base
```

Expected: FAIL, die neuen Funktionen fehlen.

- [ ] **Step 3: Karte umsetzen (`map.ts`)**

- Quelle und `circle`-Ebene `search-hits`: Radius 6, Füllung `#b3261e`, weißer Rand 2. Einrichten in `addOverlays` als oberste Ebene, Daten aus einem Feld, damit ein Stilwechsel sie wiederherstellt. Jedes Feature trägt `index`.
- `onSearchHitClick`: ein normaler `click`-Handler auf der Karte mit `queryRenderedFeatures` in ±4 px, geschützt durch `getLayer`. Er tut nichts, wenn `setPicking(true)` gilt oder eine Flächenauswahl läuft. `setPicking` merkt sich dafür den Zustand.
- Zeiger über einem Punkt: `pointer`, außer beim Picking.
- `onUserMove`: `movestart` mit gesetztem `originalEvent`.
- `fitSearchHits`: Für keinen Punkt `null`. Ein Punkt: Mitte auf den Punkt, Zoom `max(aktuell, 15)`. Mehrere: `cameraForBounds` mit `padding: 60`, `maxZoom: 15`. Bewegt die Karte mit `easeTo` über 600 ms und gibt das Ziel zurück.

- [ ] **Step 4: Panel umsetzen (`search.ts`)**

- `anchor: { centre: LatLon; radiusM: number } | null` wird beim ersten Suchen gesetzt und für jede weitere Suche verwendet. Zurückgesetzt wird er durch `onUserMove`, eine leere oder zu kurze Eingabe, Escape und `routeDone`.
- `renderResults` setzt `results` und die Punkte. Hinweise: `search.widened` bei `widened`, `search.none_nearby` bei leerer Kategorie, sonst wie bisher `search.none`.
- Einpassen: auf Treffer mit `distance_m <= results.radius_m`, sonst auf alle. Kategorie sofort, Text über `fitTimer` nach 500 ms. Der Timer endet bei neuer Eingabe, Auswahl, Leeren und Nutzerbewegung. Das Ziel wird in `overview` gespeichert, eine Nutzerbewegung setzt `overview` auf `null`.
- Klick auf einen Punkt ruft `select(results.hits[index])`. `select` beendet vorher eine angefangene „Route hierher“ über `host.selectionCleared()`, blendet die Liste aus, behält aber Punkte und `results`.
- `routeDone`, leere Eingabe und ein Suchfehler entfernen die Punkte und `results`.

- [ ] **Step 5: Selbsttest laufen lassen**

Run: wie Step 2.
Expected: `"ok": true`, alle Berichtswerte `true`.

- [ ] **Step 6: Commit**

```bash
git add apps/base/ui/src
git commit -m "Show search hits on the map and fit the map to them"
```

---

### Task 3: „Zurück“ zur Trefferliste

**Files:**
- Modify: `apps/base/ui/index.html:17-21`, `apps/base/ui/src/search.ts`, `apps/base/ui/src/style.css`, `apps/base/ui/src/i18n/de.ts`, `apps/base/ui/src/diagnostics.ts`, `README.md:84`

**Interfaces:**
- Consumes: `MapView.camera()`, `MapView.moveTo()`, `SearchPanel.overview`, `results`
- Produces: Button `#search-back` mit `data-i18n="search.back"`

- [ ] **Step 1: Selbsttest ergänzen**

```ts
report.back_restores_list    // gleiche Zeilen und Hinweise wie vor der Auswahl
report.back_restores_camera  // Mitte und Zoom wie vor der Auswahl (Toleranz 1e-4 / 0.01)
report.back_keeps_query      // Suchtext unverändert, keine neue Suche
report.back_after_two_hits   // Treffer öffnen, zweiten Punkt öffnen, Zurück führt zum ersten Überblick
report.back_during_fit       // Treffer vor dem Ende des Einpassens öffnen, Zurück führt zum Ziel des Einpassens
report.back_cancels_route_here // Zurück während der Frage nach dem Start beendet „Route hierher“
```

Alle in die `ok`-Liste aufnehmen. „Keine neue Suche“ prüft der Test, indem er `api.searchText` während des Klicks zählt.

- [ ] **Step 2: Selbsttest laufen lassen**

Run: wie in Task 2, Step 2.
Expected: FAIL, `#search-back` fehlt.

- [ ] **Step 3: Umsetzen**

- `index.html`: `<button id="search-back" data-i18n="search.back"></button>` als erstes Element in `#search-card`.
- `select`: Merkt beim ersten Treffer `returnTo = overview ?? view.camera()`. Weitere Treffer überschreiben `returnTo` nicht.
- `back()`: `host.selectionCleared()`, Pin entfernen, Karte ausblenden, Liste und Hinweise aus `results` neu zeichnen, Kategorie-Leiste wie vor der Auswahl (sichtbar genau dann, wenn das Suchfeld leer ist), `view.moveTo(returnTo)`. Kein neuer Aufruf der API.
- `returnTo` wird bei neuer Suche, Leeren und `routeDone` zurückgesetzt.
- README, Zeile 84: Selbsttest um Umkreis, Punkte und „Zurück“ ergänzen.

- [ ] **Step 4: Gesamtprüfung**

Run: `cargo test -p core-search -p core-app -p core-ffi && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check && (cd apps/base/ui && pnpm typecheck)`, danach der Selbsttest aus Task 2, Step 2.
Expected: alles grün, `"ok": true`.

- [ ] **Step 5: Commit**

```bash
git add apps/base/ui README.md
git commit -m "Go back from a chosen hit to the list of hits"
```

- [ ] **Step 6: Anhalten**

Meilenstein erreicht. Nicht pushen. Dem Nutzer sagen, was er von Hand prüfen soll: „camping“ suchen, Punkte anklicken, „Zurück“, Zoomstufen, eine ferne Stadt. Dazu nennen, was der Selbsttest nicht mit eigenen Augen gesehen hat.
