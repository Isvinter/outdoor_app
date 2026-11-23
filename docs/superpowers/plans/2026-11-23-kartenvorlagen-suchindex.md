# Map Templates and Search Index Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The build script makes packages from three templates (`urlaub`, `outdoor`, `fliegen`), every new package carries an offline search file, and Base, Trail and Sky handle all of them.

**Architecture:** `search.sqlite` becomes a normal manifest file in format 1, so download, checksum, resume, share and delete work unchanged. A new crate `core-search` owns schema, normalisation and queries; a new tool `tools/search-index` turns an osmium GeoJSON export into that file. Templates live in the region definition and decide the steps and the files of the manifest.

**Tech Stack:** Rust 2024, `rusqlite` (bundled, FTS5), bash and inline Python in `scripts/build-region.sh`, osmium in the existing Docker image, SwiftUI (Trail), TypeScript (Base UI).

**Spec:** `docs/superpowers/specs/2026-11-23-kartenvorlagen-suchindex-design.md`

## Global Constraints

- Branch `feature/map-templates-search`. Do not push; the user is asked once after the milestone.
- `FORMAT` stays 1. The only new accepted file name is exactly `search.sqlite`.
- Nothing at runtime may use the network, an account or an online service.
- Never modify, rebuild or delete the installed `alpenraum-ost` or anything else under `/mnt/data_ssd` except new files of this work. The user's other SSD data stays untouched.
- Workspace lints forbid `unsafe`, `unwrap`, `expect`, `panic`, `todo` outside tests. Tests start with `#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]`.
- CI commands must pass: `cargo fmt --all --check`, `cargo clippy --workspace --exclude vario-app --exclude tauri-plugin-vario-sensors --all-targets -- -D warnings`, `cargo test --workspace --exclude vario-app --exclude tauri-plugin-vario-sensors`. Sky (`vario-app`) is iOS-only in CI; check it with `cargo test -p vario-app` where it builds on this machine and otherwise say it was not run.
- Size limits: `alpenraum-ost-urlaub` at most 2.6 GB, its `search.sqlite` at most 100 MB. If exceeded, stop and talk to the user.
- Commit messages: imperative English sentence, ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. A user names a Base region "Kroatien Urlaub": its id must not end in a template suffix, or later template builds could overwrite it (Task 4 test `define_avoids_template_suffixes`).
2. Search input with FTS syntax (`"`, `*`, `AND`, `Bar & Grill`, only punctuation) returns results or nothing, never an error (Task 2 test `query_survives_search_syntax`).
3. Opening or querying `search.sqlite` must not change its bytes, or the checksum and sharing break (Task 2 test `reading_leaves_the_file_unchanged`).
4. Peruvian and other non-German names: `Cañete` is found by `canete`, `Ñuñoa` by `nunoa` (Task 2 test `spanish_names_are_found_without_accents`).
5. A package directory left from another template: the manifest must list only this template's files (Task 4 test `manifest_lists_only_the_files_of_the_template`).

---

### Task 1: Package format accepts search and templates

**Files:**
- Modify: `crates/core-maps/src/manifest.rs`, `crates/core-maps/src/lib.rs`, `crates/core-app/src/share.rs`
- Modify (struct literals): `crates/core-share/tests/server.rs`, `apps/sky/src-tauri/src/share.rs`
- Test: `crates/core-maps/tests/store.rs`, `crates/core-share/tests/server.rs`

**Interfaces:**
- Produces in `core_maps`: `pub const SEARCH_FILE: &str = "search.sqlite";`, `pub struct SearchInfo { pub format: u32, pub places: u64, pub streets: u64, pub pois: u64, pub named_pois: u64 }` (Serialize, Deserialize, Debug, Clone, PartialEq).
- `RegionManifest` gains `template: Option<String>`, `group: Option<String>`, `search: Option<SearchInfo>`. `CatalogEntry` gains `template: Option<String>`, `group: Option<String>`. All `#[serde(default, skip_serializing_if = "Option::is_none")]`.

- [ ] **Step 1: Write failing tests in `crates/core-maps/tests/store.rs`**

`manifest_accepts_the_search_file` (fixture manifest with an added `search.sqlite` entry parses); `manifest_rejects_other_sqlite_files` (`other.sqlite` and `search.sqlite.bak` give `InvalidManifest`); `old_manifest_has_no_template` (`template`, `group`, `search` are `None`, and serialising adds none of those keys); `package_with_search_file_installs_shares_and_deletes` (local package with a real `search.sqlite` entry: `install_local` succeeds, `package_file(id, "search.sqlite")` returns it, `is_complete` is true, `delete` removes it). In `crates/core-share/tests/server.rs`, `search_file_is_served_with_range` requests `/<id>/search.sqlite` with `Range: bytes=1-` and expects 206.

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p core-maps -p core-share search`
Expected: FAIL on the rejected file name and the missing fields.

- [ ] **Step 3: Implement**

Add the constant, struct and fields. In `RegionManifest::parse`, accept `file.name == SEARCH_FILE` next to `ROUTING_FILE`. In `core-app/src/share.rs`, pass `template` and `group` from manifest to `CatalogEntry`. Fill the new fields with `None` in the existing literals.

- [ ] **Step 4: Run the workspace tests**

Run: `cargo test --workspace --exclude vario-app --exclude tauri-plugin-vario-sensors`, then `cargo check -p vario-app --tests`
Expected: PASS.

- [ ] **Step 5: Commit** with message `Accept search files and templates in region packages`.

### Task 2: `core-search` library

**Files:**
- Create: `crates/core-search/Cargo.toml`, `src/lib.rs`, `src/normalize.rs`, `src/write.rs`, `src/read.rs`
- Modify: root `Cargo.toml` (add `core-search` to workspace dependencies)
- Test: `crates/core-search/tests/search.rs`

**Interfaces:**
- Consumes: `rusqlite`, `thiserror`.
- Produces:

```rust
pub const SEARCH_FORMAT: u32 = 1;
pub enum Kind { Place, Street, Poi }
pub enum Category { Fuel, Groceries, Food, Pharmacy, Lodging, Camping, Parking, Retail, Sights }
pub struct Entry { pub kind: Kind, pub category: Option<Category>, pub subtype: Option<String>,
                   pub name: String, pub alt_names: Vec<String>, pub town: Option<String>,
                   pub lat: f64, pub lon: f64 }
pub struct Hit { pub kind: Kind, pub category: Option<Category>, pub subtype: Option<String>,
                 pub name: String, pub town: Option<String>, pub lat: f64, pub lon: f64,
                 pub distance_m: Option<f64> }
pub fn normalize(text: &str) -> String;          // query form
pub fn index_terms(name: &str) -> Vec<String>;   // all indexed forms
pub struct SearchWriter;  // create(path, osm_timestamp: Option<&str>) -> Result<Self, SearchError>;
                          // add(&mut self, &Entry) -> Result<(), SearchError>;
                          // finish(self) -> Result<(), SearchError>;
pub struct SearchIndex;   // open(path) -> Result<Self, SearchError>;
                          // search(&self, text: &str, near: Option<(f64, f64)>, limit: usize) -> Result<Vec<Hit>, SearchError>;
                          // by_category(&self, Category, near: (f64, f64), limit: usize) -> Result<Vec<Hit>, SearchError>;
```

`Kind` and `Category` derive Debug, Clone, Copy, PartialEq, Eq. `near` is `(lat, lon)`.

- [ ] **Step 1: Write failing tests**

```rust
#[test] fn fts5_is_available() // creates an fts5 table through SearchWriter without error
#[test] fn umlauts_and_sharp_s() {
    assert_eq!(normalize("Straße"), "strasse");
    assert_eq!(normalize("München"), "munchen");
    assert!(index_terms("München").contains(&"muenchen".to_string()));
    assert!(index_terms("München").contains(&"munchen".to_string()));
    assert!(index_terms("Hauptstr.").contains(&"hauptstrasse".to_string()));
}
```

With a small written index: `name_prefix_is_found` (`hos` finds `Hostel Alpina`); `street_shows_its_town` (`Hauptstraße` hit has `town == Some("Kufstein")`); `street_abbreviation_is_found` (`Hauptstr` and `Hauptstraße` find both spellings); `alternative_name_is_found` (`Bolzano` and `Bozen` find one entry whose `name` is `Bozen`); `spanish_names_are_found_without_accents`; `nearer_hits_come_first` (two `Hauptstraße`, `near` the second, second is first and `distance_m` is set); `category_finds_unnamed_places` (`by_category(Parking, …)` returns an entry with empty name, nearest first); `query_survives_search_syntax` (inputs `"`, `*`, `a AND`, `NEAR(`, `Bar & Grill`, `...`, empty: all `Ok`; `Bar & Grill` finds `Bar Grill`); `reading_leaves_the_file_unchanged` (bytes equal after open and queries, no `-wal`, `-shm` or `-journal` file beside it); `missing_or_foreign_format_is_an_error` (no file, and `meta` with format 2).

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p core-search`
Expected: FAIL, crate has no such items.

- [ ] **Step 3: Implement**

`normalize`: lower-case, `ß`→`ss`, NFKD without combining marks (use the crate `unicode-normalization`), other punctuation becomes a space. `index_terms`: tokens of `normalize`, plus the `ae/oe/ue` form of a token with an umlaut, plus `…strasse` for a token ending in `str`. Store terms in a contentless or external-content FTS5 column with `tokenize='unicode61 remove_diacritics 0'`. Quote every query token and add `*` for prefix matching, so user text is never FTS syntax. `meta` holds `format` and `osm_timestamp`. Open read-only with URI `file:…?immutable=1`. `search` takes at most 2000 FTS candidates, then sorts by distance when `near` is given and otherwise by rank. Add an index on `(category, lat)` for `by_category`, widening a latitude band until `limit` hits are inside it or the data is exhausted. `finish` runs FTS `optimize` and `VACUUM`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p core-search`
Expected: PASS.

- [ ] **Step 5: Commit** with message `Add the offline search index library`.

### Task 3: `search-index` tool

**Files:**
- Create: `tools/search-index/Cargo.toml`, `src/main.rs`, `src/lib.rs`, `src/classify.rs`, `src/streets.rs`, `src/towns.rs`
- Test: `tools/search-index/tests/run.rs`, fixture `tools/search-index/tests/fixtures/mini.geojsonseq`

**Interfaces:**
- Consumes: `SearchWriter`, `Entry`, `Kind`, `Category`, `SEARCH_FORMAT`; `core_maps::SearchInfo`.
- Produces: `search_index::build(input: &Path, output: &Path, osm_timestamp: Option<&str>) -> Result<SearchInfo, BuildError>`, and the CLI `search-index <input.geojsonseq> <output.sqlite> --info <search.json> [--osm-timestamp T]`. `search.json` is `SearchInfo` as JSON.

- [ ] **Step 1: Write failing tests**

The fixture holds RS-prefixed GeoJSON lines with ids (`n1`, `w2`, `r3`). Tests:
`categories_follow_the_design` (one feature per row of the POI table in the spec maps to its category; `shop=bakery` is `Retail` with subtype `shop=bakery`; `shop=supermarket` is `Groceries`); `private_parking_is_left_out`; `unnamed_poi_stays`; `same_feature_counts_once` (same id as linestring and polygon); `street_segments_merge` (three nearby ways `Hauptstraße` are one street, a fourth 30 km away is another); `unnamed_streets_are_left_out`; `towns_are_assigned` (a street near `place=town` Kufstein gets that town; one with no place in range gets `None`); `alternative_names_are_kept` (`name:de`, `name:en`, `int_name`, `alt_name`, `short_name`; `alt_name` split at `;`); `info_counts_are_right`; `broken_line_is_an_error` (error names the line number, no output file remains).

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p search-index`
Expected: FAIL.

- [ ] **Step 3: Implement**

Use the tag lists of the spec exactly. Position is the mean of a feature's outer coordinates. Merge streets by name and distance with a grid of about 2 km. Assign towns by nearest `place` within: city 15 km, town 8 km, suburb/village 4 km, quarter/neighbourhood/hamlet 2 km, preferring a `place` within its radius by smallest distance; use a coarse grid. Stream the input; hold only what merging needs. Write to `<output>.tmp` and rename on success.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p search-index`
Expected: PASS.

- [ ] **Step 5: Commit** with message `Add the tool that builds search files from OSM`.

### Task 4: Templates in the definition and the build script

**Files:**
- Modify: `crates/region-builder/src/definition.rs`, `crates/region-builder/src/lib.rs`, `scripts/build-region.sh`, `apps/base/ui/src/regions.ts`, `apps/base/ui/src/i18n/de.ts`
- Test: `crates/region-builder/tests/definition.rs`, `crates/region-builder/tests/builder.rs`, Create `crates/region-builder/tests/script.rs`

**Interfaces:**
- Consumes: CLI of Task 3.
- Produces: `pub enum Template { Urlaub, Outdoor, Fliegen }` (serde lower-case, `Default = Outdoor`), `RegionDefinition.template` with `#[serde(default, skip_serializing_if = "Template::is_outdoor")]`, and the script option `--print-plan`, which prints `steps=<comma list>`, `files=<comma list>`, `group=<id>` and exits before checking tools.

Rules for the script: a definition **with an explicit** `template` must match its id (`urlaub` ends in `-urlaub`, `fliegen` in `-fliegen`, `outdoor` in neither). A definition without the field is `outdoor` and passes, so old definitions keep working. If `OUT/manifest.json` exists with another template, abort before the first step.

- [ ] **Step 1: Write failing tests**

In `definition.rs`: `template_defaults_to_outdoor_and_is_not_written`; `urlaub_round_trips`; `unknown_template_is_rejected`. In `builder.rs`: `define_avoids_template_suffixes` (`define("Kroatien Urlaub", …)` gives id `kroatien-urlaub-region`, `Fliegen` likewise). In `script.rs` (runs the real script with a temp `--config`):

```rust
#[test] fn urlaub_plan() // steps=osm,basemap,search,routing,manifest
                         // files=basemap.pmtiles,search.sqlite,routing.tar, group=alps
#[test] fn outdoor_plan() // steps=osm,basemap,outdoor,dem,slope,contours,search,routing,manifest
#[test] fn fliegen_plan() // steps=osm,basemap,outdoor,dem,contours,search,manifest; no routing.tar
```

Also `old_definition_is_outdoor`; `id_and_template_must_match` (nonzero exit, nothing created in the package dir); `unknown_template_stops_the_build`; `other_template_in_the_directory_stops_the_build`; `manifest_lists_only_the_files_of_the_template` (package dir with `basemap.pmtiles`, `search.sqlite`, `routing.tar`, `routing.json`, `search.json` and a stale `dem.pmtiles`, run `--steps manifest` for an `urlaub` id: `dem.pmtiles` is not listed, `template`, `group` and `search` are set, and `catalog.json` carries `template` and `group`).

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p region-builder`
Expected: FAIL.

- [ ] **Step 3: Implement the definition and `define`**

If the slug ends in `-urlaub` or `-fliegen`, append `-region` before `unique_id`.

- [ ] **Step 4: Implement the script**

Read `template` with a default, derive default steps unless `--steps` was given, check the rules, handle `--print-plan` before the tool check. Skip the tool check and `ensure_tools` when the steps are only `manifest`. Add `search` to `ALL_STEPS` after `contours`:

`step_search`: runs if `$OUT/search.sqlite` is stale or `--force`/`--refresh-osm` was given. It uses `osmium tags-filter` on `$REGION_PBF` with the POI, street and `place` filters of the spec, `osmium export --add-unique-id=type_id -f geojsonseq --geometry-types=point,linestring,polygon` into `$BUILD`, then `cargo run --quiet --release -p search-index` with `--info "$OUT/search.json"`, moves the result to `$OUT/search.sqlite`, and deletes the intermediates. The OSM timestamp comes from `osmium fileinfo -g header.option.osmosis_replication_timestamp` when present.

In `step_manifest`, list only the template's files, set `template`, `group`, `search` (from `search.json`, else from the previous manifest as with `routing`), and copy `template` and `group` into `catalog.json`. For a package without a `search.sqlite`, omit `search`, so older outdoor packages can still get a new manifest.

- [ ] **Step 5: Add Base labels**

`BUILD_STEPS` in `regions.ts` gets `contours` and `search`; `de.ts` gets `"maps.build.step.contours": "Höhenlinien"` and `"maps.build.step.search": "Suchindex"`.

- [ ] **Step 6: Run tests and the UI build**

Run: `cargo test -p region-builder`, `pnpm build` in `apps/base/ui`
Expected: PASS.

- [ ] **Step 7: Commit** with message `Build region packages from templates with a search file`.

### Task 5: Apps handle packages without terrain

**Files:**
- Modify: `apps/trail/ZeroGridTrail/Plan/PlanSheet.swift`
- Test: `crates/core-maps/tests/elevation.rs`, `crates/core-app/tests/app.rs` or the plan tests, `crates/flight-core/src/airspace_warning.rs`, `apps/trail/ZeroGridTrailTests/` (only if a view-free check fits)

**Interfaces:**
- Consumes: `plan.elevation`, `total.ascentM` in Trail.
- Produces: `PlanResult`-based helper `hasElevation` in `PlanSheet.swift`: true if any sample has `ele != nil`.

- [ ] **Step 1: Write failing or pinning tests**

`region_without_dem_has_no_elevation` (a package of basemap only: `elevation` is `Ok(None)`, and `tile(Layer::Dem, …)` is `Ok(None)`); a core-app test `route_without_dem_has_no_ascent` (a computed leg over a package without DEM has `ascent_m == None` and a time); in `airspace_warning.rs`, next to `uses_the_ground_for_agl_limits`, `unknown_ground_keeps_the_worst_case_for_agl` (with ground `None`, a fix that would be clear over known high ground still warns for that AGL limit).

- [ ] **Step 2: Run them**

Run: `cargo test -p core-maps -p core-app -p flight-core`
Expected: PASS for pinned behaviour. If the airspace test fails, stop: the spec forbids a weaker warning, and the fix goes into `flight-core` in this task.

- [ ] **Step 3: Change Trail**

In `PlanSheet.details`, show `ProfileChart` only if `hasElevation`; otherwise show `Text("Keine Höhendaten für diese Route.")` in footnote style with accessibility identifier `plan-no-elevation`.

- [ ] **Step 4: Check Trail on the Mac**

Run on the Mac: the existing Trail test scheme. If no Mac is reachable from this machine, record that it was not run and include it in the user's manual test in Task 6.

- [ ] **Step 5: Commit** with message `Show routes without elevation data cleanly`.

### Task 6: Real builds, measurements and handover

**Files:**
- Create: `regions/alpenraum-ost-urlaub.json`, `regions/cusco-urlaub.json`
- Modify: `docs/MESSPROTOKOLL.md`

- [ ] **Step 1: Build the holiday package**

Copy `name`, `bbox`, `osm`, zoom levels and `attribution` from `/mnt/data_ssd/Coding/zerogrid/app-data/de.zeibig.zerogrid.base/region-defs/alpenraum-ost.json` (read only) into `regions/alpenraum-ost-urlaub.json`, with id `alpenraum-ost-urlaub` and `"template": "urlaub"`. Its ten OSM extracts are already cached in `data/sources/osm`.
Run: `scripts/build-region.sh alpenraum-ost-urlaub`
Expected: package at most 2.6 GB, `search.sqlite` at most 100 MB, the installed `alpenraum-ost` directory unchanged (compare its `manifest.json` checksum before and after).

- [ ] **Step 2: Check the search on real data**

With a small example or test binary of `core-search`, query: an existing hostel by name, a street with its town, `Bozen`, `Bolzano`, and the nearest pharmacy near Innsbruck. Record results and the query time.

- [ ] **Step 3: Build Cusco**

Create `regions/cusco-urlaub.json` with bbox `-72.10,-13.65,-71.80,-13.40`, the Geofabrik Peru extract and `"template": "urlaub"`. Ask the user before this first download of the Peru extract only if it is above 1 GB. Build and record POI, street and place counts.

- [ ] **Step 4: Record measurements**

Add a section `## Kartenvorlagen und Suchindex` to `docs/MESSPROTOKOLL.md`: sizes per file and template, index size, build time of the `search` step, counts for both regions, and the query results.

- [ ] **Step 5: Run the full checks and commit**

Run all CI commands of the Global Constraints.
Expected: PASS. Commit with message `Measure the holiday template and the search index`.

- [ ] **Step 6: Stop and show the milestone to the user**

Report what was built and measured. Ask the user to test by hand: install `alpenraum-ost-urlaub` from Base on the iPhone in Trail and in Sky, show the map, compute a car route, confirm the "Keine Höhendaten" note, delete it. Do not push until the user agrees.
