# OLED Dark Mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Trail and Sky can switch between light and an OLED-black map and UI, with switchable relief.

**Architecture:** `core-maps` recolors the existing light styles at runtime from one explicit palette, so no dark style files exist. `core-ffi` passes the choice to Trail, and Sky calls `core-maps` directly. Relief changes live on the map without reloading the style.

**Tech Stack:** Rust 2024, `serde_json`, MapLibre style JSON, UniFFI, SwiftUI with MapLibre Native (Trail), Tauri with MapLibre GL JS (Sky).

**Spec:** `docs/superpowers/specs/2026-11-23-oled-dark-mode-design.md`

## Global Constraints

- Code, comments and commits in English; visible text in German.
- Before each commit: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
- The repository is public: no home-network addresses, home coordinates or credentials.
- Never push or merge without asking the user.
- Dark background is exactly `#000000`.
- Contrast against black: main labels, instrument values and warnings at least 7:1; trails, water names and secondary labels at least 4.5:1.
- `Light` output stays byte-identical in meaning to today's style. Base is not changed.
- The `slope` raster layer is never recolored.
- First start: light, relief off. No automatic switching.
- Trail cannot be built on Linux. Its task ends with a Mac build and the UI tests, run by the user.

## Review Focus

- Switching light/dark during a running Sky recording: recording, track and airspaces continue (Task 4 smoke test).
- Sky with no installed region or an unreachable style: the fallback map is black in dark mode (Task 4).
- Sky with an online style in dark mode: only the UI turns dark, and the relief switch does nothing and raises no error (Task 4).
- A region without contour tiles: relief works with only the hillshade layer present (Tasks 1, 4, 5).
- Reloading regions in Trail while dark: dark, relief strength, slope opacity and the planned route survive (Task 5).

---

### Task 1: Dark recoloring in `core-maps`

**Files:**
- Create: `crates/core-maps/src/dark.rs`
- Create: `crates/core-maps/tests/dark.rs`
- Modify: `crates/core-maps/src/style.rs`, `crates/core-maps/src/lib.rs`
- Modify: `crates/core-maps/tests/style.rs`, `crates/core-ffi/src/lib.rs`, `apps/sky/src-tauri/src/maps.rs` (add `appearance: Appearance::Light`, so the workspace keeps compiling)

**Interfaces:**
- Produces: `pub enum Appearance { #[default] Light, Dark }` (`Debug, Clone, Copy, PartialEq, Eq, Default`)
- Produces: `StyleOptions.appearance: Appearance`
- Produces: `pub const RELIEF_LAYERS: [&str; 4] = ["hillshade", "contour-minor", "contour-major", "contour-label"]`
- Internal: `pub(crate) fn apply_dark(style: &mut serde_json::Value) -> Result<(), MapsError>`

- [ ] **Step 1: Write failing tests in `crates/core-maps/tests/dark.rs`.** Run them for `styles/topo-summer.json`, `styles/topo-winter.json` and `apps/sky/src-tauri/map-styles/flying.json`, with `bounds: Some(...)` unless stated:

| Test | Assertion |
|---|---|
| `background_is_black` | every `background-color` is `#000000`, also with `bounds: None` |
| `every_color_has_a_dark_value` | `offline_style` returns `Ok` for all three styles |
| `an_unknown_color_is_an_error` | a style with `fill-color: "#123456"` returns `MapsError::Style` naming the color |
| `light_stays_as_it_was` | `Appearance::Light` gives the same parsed JSON as the style filled before this change, checked by comparing every layer's `paint` with the template |
| `slope_is_untouched` | the `slope` layer of winter has the same `paint` in light and dark |
| `relief_starts_hidden` | every layer in `RELIEF_LAYERS` that exists has `layout.visibility == "none"` in dark |
| `relief_without_contours` | with `Contours::Omit`, `hillshade` is hidden and no contour layer exists |
| `hillshade_is_dark` | `hillshade-shadow-color` and `hillshade-accent-color` are `#000000` |
| `no_large_area_stays_bright` | every `fill` color has relative luminance at most 0.03 |
| `labels_are_readable` | every `text-color` has at least 7:1 on black; every `text-halo-color` is black |
| `trails_and_obstacles_are_readable` | every `line-color` of layers with `source-layer` `trail` or `flying` has at least 4.5:1 on black |

Contrast on black is `(L + 0.05) / 0.05`, with `L` the WCAG relative luminance. Check every color inside expressions too.

- [ ] **Step 2: Run and confirm failure.** `cargo test -p core-maps --test dark`. Expected: compile error for `Appearance`.

- [ ] **Step 3: Implement `apply_dark` in `dark.rs`.** Keep a sorted table `light color → dark color` for all 103 colors, keyed by lowercase text without spaces, and a small table for per-layer exceptions keyed by `(layer id, property)`. Walk only properties ending in `-color`, recursively through arrays, and replace strings starting with `#` or `rgb`. Set hillshade colors and relief visibility after the walk. Forests, meadows and settlements become black; water and glaciers a very dark tint. Keep the hue of trail difficulty, piste and obstacle colors.

- [ ] **Step 4: Call it from `offline_style`** right after parsing, before the `bounds == None` return, only for `Appearance::Dark`. Export `Appearance` and `RELIEF_LAYERS`.

- [ ] **Step 5: Run.** `cargo test -p core-maps` and `cargo check --workspace`. Expected: all pass.

- [ ] **Step 6: Commit** with message `Recolor offline map styles for OLED dark mode`.

### Task 2: Dark parameter in `core-ffi`

**Files:**
- Modify: `crates/core-ffi/src/lib.rs:176`, `crates/core-ffi/tests/ffi.rs`, `crates/core-ffi/tests/share.rs`
- Modify: `apps/trail/ZeroGridTrail/Map/MapStyle.swift:26` (pass `dark: false`, so Trail keeps compiling)

**Interfaces:**
- Consumes: `Appearance`
- Produces: `pub fn offline_style(&self, style_json: String, tiles_base: String, dark: bool) -> Result<String, CoreError>`; in Swift `core.offlineStyle(styleJson:tilesBase:dark:)`

- [ ] **Step 1: Add test `style_can_be_dark` to `ffi.rs`:** with an installed region, `dark: true` gives `background-color == "#000000"`, and `dark: false` keeps the template's color.
- [ ] **Step 2: Run.** `cargo test -p core-ffi style_can_be_dark`. Expected: compile error for the argument count.
- [ ] **Step 3: Add the parameter,** map it to `Appearance`, and update the existing test calls with `false`.
- [ ] **Step 4: Run.** `cargo test -p core-ffi`. Expected: pass.
- [ ] **Step 5: Commit** with message `Pass the dark appearance through core-ffi`.

### Task 3: Remembered map look in Sky

**Files:**
- Modify: `apps/sky/src-tauri/src/maps.rs`, `apps/sky/src-tauri/src/lib.rs`

**Interfaces:**
- Produces: `#[serde(rename_all = "camelCase", default)] pub struct MapLook { pub dark: bool, pub relief: bool }` with `Default` = both false
- Produces: `MapLook::parse(text: &str) -> MapLook`, `maps::load_look(&AppHandle) -> MapLook`, `maps::save_look(&AppHandle, MapLook) -> Result<(), String>`, stored as `map-look.json`
- Produces: Tauri commands `get_map_look() -> MapLook` and `set_map_look(look: MapLook) -> Result<(), String>`

- [ ] **Step 1: Add unit tests in `maps.rs`:** `look_defaults_to_light_without_relief` (empty and broken text give the default), `look_survives_saving` (JSON round trip), `missing_field_keeps_default`.
- [ ] **Step 2: Run.** `cargo test -p vario-app look_`. Expected: compile error.
- [ ] **Step 3: Implement,** register both commands, and make `reload` pass `Appearance::Dark` when `load_look(app).dark`.
- [ ] **Step 4: Run.** `cargo test -p vario-app`. Expected: pass.
- [ ] **Step 5: Commit** with message `Remember the map look in Sky`.

### Task 4: Dark UI, map and relief in Sky

**Files:**
- Modify: `apps/sky/ui/index.html:117`, `apps/sky/ui/app.js`, `apps/sky/ui/map.js`, `apps/sky/ui/style.css`, `apps/sky/src-tauri/src/smoke.rs`

**Interfaces:**
- Consumes: `get_map_look`, `set_map_look`, `RELIEF_LAYERS` names
- Produces: `FlightMap.setLook({ dark, relief })`; `document.documentElement.dataset.theme` is `light` or `dark`

- [ ] **Step 1: Extend the smoke test.** With `VARIO_SMOKE_DARK=1`, it sets dark through the real buttons, turns relief on and off, and prints `look report:` as JSON with `theme`, `background` (computed body color), `fallbackBackground`, `reliefVisible` before and after, and `scriptErrors`. Expected report: theme `dark`, black background and fallback, relief `false → true → false`, no script errors, on a style with and without contour layers, and on an online style.
- [ ] **Step 2: Run and confirm failure.** `HOME=$(mktemp -d) VARIO_SMOKE_TEST=1 VARIO_SMOKE_DARK=1 target/debug/vario-app`. Expected: no dark controls found.
- [ ] **Step 3: Add the controls** below `#offline-style`: `#map-look` with buttons `data-look="light"` „Hell“ and `data-look="dark"` „Dunkel“, and `#map-relief` with „Relief aus“ and „Relief an“, visible only in dark.
- [ ] **Step 4: Add the theme** in `style.css`: `[data-theme="dark"]` sets `color-scheme: dark`, `--ink: #fff`, `--paper: #000`, a `--muted` of at least 7:1, a `--line` of at least 4.5:1, and brighter `--climb` and `--sink` of at least 7:1. Replace `#fff` at the tap marker with a variable. The orange warning banner keeps its colors; the alarm stays red with white text.
- [ ] **Step 5: Implement `FlightMap.setLook`.** Dark uses a black `EMPTY_STYLE`, black track casing and halos, and an `AIRSPACE_COLORS_DARK` table with the same hue per category, each at least 4.5:1 on black. Relief sets `visibility` on existing `RELIEF_LAYERS` only, without `setStyle`, and is applied again after every style load.
- [ ] **Step 6: Wire `app.js`.** Load the look in `init` before the first `updateMapStyle`. Changing light/dark saves it, calls `setLook` and `updateMapStyle`. Changing relief saves it and calls `setLook` only. A running recording is not touched.
- [ ] **Step 7: Run.** `node --check apps/sky/ui/app.js apps/sky/ui/map.js`, then the smoke command from Step 2. Expected: the report from Step 1, and exit 0.
- [ ] **Step 8: Commit** with message `Add dark mode and relief switch to Sky`.

### Task 5: Dark UI, map and relief in Trail

**Files:**
- Create: `apps/trail/ZeroGridTrail/Map/PanelBackground.swift`
- Modify: `Map/MapStyle.swift`, `Map/MapView.swift`, `Map/MapToolbar.swift`, `Map/MapScreen.swift`, `Map/LocationButton.swift`, `Plan/RouteLayers.swift`, `Plan/WaypointViews.swift`, `Regions/ScanSheet.swift` (under `apps/trail/ZeroGridTrail/`)
- Test: `apps/trail/ZeroGridTrailUITests/MapUITests.swift`

**Interfaces:**
- Consumes: `core.offlineStyle(styleJson:tilesBase:dark:)`
- Produces: `MapStyle.url(for: MapBase, dark: Bool, core: CoreApp) throws -> URL`, cache file `style-<base>-<light|dark>.json`
- Produces on `MapController`: `@Published private(set) var dark: Bool`, `func setDark(_ dark: Bool)`, `@Published var reliefStrength: Double` (0…1), and for tests `reliefVisibleOnMap: Bool`; stored in `UserDefaults` as `map.dark` and `map.reliefStrength`
- Produces: `View.panelBackground(in: some Shape, dark: Bool) -> some View`
- Accessibility identifiers: `look-light`, `look-dark`, `relief-strength`

- [ ] **Step 1: Add UI tests to `MapUITests`:** `testDarkMapSurvivesReload` (dark, relief 0.7, winter, slope value, route present, then reload regions: all kept), `testReliefSliderHidesAndShows` (0 hides, above 0 shows), `testReliefWithoutContours` (region without contours: no failure), `testLightStaysDefault` (fresh launch: light, relief control hidden).
- [ ] **Step 2: Implement** the look in `MapStyle` and `MapController`. Reapply dark, relief and slope in `didFinishLoading`. At strength 0 hide `RELIEF_LAYERS`; above 0 show them and set hillshade exaggeration and contour opacity live.
- [ ] **Step 3: Add the toolbar controls** from the spec. Apply `.preferredColorScheme` from `controller.dark` at `MapScreen`.
- [ ] **Step 4: Replace `.regularMaterial`** with `panelBackground`: unchanged in light, solid black in dark.
- [ ] **Step 5: Adapt the route and waypoints.** In dark, the route casing is black and the leg colors keep their meaning with at least 4.5:1. Rebuild the route layers when the style loads.
- [ ] **Step 6: Run on Linux.** `swiftc -frontend -parse` for each changed Swift file. Expected: exit 0. This does not replace a real build.
- [ ] **Step 7: Commit** with message `Add dark mode and relief slider to Trail`.
- [ ] **Step 8: User runs on the Mac:** `apps/trail/scripts/build-core.sh` and `apps/trail/scripts/test.sh`. Expected: build succeeds and UI tests pass. Fix any failure before continuing.

### Task 6: Visual check and documentation

**Files:**
- Modify: `README.md`, `apps/sky/CLAUDE.md`, `docs/sky/ios-test.md`

- [ ] **Step 1: Render Sky's dark styles from the real regions** in the Linux desktop build, into the session scratchpad, for Alps and lowland, with relief on and off, plus the winter style filled by `core-maps`. Look at each image and check labels, water, obstacles and slope classes.
- [ ] **Step 2: Report mean pixel brightness** of dark and light for the same views, labeled as an approximation of power use.
- [ ] **Step 3: Document** the switch in the README, add an iPhone checklist to `docs/sky/ios-test.md` from the spec's "Auf dem iPhone" section, and note the status in `apps/sky/CLAUDE.md`.
- [ ] **Step 4: Run all checks** from Global Constraints. Expected: all pass.
- [ ] **Step 5: Commit** with message `Document the OLED dark mode`.
- [ ] **Step 6: Stop and show the result to the user.** Pushing and the pull request need their okay.
