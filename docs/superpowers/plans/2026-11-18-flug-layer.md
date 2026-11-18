# Flug-Layer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The region builder writes a vector layer `flying` (power lines, aerialways, free flying sites, tall obstacles) into `outdoor.pmtiles`, for the paragliding app Vario.

**Architecture:** The existing Planetiler profile `scripts/tiles/OutdoorOverlay.java` gets a fifth layer in the same pass over the OSM data. A small Rust tool `tools/tile-inspect` decodes tiles of a PMTiles archive to JSON so that a shell test can check the profile against a handwritten OSM fixture, and so that the rebuilt test region can be compared before/after.

**Tech Stack:** Planetiler 0.10.2 (Java single-file profile), osmium (Docker image `outdoor-osmium`), Rust 2024 (`core-maps` PMTiles reader, `clap`, `serde_json`), `jq`.

**Spec:** `docs/superpowers/specs/2026-11-18-flug-layer-design.md`

## Global Constraints

- New layer name: `flying`, inside `outdoor.pmtiles`; no new build step, no manifest or `core-maps` change.
- Layers `trail`, `route`, `piste`, `poi` must stay byte-for-byte equivalent in content (same feature counts per kind).
- Kinds, OSM tags, attributes and min zooms exactly as in the spec table:
  - `power_line` (`power=line`, z10), `power_minor_line` (`power=minor_line`, z12): attrs `voltage` (highest value, in kV, integer), `name`.
  - aerialways: `kind` = OSM `aerialway` value, all except `pylon` and `station`; attr `name`; z11, `goods` and `zip_line` z12.
  - sites `takeoff`, `landing`, `toplanding`, `training` (points; areas as centroid), z9: attrs `name`, `ele`, `paragliding`, `hanggliding`, `direction` (raw text), `winch`.
  - obstacles `mast`, `tower`, `chimney`, `wind_turbine` (points), z12: attrs `height` (m, integer), `name`.
- Code, comments, commit messages in English; talk to the user in German.
- Do not push; the user decides after the milestone.

## Review Focus

1. Mixed point/line layer through `FeatureMerge.mergeLineStrings` — points must survive post-processing (test: fixture site and obstacle points present at z14).
2. Multi-valued and unit-carrying tags: `voltage=110000;20000` → 110, `voltage=20000` → 20, `voltage=medium` → no attribute; `height=45 m` → 45 (test: fixture lines/points with these values).
3. Underground power (`power=line` + `location=underground`, `power=cable`) must not appear — a pilot would avoid a field for nothing (test: fixture way absent).
4. `free_flying:site=takeoff;landing` on one node yields both a `takeoff` and a `landing` point (test: fixture node).
5. A site mapped as a closed way (area) appears as one point, not a polygon (test: geometry `point`, count 1).

---

### Task 1: `tile-inspect` tool

**Files:**
- Create: `tools/tile-inspect/Cargo.toml`, `tools/tile-inspect/src/main.rs`, `tools/tile-inspect/src/mvt.rs`
- Test: `tools/tile-inspect/tests/mvt.rs`

**Interfaces:**
- Produces CLI: `tile-inspect <archive.pmtiles> --zoom <z> --bbox <W,S,E,N> [--layer <name>] [--summary]`
  - default: one JSON object per line per feature per tile: `{"layer":"flying","geometry":"point"|"line"|"polygon","props":{...}}` — props with strings, integers, floats, booleans as JSON types.
  - `--summary`: one line per (layer, kind) sorted: `{"layer":"trail","kind":"path","count":123}`; `kind` = prop `kind`, else `class`, else `null`.
- Produces in `mvt.rs`: `pub fn decode(tile: &[u8]) -> Vec<Layer>` with `pub struct Layer { pub name: String, pub features: Vec<Feature> }`, `pub struct Feature { pub geometry: Geometry, pub props: serde_json::Map<String, serde_json::Value> }`, `pub enum Geometry { Point, Line, Polygon, Unknown }`. Geometry coordinates are not decoded (not needed).
- Consumes: `core_maps::pmtiles::PmTiles::open(&Path)` and `.tile(z, x, y)` (returns decompressed bytes).

- [ ] **Step 1: Write failing tests** in `tests/mvt.rs`: a test-local helper encodes a one-layer tile (layer `flying`, keys `kind`,`voltage`,`winch`,`ratio`; values string/sint/bool/double; one point feature `type=1`, one line feature `type=2`). Assertions: `decode` returns one layer named `flying`; feature 0 `geometry == Point`, `props["kind"] == "takeoff"`, `props["winch"] == true`; feature 1 `geometry == Line`, `props["voltage"] == 110`, `props["ratio"] == 0.5`. Second test: `decode(&[])` is empty.
- [ ] **Step 2: Run** `cargo test -p tile-inspect` — expect compile failure (crate missing).
- [ ] **Step 3: Implement** crate (workspace member via `tools/*`; deps `core-maps`, `clap`, `serde_json`, `thiserror`; `[lints] workspace = true`). Decoder covers MVT value fields 1 string, 2 float, 3 double, 4 int, 5 uint, 6 sint, 7 bool. `main.rs`: bbox → tile range at zoom (Web Mercator, y from north), iterate tiles, skip missing.
- [ ] **Step 4: Run** `cargo test -p tile-inspect && cargo clippy -p tile-inspect --all-targets -- -D warnings` — PASS. Smoke: `cargo run -q -p tile-inspect -- data/regions/isarwinkel-achensee/outdoor.pmtiles --zoom 14 --bbox 11.50,47.65,11.56,47.68 --summary` prints lines for `trail`/`poi`.
- [ ] **Step 5: Save baseline** of the current test region (needed in Task 3, before anything is rebuilt): summary at z12 and z14 for the region bbox `11.35,47.35,11.95,47.80` into `data/build/isarwinkel-achensee/outdoor-before-z12.jsonl` / `-z14.jsonl`.
- [ ] **Step 6: Commit** `tools/tile-inspect` + `Cargo.lock`: "tile-inspect: print features of PMTiles vector tiles".

### Task 2: Layer `flying` in the overlay profile

**Files:**
- Modify: `scripts/tiles/OutdoorOverlay.java` (layer doc comment, `processFeature`, `postProcessLayerFeatures`, `description()`)
- Create: `fixtures/osm/flying.osm` (OSM XML), `scripts/tiles/test-outdoor-overlay.sh`
- Modify: `fixtures/README.md` (one line for `osm/flying.osm`)

**Interfaces:**
- Consumes: `tile-inspect` CLI from Task 1.
- Produces: layer `flying` as in Global Constraints.

- [ ] **Step 1: Write the fixture** `fixtures/osm/flying.osm`, all objects within bbox `11.000,47.000,11.010,47.010`, ways ≥ 300 m long so they survive at z10–z12:
  - way `power=line voltage=110000;20000 name=Test 110`
  - way `power=minor_line voltage=20000`
  - way `power=minor_line voltage=medium`
  - way `power=line location=underground voltage=110000` (must be absent)
  - way `power=cable voltage=20000` (must be absent)
  - ways `aerialway=goods`, `aerialway=chair_lift name=Testlift`, `aerialway=zip_line`; node `aerialway=pylon` (absent); node `aerialway=station` (absent)
  - node `sport=free_flying free_flying:site=takeoff name=Startplatz Ost ele=1500 free_flying:paragliding=yes free_flying:hanggliding=no direction=E-SE`
  - node `sport=free_flying free_flying:site=takeoff;landing`
  - closed way (area) `sport=free_flying free_flying:site=landing name=Landewiese`
  - node `free_flying=takeoff` (old scheme) and node `sport=paragliding` (old scheme, no site → `takeoff`)
  - node `sport=free_flying free_flying:site=training free_flying:winch=yes`
  - nodes `man_made=mast height=45 m`, `man_made=tower height=30`, `man_made=chimney`, `power=generator generator:source=wind height=140`, `man_made=wind_turbine`
  - node `natural=peak ele=1600 name=Testgipfel` (control: still in `poi`)
- [ ] **Step 2: Write the test** `scripts/tiles/test-outdoor-overlay.sh` (bash, `set -euo pipefail`, work dir from `mktemp -d` under `data/build/test-outdoor-overlay`): convert XML → PBF with `docker run outdoor-osmium cat` (same volume/user pattern as `build-region.sh`), run `java -cp data/tools/planetiler.jar scripts/tiles/OutdoorOverlay.java --osm-path=… --bounds=11.000,47.000,11.010,47.010 --maxzoom=14 --output=…`, then `tile-inspect --layer flying` at z14 (and z9, z11, z12 for zoom checks) and assert with `jq` (feature counts after de-duplicating by props, since z14 objects may span tiles):
  - z14: `power_line` with `voltage==110`, `name=="Test 110"`; `power_minor_line` with `voltage==20`; one `power_minor_line` without `voltage`; no feature with voltage from the underground/cable ways (exactly 1 `power_line`, 2 `power_minor_line`).
  - z14: kinds `goods`, `chair_lift` (`name=="Testlift"`), `zip_line` present; no `pylon`, `station`.
  - z14: `takeoff` "Startplatz Ost" with `ele==1500`, `paragliding==true`, `hanggliding==false`, `direction=="E-SE"`; total `takeoff` count 4 (Ost, combined node, `free_flying=takeoff`, `sport=paragliding`); `landing` count 2, "Landewiese" has `geometry=="point"`; `training` with `winch==true`.
  - z14: `mast` `height==45`, `tower` `height==30`, `chimney` without `height`, `wind_turbine` count 2, one with `height==140`.
  - z9: only sites present (no power, aerialway, obstacles). z11: `power_line` and `chair_lift` present, no `power_minor_line`, `goods`, `zip_line`, `mast`. z12: all line kinds present.
  - `poi` layer at z14 still has `peak` "Testgipfel".
  - Prints `ok` and exits 0; on failure prints the failing check and exits 1.
- [ ] **Step 3: Run** `scripts/tiles/test-outdoor-overlay.sh` — expect FAIL (no `flying` layer).
- [ ] **Step 4: Implement** in `OutdoorOverlay.java`: `flying` lines via `features.line("flying")`, points via `features.point` / `features.centroid` for areas; reuse `leadingInt` for `ele`/`height`; voltage = max of `;`-separated `leadingInt`s ÷ 1000 (values < 1000 are already kV); booleans only set when tag is `yes`/`no`; old schemes: `free_flying=<site>`, `sport=paragliding|hang_gliding` without site → `takeoff`; `free_flying:site` split on `;`, one point per known value. Keep `postProcessLayerFeatures` merging lines for `flying` — verify points pass through (Review Focus 1); if not, merge only line features of `flying`.
- [ ] **Step 5: Run** the test again — `ok`.
- [ ] **Step 6: Commit** profile, fixture, test, README: "region builder: flying layer with power lines, aerialways, flying sites and obstacles".

### Task 3: Rebuild the test region and document

**Files:**
- Modify: `scripts/build-region.sh` (header comment line for `outdoor.pmtiles`), `PLAN.md` (§4.1: one paragraph on layer `flying` and that Vario uses it)

- [ ] **Step 1: Rebuild** `scripts/build-region.sh isarwinkel-achensee --steps osm,outdoor,manifest --force` (osm step clips the cached `oberbayern-latest.osm.pbf` of 2026-11-13, the same data the old overlay was built from).
- [ ] **Step 2: Compare** summary at z12 and z14 (same bbox as Task 1 Step 5) with the baseline, filtered to layers other than `flying`: `diff` empty. Expected: PASS.
- [ ] **Step 3: Inspect the Brauneck** (bbox `11.50,47.64,11.56,47.68`, z14, `--layer flying`): list sites, aerialways and power lines; also check basemap `transportation` with `class=="aerialway"` subclasses there to record whether OpenMapTiles carries `goods`. Note results for the user.
- [ ] **Step 4: Run workspace checks** `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- [ ] **Step 5: Update docs** (build-region.sh comment, PLAN.md) and commit: "docs: flying layer; test region rebuilt".
- [ ] **Step 6: Stop** and show the user the Brauneck findings; ask once whether to push.
