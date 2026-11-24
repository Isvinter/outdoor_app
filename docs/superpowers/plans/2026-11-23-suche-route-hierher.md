# Offline-Suche und „Route hierher“ Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Orte, Straßen und POIs in Base und Trail offline finden und mit wenigen Tipps eine Route dorthin bekommen.

**Architecture:** `core-search` versteht Schreibweisen, Kategorie-Wörter und sortiert. `core-app` sucht über alle Regionen und entscheidet die Regeln für Start und „ungespeichert“. `core-ffi` und die Tauri-Kommandos reichen das nur durch, Base und Trail zeigen an.

**Tech Stack:** Rust, SQLite FTS5, UniFFI, Tauri 2, TypeScript mit MapLibre, SwiftUI.

**Spec:** `docs/superpowers/specs/2026-11-23-suche-route-hierher-design.md`

## Global Constraints

- Branch `feature/search-route-here`. Am Ende PR, Merge lokal mit `--no-ff`. Push nur nach Rückfrage.
- **Nach jedem der drei Meilensteine anhalten und dem Nutzer das Ergebnis zeigen.**
- Die Reihenfolge der Wegpunkte wird nie verändert. Keine Netzwerkaufrufe in Suche oder Routing.
- `SEARCH_FORMAT` bleibt 1. Vorhandene Pakete bleiben lesbar.
- Sichtbare Texte auf Deutsch, wörtlich aus der Spec:
  - „Nicht gespeichert: Bei „Neue Route“ geht die alte verloren.“
  - „Tippe auf der Karte, wo die Route beginnen soll.“
  - „Dein Standort liegt nicht auf derselben Karte wie das Ziel. Tippe, wo die Route beginnen soll.“
  - „Standort wird gesucht …“, „Start selbst wählen“, „Abbrechen“, „Neue Route“, „Als nächstes Ziel anhängen“, „Route hierher“
  - „Für dieses Stück gibt es keine Routing-Daten.“
  - „Keine Route gefunden. Versuche ein anderes Profil, verschiebe den Punkt oder verbinde gerade.“
  - „Als gerade Linie verbinden“
  - „{Region} ist nicht durchsuchbar. Baue die Region neu.“
- Werte: Suche ab 2 Zeichen, UI-Limit 30, Kern-Limit 1 bis 100, Duplikate bis 30 m, Position höchstens 120 s alt, GPS-Wartezeit 30 s, Antwort unter 300 ms auf dem Rechner.
- Base-Selbsttests nutzen echte Mausereignisse und setzen **`OUTDOOR_DATA_DIR` und `OUTDOOR_DATA`** auf einen Testordner. Echte Karten in `data/` nicht anfassen oder löschen.
- Swift lässt sich unter Linux nicht kompilieren. Trail baut und testet der Nutzer auf dem Mac, vorher `apps/trail/scripts/build-core.sh`.
- Jeder Commit endet mit `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. Sonderzeichen in der Eingabe (`"`, `*`, `-`, `(`, nur Satzzeichen) lösen keinen FTS-Fehler aus, sondern liefern Treffer oder eine leere Liste. Test in Task 1.
2. Eine Region mit fehlender oder kaputter `search.sqlite` nimmt den anderen Regionen die Treffer nicht und wird genannt. Test in Task 2.
3. Eine Position kommt, nachdem der Nutzer „Start selbst wählen“ oder „Abbrechen“ gedrückt hat: Es entsteht keine Route und kein zweiter Startpunkt. Selbsttest in Task 7, Handtest in Task 10.
4. „Anhängen“ an eine Route mit nur einem Punkt oder mit gescheitertem letzten Teilstück lässt vorhandene Teilstücke und Reihenfolge unverändert. Test in Task 3 und Task 7.
5. Eine GPX-Wiedergabe liefert alte Zeitstempel: Das Alter der Position zählt ab dem Empfang in Base, nicht ab `Fix.time_ms`. Selbsttest in Task 7.

---

## Meilenstein 1: Kern

### Task 1: Suche versteht Schreibweisen, Kategorie-Wörter und Ortsgröße

**Files:**
- Modify: `crates/core-search/src/normalize.rs`, `crates/core-search/src/read.rs`, `crates/core-search/src/lib.rs`
- Create: `crates/core-search/src/words.rs`
- Modify: `tools/search-index/src/lib.rs:250-262`, `tools/search-index/src/towns.rs:11`
- Test: `crates/core-search/tests/search.rs`, `tools/search-index/tests/run.rs`

**Interfaces:**
- Produces:
  - `pub fn query_words(text: &str) -> Vec<Vec<String>>`: je eingegebenes Wort seine Varianten
  - `pub fn category_for_word(text: &str) -> Option<Category>`
  - `Kind::as_str(self) -> &'static str`: `place`, `street`, `poi`
  - `Category::as_str(self) -> &'static str`: `fuel`, `groceries`, `food`, `pharmacy`, `lodging`, `camping`, `parking`, `retail`, `sights`
  - `Category::parse(text: &str) -> Option<Category>`
  - `Hit.match_rank: u8`: 0 Stadt, 1 Kleinstadt, 2 Dorf, 3 Ortsteil, 4 Ort ohne Rang (jeweils mit allen Suchwörtern als ganze Wörter), 5 alles andere

- [ ] **Step 1: Failing tests schreiben**

```rust
#[test] fn umlaut_query_finds_stored_ae_spelling() // gespeichert "Muenchen", search("München") findet es
#[test] fn strasse_finds_separate_str() // gespeichert "Haupt Str.", search("Haupt Straße") findet es
#[test] fn punctuation_never_breaks_the_query() {
    for text in ["\"", "*", "-", "(", "a\"b", "NEAR(", "AND", "name:x"] {
        assert!(index.search(text, Some((47.0, 11.0)), 10).is_ok(), "{text}");
    }
}
#[test] fn bigger_place_beats_nearer_street() {
    // "Cusco" (subtype "city", 20 km entfernt), Straße "Calle Cusco" (100 m)
    assert_eq!(names(&hits)[0], "Cusco");
    assert_eq!(hits[0].match_rank, 0);
}
#[test] fn city_beats_nearer_village_of_same_name()
#[test] fn alt_name_counts_as_whole_word() // Ort "Bozen", alt "Bolzano", Suche "Bolzano" => match_rank 0..=4
#[test] fn prefix_of_place_is_not_a_whole_word() // Suche "Cus" => "Cusco" hat match_rank 5
#[test] fn place_without_rank_stays_readable() // subtype None => match_rank 4
#[test] fn category_words() {
    for w in ["Hostel", "hotel", "Unterkunft", "hostal"] { assert_eq!(category_for_word(w), Some(Category::Lodging)); }
    for w in ["Apotheke", "Farmacia", "pharmacy"] { assert_eq!(category_for_word(w), Some(Category::Pharmacy)); }
    for w in ["Tankstelle", "Grifo", "gasolinera"] { assert_eq!(category_for_word(w), Some(Category::Fuel)); }
    assert_eq!(category_for_word("Supermarkt"), Some(Category::Groceries));
    assert_eq!(category_for_word("Hostel Cusco"), None);
    assert_eq!(category_for_word("Host"), None);
}
```

In `tools/search-index/tests/run.rs`: Ein gebauter Ort `place=city` hat `subtype == Some("city")`, `suburb` hat `Some("part")`.

- [ ] **Step 2: Tests laufen lassen**

Run: `cargo test -p core-search -p search-index`
Expected: FAIL, `query_words`, `category_for_word` und `match_rank` fehlen.

- [ ] **Step 3: Implementieren**

`query_words`: Text klein schreiben, an Nicht-Alphanumerischem trennen. Je Wort die Varianten `normalize(word)` und `normalize(word mit ä→ae, ö→oe, ü→ue)`. `strasse` bekommt zusätzlich die exakte Variante `str`.

FTS-Abfrage in `search`: je Wort `{name town} : ("v1"* OR "v2"*)`, verbunden mit `AND`, dazu mindestens ein Wort im Namen wie bisher. Da `name_terms` contentless ist, wird „ganzes Wort“ mit einer zweiten Abfrage ohne `*` bestimmt: alle Wörter in `name`, nur Zeilen mit `kind = 0`, die `rowid` als `HashSet<i64>`. Das Select liefert zusätzlich `e.id`.

Sortierung mit Standort: `(match_rank, distance_m)`, im Heap dieselbe Ordnung, damit ein ferner Ort mit Rang 0 nicht wegfällt. `by_category` liefert `match_rank = 5`.

`category_for_word`: `normalize` der Eingabe muss genau ein Wort sein und in dieser Tabelle stehen:

| Kategorie | Wörter |
|---|---|
| Lodging | hostel, hostal, hotel, pension, unterkunft, hospedaje, alojamiento, lodging |
| Pharmacy | apotheke, farmacia, botica, pharmacy |
| Fuel | tankstelle, grifo, gasolinera, fuel |
| Groceries | supermarkt, lebensmittel, supermercado, mercado, grocery, supermarket |
| Food | restaurant, essen, restaurante, cafe, food |
| Camping | camping, campingplatz, campsite |
| Parking | parkplatz, parken, parking, estacionamiento |
| Sights | sehenswuerdigkeit, sehenswert, sights, museum, museo |

Einträge werden über `normalize` und die Umlaut-Variante der Eingabe verglichen.

Index-Werkzeug: `Rank::as_str` (`city`, `town`, `village`, `part`), in `build_into` als `subtype` des Orts schreiben.

- [ ] **Step 4: Tests laufen lassen**

Run: `cargo test -p core-search -p search-index && cargo clippy -p core-search -p search-index --all-targets -- -D warnings`
Expected: PASS, keine Warnungen.

- [ ] **Step 5: Commit**

```bash
git add crates/core-search tools/search-index
git commit -m "Rank places by size and understand spellings and category words"
```

### Task 2: Suche über alle Regionen

**Files:**
- Create: `crates/core-app/src/search.rs`
- Modify: `crates/core-app/src/lib.rs`, `crates/core-app/src/error.rs`, `crates/core-app/Cargo.toml`, `crates/core-maps/src/store.rs`
- Test: `crates/core-app/tests/search.rs`

**Interfaces:**
- Consumes: `query_words`, `category_for_word`, `Category::parse`, `Hit.match_rank` aus Task 1.
- Produces:

```rust
// core-maps
pub struct SearchSource { pub region: String, pub name: String, pub path: Option<PathBuf> }
impl RegionStore { pub fn search_sources(&self) -> Vec<SearchSource>; } // nur aktivierte Regionen; path None ohne Suchindex

// core-app
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SearchHit { pub kind: String, pub category: Option<String>, pub name: String,
    pub town: Option<String>, pub lat: f64, pub lon: f64, pub distance_m: f64 }
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct SearchResults { pub hits: Vec<SearchHit>, pub unsearchable: Vec<String> } // Regionsnamen
impl App {
    pub fn search(&self, text: &str, near: LatLon, limit: usize) -> SearchResults;
    pub fn search_category(&self, category: &str, near: LatLon, limit: usize) -> Result<SearchResults, AppError>;
}
// AppError::SearchCategory(String), Code "search_category"
```

- [ ] **Step 1: Failing tests schreiben**

```rust
#[test] fn finds_in_every_enabled_region()
#[test] fn disabled_region_is_not_searched()
#[test] fn duplicate_within_30_m_appears_once() // gleiche Art, Kategorie, Name in zwei Regionen, 10 m auseinander
#[test] fn same_name_50_m_apart_stays_twice()
#[test] fn region_without_index_is_named() // unsearchable == ["Alt"], Treffer der anderen bleiben
#[test] fn damaged_index_is_named_and_others_still_hit()
#[test] fn category_word_adds_unnamed_kind() // POI "Pariwana" (Lodging), search("Hostel") findet ihn
#[test] fn category_word_does_not_duplicate_name_hit() // "Hostel Alpina" nur einmal
#[test] fn two_words_stay_name_search() // "Hostel Cusco" findet "Pariwana" nicht
#[test] fn too_short_input_is_empty() // "a" => keine Treffer
#[test] fn limit_is_clamped() // usize::MAX läuft nicht über, höchstens 100 Treffer
#[test] fn unknown_category_is_an_error() // code "search_category"
```

- [ ] **Step 2: Tests laufen lassen**

Run: `cargo test -p core-app --test search`
Expected: FAIL, Methoden fehlen.

- [ ] **Step 3: Implementieren**

`SearchIndex` je Aufruf neu öffnen und danach schließen. Das ist schnell genug und hält nie eine Verbindung auf eine ersetzte Datei offen. Jede Region einzeln mit dem begrenzten Limit abfragen, Fehler beim Öffnen oder Lesen tragen die Region in `unsearchable` ein. Zusammenführen nach `(match_rank, distance_m)`, Duplikate entfernen (`Kind`, `Category`, `normalize(name)`, Abstand ≤ 30 m), auf das Limit kürzen. Unbenannte POIs behalten `name == ""`, die Apps zeigen die Kategorie.

- [ ] **Step 4: Tests laufen lassen**

Run: `cargo test -p core-app --test search && cargo clippy -p core-app -p core-maps --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/core-app crates/core-maps Cargo.lock
git commit -m "Search all installed regions from the app core"
```

### Task 3: Regeln für Start, „ungespeichert“ und gerade Linie

**Files:**
- Modify: `crates/core-app/src/plan.rs`, `crates/core-app/src/routes.rs`, `crates/core-app/src/lib.rs`
- Test: `crates/core-app/tests/plan_edit.rs`, `crates/core-app/tests/routes.rs`

**Interfaces:**
- Produces:

```rust
pub const MAX_POSITION_AGE_S: f64 = 120.0;
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct KnownPosition { pub lat: f64, pub lon: f64, pub age_s: f64 }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteStart { Ready, NoPosition, OtherMap }
impl App {
    pub fn route_start(&self, position: Option<KnownPosition>, target: LatLon) -> RouteStart;
    pub fn route_is_unsaved(&self, request: &PlanRequest) -> bool;
}
impl PlanRequest { pub fn set_leg(&self, index: usize, leg: LegSpec) -> Self; }
impl LegError { pub fn can_connect_straight(&self) -> bool; }
```

- [ ] **Step 1: Failing tests schreiben**

```rust
#[test] fn fresh_position_in_same_region_is_ready()
#[test] fn position_older_than_two_minutes_is_no_position() // age_s 120.1 => NoPosition, 120.0 => Ready
#[test] fn position_in_other_region_is_other_map()
#[test] fn target_without_routing_is_other_map()
#[test] fn disabled_region_does_not_count()
#[test] fn empty_route_is_not_unsaved()
#[test] fn new_route_is_unsaved()
#[test] fn saved_route_is_not_unsaved_until_changed()
#[test] fn loaded_route_is_not_unsaved_until_changed()
#[test] fn set_leg_changes_only_that_leg() // Wegpunkte, Etappen und andere Teilstücke bleiben
#[test] fn append_keeps_order_and_existing_legs() // auch bei einem Punkt und bei gescheitertem letzten Teilstück
#[test] fn straight_line_only_for_missing_data_or_no_route() {
    // true: "routing_unavailable", "route_not_found"; false: "routing_incompatible", "routing"
}
```

- [ ] **Step 2: Tests laufen lassen**

Run: `cargo test -p core-app --test plan_edit --test routes`
Expected: FAIL.

- [ ] **Step 3: Implementieren**

`route_start` nutzt `routing_tiles(&[position, target])`. `route_is_unsaved`: keine Wegpunkte `false`, keine geladene Route `true`, sonst Vergleich des Requests mit dem gespeicherten Stand (Lesen wie `loaded_route_is_edited`, unlesbarer Stand gilt als ungespeichert).

- [ ] **Step 4: Tests laufen lassen**

Run: `cargo test -p core-app && cargo clippy -p core-app --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/core-app
git commit -m "Decide where a route to a result starts and if it is unsaved"
```

### Task 4: Kern für beide Apps freigeben

**Files:**
- Create: `crates/core-ffi/src/search.rs`
- Modify: `crates/core-ffi/src/lib.rs`, `crates/core-ffi/src/plan.rs`, `apps/base/src-tauri/src/commands.rs`, `apps/base/src-tauri/src/main.rs`, `apps/base/ui/src/api.ts`
- Test: bestehende Tests in `crates/core-ffi`, neuer Test in `crates/core-ffi/src/search.rs`

**Interfaces:**
- Consumes: Tasks 2 und 3.
- Produces:
  - UniFFI: Records `SearchHit`, `SearchResults`, `KnownPosition`, Enum `RouteStart`; `CoreApp.search(text, near: LatLon, limit: u32) -> SearchResults`, `CoreApp.searchCategory(category, near, limit) throws`, `CoreApp.routeStart(position: KnownPosition?, target: LatLon) -> RouteStart`, `CoreApp.routeIsUnsaved(request: PlanInput) -> Bool`, freie Funktionen `planSetLeg(request, index: UInt32, leg: LegInput) -> PlanInput`, `legCanConnectStraight(code: String) -> Bool`.
  - Tauri: `search_text`, `search_category`, `route_start_check`, `route_is_unsaved`.
  - `api.ts`: `searchText(text, near, limit)`, `searchCategory(category, near, limit)`, `routeStartCheck(position, target)`, `routeIsUnsaved(request)`, Typen `SearchHit`, `SearchResults`, `KnownPosition`, `RouteStart = "ready" | "no_position" | "other_map"`.

- [ ] **Step 1: Failing test schreiben**

```rust
#[test] fn search_results_survive_the_bindings() // Felder von core_app::SearchResults kommen unverändert an
```

- [ ] **Step 2: Test laufen lassen**

Run: `cargo test -p core-ffi`
Expected: FAIL.

- [ ] **Step 3: Weiterleitungen ergänzen**

Keine eigene Logik. Suche in Tauri über den vorhandenen Helfer `blocking`.

- [ ] **Step 4: Alles prüfen**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && npm --prefix apps/base/ui run typecheck`
Expected: PASS.

- [ ] **Step 5: Commit und Meilenstein zeigen**

```bash
git add crates/core-ffi apps/base
git commit -m "Offer search and route start to the apps"
```

**Meilenstein 1: anhalten, Testergebnis zeigen, Freigabe für Base abwarten.**

---

## Meilenstein 2: Base

### Task 5: Suchleiste und Trefferliste

**Files:**
- Create: `apps/base/ui/src/search.ts`
- Modify: `apps/base/ui/index.html`, `apps/base/ui/src/main.ts`, `apps/base/ui/src/map.ts`, `apps/base/ui/src/style.css`, `apps/base/ui/src/i18n/de.ts`

**Interfaces:**
- Consumes: `api.searchText`, `api.searchCategory`.
- Produces:

```ts
export interface SearchPanelHost { routeHere(hit: SearchHit): void; showError(error: unknown): void; }
export class SearchPanel { constructor(view: MapView, host: SearchPanelHost); start(): void; }
// MapView
showSearchPin(at: LatLon | null): void;
centre(): LatLon;
flyToResult(at: LatLon): void;
```

- [ ] **Step 1: Oberfläche bauen**

Oben in `#sidebar`, direkt unter dem Header, liegt `<section id="search">` mit `#search-input`, `#search-categories`, `#search-results`, `#search-notes` und `#search-card`. Die Kategorie-Knöpfe in dieser Reihenfolge: Unterkunft, Essen, Lebensmittel, Apotheke, Tankstelle, Camping, Parken, Sehenswertes, sichtbar bei leerem Feld.

Beim Tippen ab 2 Zeichen nach 200 ms suchen, mit der Kartenmitte als `near` und Limit 30. Eine Zähl-Variable verwirft ältere Antworten, wie `RoutePanel.request`. Ein Treffer schließt die Liste, zentriert die Karte, setzt die Markierung und füllt `#search-card` mit Name oder Kategorie, Art, Ort, Entfernung und `#search-route` („Route hierher“). `unsearchable` erscheint in `#search-notes`.

- [ ] **Step 2: Prüfen**

Run: `npm --prefix apps/base/ui run build`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add apps/base/ui
git commit -m "Search places from the Base sidebar"
```

### Task 6: „Route hierher“ in Base

**Files:**
- Modify: `apps/base/ui/src/route.ts`, `apps/base/ui/src/main.ts`, `apps/base/ui/index.html`, `apps/base/ui/src/style.css`, `apps/base/ui/src/i18n/de.ts`

**Interfaces:**
- Consumes: `api.routeStartCheck`, `api.routeIsUnsaved`, `SearchPanelHost`.
- Produces:

```ts
class RoutePanel {
  routeHere(target: LatLon): Promise<void>;
  setPosition(fix: Fix): void;          // merkt die Empfangszeit mit Date.now()
  setSourceStatus(status: SourceStatus | null): void;
}
```

- [ ] **Step 1: Ablauf bauen**

1. Mit Wegpunkten zeigt `#route-here-choice` „Neue Route“, „Als nächstes Ziel anhängen“, „Abbrechen“, die Warnung nur wenn `routeIsUnsaved`. Ohne Wegpunkte weiter mit 2.
2. **Anhängen:** Ziel als letzten Wegpunkt mit dem gemerkten Profil, ohne die Form-Änderung auf das bisherige letzte Teilstück anzuwenden.
3. **Neue Route:** `routeStartCheck` mit der Position.
   - `ready`: Route `[Position, Ziel]`.
   - `other_map`: Hinweis „Dein Standort …“, dann Tipp auf die Karte.
   - `no_position` mit Quelle `connecting` oder `active`: `#route-here-wait` zeigt „Standort wird gesucht …“ mit „Start selbst wählen“ und „Abbrechen“. Kommt eine Position, erneut prüfen. Nach 30 s zum Antippen.
   - `no_position` sonst: sofort zum Antippen.
4. **Antippen:** Der nächste Klick wird Start, die Route ist `[Klick, Ziel]`. Nach Abbruch oder Wahl von „Start selbst wählen“ ignoriert der Ablauf spätere Positionen (eine Lauf-Nummer je Ablauf).

Profil: bei jeder Änderung außer `manual` in `localStorage` unter `outdoor.last-route-profile`. Beim Ablauf `select.value` setzen, ohne `change` auszulösen, nur wenn das Profil verfügbar ist.

In `renderPlan`: Hat ein Teilstück einen Fehler mit Code `routing_unavailable` oder `route_not_found`, zeigt `#route-straight` den passenden Text und „Als gerade Linie verbinden“. Der Knopf setzt dieses Teilstück auf `{profile: "manual", options: {}}` und berechnet neu.

- [ ] **Step 2: Prüfen**

Run: `npm --prefix apps/base/ui run build`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add apps/base/ui
git commit -m "Route to a search result in Base"
```

### Task 7: Base-Selbsttest und Messung

**Files:**
- Modify: `apps/base/ui/src/diagnostics.ts`, `apps/base/src-tauri/src/commands.rs`, `apps/base/src-tauri/src/main.rs`, `docs/MESSPROTOKOLL.md`

**Interfaces:**
- Produces: Umgebungsvariable `OUTDOOR_SELFTEST_SEARCH=<Suchtext>;<lat>,<lon>`, Bericht als JSON wie bei den anderen Selbsttests.

- [ ] **Step 1: Selbsttest schreiben**

Mit echten Mausereignissen auf den Bedienelementen. Der Bericht muss diese Werte enthalten und alle müssen `true` sein:

```ts
report.found;                     // Treffer erscheint
report.category_word_found;       // "Hostel" zeigt eine Unterkunft ohne das Wort im Namen
report.new_route_without_prompt;  // ohne offene Route keine Auswahl
report.choice_warns_when_unsaved;
report.append_keeps_waypoints;    // Reihenfolge und alte Teilstücke unverändert, auch bei einem Punkt
report.no_position_asks_for_tap;
report.wait_shows_choose_start;   // simulierte Quelle "connecting"
report.late_fix_ignored_after_choose;
report.late_fix_ignored_after_cancel;
report.replay_age_from_receipt;   // Fix mit altem time_ms gilt als frisch
report.straight_offered_on_failed_leg;
report.straight_makes_manual_leg;
```

- [ ] **Step 2: Selbsttest laufen lassen, isoliert**

Run:

```bash
T=$(mktemp -d) && OUTDOOR_DATA_DIR="$T" OUTDOOR_DATA="$T/data" OUTDOOR_SELFTEST_SEARCH="Hostel;-13.5170,-71.9785" cargo run -p zerogrid-base
```

Testregion vorher nur per Kopie aus `data/template-test/cusco-urlaub` in den Testordner legen.
Expected: Alle Berichtswerte `true`.

- [ ] **Step 3: Antwortzeit messen**

Mit dem größten Paket aus `data/template-test` (`alpenraum-ost-urlaub`) die Suche nach `Hostel`, `Hauptstraße` und `Ma` messen und in `docs/MESSPROTOKOLL.md` eintragen. Ziel: unter 300 ms. Wird das Ziel verfehlt, anhalten und dem Nutzer berichten, nicht still ändern.

- [ ] **Step 4: Commit und Meilenstein zeigen**

```bash
git add apps/base docs/MESSPROTOKOLL.md
git commit -m "Test search and routing to a result through the real Base controls"
```

**Meilenstein 2: anhalten. Der Nutzer probiert Base von Hand aus. Freigabe für Trail abwarten.**

---

## Meilenstein 3: Trail

### Task 8: Suchleiste in Trail

**Files:**
- Create: `apps/trail/ZeroGridTrail/Search/SearchController.swift`, `apps/trail/ZeroGridTrail/Search/SearchBar.swift`, `apps/trail/ZeroGridTrail/Search/ResultCard.swift`
- Modify: `apps/trail/ZeroGridTrail/Map/MapScreen.swift`, `apps/trail/ZeroGridTrail/Map/MapView.swift`, `apps/trail/ZeroGridTrail/ZeroGridTrailApp.swift`
- Test: `apps/trail/ZeroGridTrailTests/SearchControllerTests.swift`

**Interfaces:**
- Consumes: `CoreApp.search`, `CoreApp.searchCategory`.
- Produces:

```swift
@MainActor final class SearchController: ObservableObject {
    @Published var text: String
    @Published private(set) var results: SearchResults?
    @Published private(set) var selected: SearchHit?
    @Published var open: Bool
    func chooseCategory(_ key: String)
    func select(_ hit: SearchHit)
    func close()
}
extension MapController { func showSearchPin(_ at: CLLocationCoordinate2D?); var mapCentre: CLLocationCoordinate2D { get } }
```

- [ ] **Step 1: Test schreiben**

```swift
func testOlderAnswerDoesNotReplaceNewer()
func testShortTextDoesNotSearch()
func testUnnamedPoiShowsCategory()
```

- [ ] **Step 2: Oberfläche bauen**

`SearchBar` steht im `VStack` von `MapScreen` über `MapToolbar`, Bezeichner `search-field`. Beim Öffnen liegt der Bereich von oben über der Karte, die unten teilweise sichtbar bleibt. Kategorien, Trefferliste und Hinweise wie in Base. Suche auf einer eigenen seriellen Queue, 200 ms verzögert. `ResultCard` zeigt Name, Art, Ort, Entfernung und „Route hierher“ (`route-here`).

- [ ] **Step 3: Commit**

```bash
git add apps/trail
git commit -m "Search places on the Trail map"
```

Der Nutzer baut auf dem Mac: `apps/trail/scripts/build-core.sh`, dann Xcode-Tests.

### Task 9: „Route hierher“ in Trail

**Files:**
- Create: `apps/trail/ZeroGridTrail/Search/RouteHereFlow.swift`, `apps/trail/ZeroGridTrail/Search/RouteHereViews.swift`
- Modify: `apps/trail/ZeroGridTrail/Plan/PlanController.swift`, `apps/trail/ZeroGridTrail/Plan/PlanSheet.swift`, `apps/trail/ZeroGridTrail/Map/MapView.swift`, `apps/trail/ZeroGridTrail/Map/MapScreen.swift`
- Test: `apps/trail/ZeroGridTrailTests/RouteHereFlowTests.swift`

**Interfaces:**
- Consumes: `routeStart`, `routeIsUnsaved`, `planSetLeg`, `legCanConnectStraight`.
- Produces:

```swift
enum RouteHereStep: Equatable { case idle, choosing(unsaved: Bool), waiting, tapStart(otherMap: Bool) }
@MainActor final class RouteHereFlow: ObservableObject {
    @Published private(set) var step: RouteHereStep
    func begin(target: CLLocationCoordinate2D)
    func chooseNewRoute()
    func chooseAppend()
    func chooseStartMyself()
    func cancel()
    func received(_ location: CLLocation)
    func tapped(_ at: CLLocationCoordinate2D) -> Bool   // true: Tipp wurde verbraucht
}
extension PlanController {
    func startRoute(from: CLLocationCoordinate2D, to: CLLocationCoordinate2D)
    func appendTarget(_ to: CLLocationCoordinate2D)
    func connectStraight(leg: Int)
    var straightLineLeg: Int? { get }
}
extension MapController { func requestPosition(_ onUpdate: @escaping (CLLocation) -> Void, denied: @escaping () -> Void); func stopRequestingPosition() }
```

- [ ] **Step 1: Tests schreiben**

```swift
func testEmptyRouteSkipsChoice()
func testOpenRouteAsks()
func testLateFixIgnoredAfterChooseStart()
func testLateFixIgnoredAfterCancel()
func testThirtySecondsLeadToTapStart()
func testDeniedPermissionGoesStraightToTap()
func testTapOutsideFlowIsNotConsumed()
```

- [ ] **Step 2: Implementieren**

Gleicher Ablauf wie Task 6. Die Position kommt aus einem eigenen `CLLocationManager` mit `startUpdatingLocation`, der am Ende des Ablaufs gestoppt wird, damit er keinen Akku verbraucht. Alter aus `CLLocation.timestamp`. Das Profil liegt in `UserDefaults` unter `lastRouteProfile`, `manual` wird nicht gemerkt. `overlay.onTap` fragt zuerst `RouteHereFlow.tapped`. Während `replacing` tut der Ablauf nichts. Fehlermeldung und Knopf „Als gerade Linie verbinden“ erscheinen in `PlanSheet`.

- [ ] **Step 3: Commit**

```bash
git add apps/trail
git commit -m "Route to a search result in Trail"
```

### Task 10: Handtest und Abschluss

**Files:**
- Modify: `docs/MESSPROTOKOLL.md`

- [ ] **Step 1: Der Nutzer testet auf dem iPhone im Flugmodus**

1. Hostel in Cusco suchen, antippen, „Route hierher“.
2. Mit offener, ungespeicherter Route: Warnung, „Neue Route“, „Anhängen“.
3. GPS sucht noch: „Start selbst wählen“, danach kommt eine Position, nichts springt.
4. Ziel außerhalb der Karte des Standorts.
5. Stück ohne Routing-Daten, „Als gerade Linie verbinden“.
6. Antwortzeit der Suche im Selbsttest-Protokoll ablesen.

- [ ] **Step 2: Ergebnis ins Messprotokoll, Gesamtprüfung**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && npm --prefix apps/base/ui run build`
Expected: PASS.

- [ ] **Step 3: Commit, dann Abschluss**

```bash
git add docs/MESSPROTOKOLL.md
git commit -m "Record search timings on the device"
```

**Meilenstein 3: anhalten.** Danach `superpowers:finishing-a-development-branch`, PR und Push erst nach Rückfrage.
