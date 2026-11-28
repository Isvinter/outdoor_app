# Thermikkarte Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Sky shows the kk7 thermal probability map, switched on and off by a map button, chosen by season and time of day, kept on the device for flights without network.

**Architecture:** The map asks for `tiles://localhost/thermals/<key>/<z>/<x>/<y>`; `maps.rs` hands that to a new `ThermalCache` that answers from disk, else fetches from kk7 (throttled), else falls back to an older tile or another stored layer. Layer choice (season from date, time of day from hours since sunrise) is pure code with tests; sunrise lives in `flight-core`. The web UI adds/removes a raster source with `maxzoom` 10 when the button is pressed.

**Tech Stack:** Rust (Tauri 2 app crate `vario-app`, `flight-core`), `ureq` 3 (already a dependency), MapLibre GL JS 5, plain HTML/JS.

**Spec:** `docs/superpowers/specs/2026-11-28-thermikkarte-design.md`

## Global Constraints

- Code, comments, commit messages in English; visible texts in German. User is a Rust beginner: explain non-obvious Rust concepts briefly in comments where they appear (traits as objects, `Condvar`, `Arc`).
- Work on branch `feature/thermal-map` (exists, holds the spec commit).
- Before every commit, all must pass (no `| tail` hiding exit codes):
  `cargo fmt --all -- --check` · `cargo clippy --workspace --exclude vario-app --exclude tauri-plugin-vario-sensors --all-targets -- -D warnings` · `cargo test --workspace --exclude vario-app --exclude tauri-plugin-vario-sensors` · `cargo test -p vario-app`. Also `cargo clippy -p vario-app --all-targets -- -D warnings`.
- `flight-core`: no system clock, no OS/UI dependencies.
- No test and no smoke run may send a request to kk7 unless `VARIO_SMOKE_KK7=1` is set.
- Not a single request to kk7 while the thermal map is off.
- Never fetch tiles the user does not look at (no neighbours, no other zooms, no other layers).
- kk7 tile URL: `https://thermal.kk7.ch/tiles/thermals_<key>/<z>/<x>/<y_tms>.png?src=ZeroGrid`, `y_tms = 2^z − 1 − y`. User-Agent `ZeroGrid/<app version> (Sky)`. Nothing else identifying.
- At most 10 requests per second to kk7.
- Keys: season `all|jan|apr|jul|oct`, time of day `all|04|07|10`; key = `<season>_<time>`.
- Stored tiles are never deleted automatically; only „Thermik-Speicher leeren“ clears them. Freshness limit 30 days.
- Attribution text: `Thermik © thermal.kk7.ch (CC BY-NC-SA 4.0)`.
- Source `maxzoom` 10, raster opacity 0.6.
- The public repo gets no home coordinates; test coordinates are Munich/Brauneck/Cusco/Tromsø only.

## Review Focus

1. **Flight without network, tile never fetched for any layer** → 204, map shows nothing, no stutter; the next tile must not wait for a network timeout again (offline backoff). Test in Task 3: `offline_backoff_skips_network`.
2. **Slow network (fetch hangs)** → other region tiles keep loading; thermal fetches end after the timeout. Test in Task 3: `fetch_runs_outside_cache_lock` (a slow fetch for tile A does not block a cached answer for tile B); region tiles use a separate code path in `maps::respond`.
3. **Malformed path / unknown key / z > 12** (`thermals/../../etc/1/2/3`, `thermals/jul_99/1/1/1`) → 400, never a file outside the cache folder. Test in Task 3: `rejects_bad_keys_and_paths`.
4. **Toggling off while tiles are in flight** → no further requests reach the handler after the source is removed. Smoke test in Task 6 counts handler calls before and after.
5. **Southern hemisphere / far west longitude at late UTC** (Cusco 02:00 UTC = 21:00 local the day before) → solar date is the local day, not the UTC day. Test in Task 1: `solar_date_follows_longitude`, Task 2: `cusco_evening_picks_evening`.

---

### Task 1: Sunrise in `flight-core`

**Files:**
- Create: `crates/flight-core/src/sun.rs`
- Modify: `crates/flight-core/src/lib.rs` (add `pub mod sun;`), `crates/flight-core/src/time.rs` (add `Date::days_since_unix_epoch`)

**Interfaces:**
- Produces:
  - `impl Date { pub fn days_since_unix_epoch(self) -> i64 }` (wraps the existing private `days_from_civil`)
  - `pub enum Sunrise { At { unix_seconds: i64 }, AlwaysUp, AlwaysDown }` (derive Debug, Clone, Copy, PartialEq)
  - `pub fn sunrise(date: Date, latitude_deg: f64, longitude_deg: f64) -> Sunrise`
  - `pub fn solar_date(unix_seconds: i64, longitude_deg: f64) -> Date` — the date at local solar time: `UtcDateTime::from_unix_seconds(unix_seconds + (longitude_deg * 240.0).round() as i64).date`

- [ ] **Step 1: Write the failing tests** in `sun.rs`:

```rust
fn utc_minutes(s: Sunrise) -> f64 { match s { Sunrise::At { unix_seconds } => (unix_seconds.rem_euclid(86_400)) as f64 / 60.0, other => panic!("{other:?}") } }

#[test] fn munich_midsummer() { // published 05:13 CEST
    let m = utc_minutes(sunrise(Date::new(2026, 6, 21).unwrap(), 48.137, 11.575));
    assert!((m - (3.0 * 60.0 + 13.0)).abs() <= 3.0, "{m}");
}
#[test] fn munich_midwinter() { // published 08:02 CET
    let m = utc_minutes(sunrise(Date::new(2026, 12, 21).unwrap(), 48.137, 11.575));
    assert!((m - (7.0 * 60.0 + 2.0)).abs() <= 3.0, "{m}");
}
#[test] fn cusco_june() { // 06:09 local (UTC−5)
    let m = utc_minutes(sunrise(Date::new(2026, 6, 21).unwrap(), -13.53, -71.97));
    assert!((m - (11.0 * 60.0 + 9.0)).abs() <= 3.0, "{m}");
}
#[test] fn sunrise_lies_on_the_given_date() {
    let date = Date::new(2026, 6, 21).unwrap();
    let Sunrise::At { unix_seconds } = sunrise(date, -13.53, -71.97) else { panic!() };
    assert_eq!(unix_seconds.div_euclid(86_400), date.days_since_unix_epoch());
}
#[test] fn polar_day_and_night() {
    assert_eq!(sunrise(Date::new(2026, 6, 21).unwrap(), 69.65, 18.96), Sunrise::AlwaysUp);
    assert_eq!(sunrise(Date::new(2026, 12, 21).unwrap(), 69.65, 18.96), Sunrise::AlwaysDown);
}
#[test] fn solar_date_follows_longitude() {
    // 2026-06-22 02:00 UTC is still the evening of 21 June in Cusco.
    let t = Date::new(2026, 6, 22).unwrap().days_since_unix_epoch() * 86_400 + 2 * 3600;
    assert_eq!(solar_date(t, -71.97), Date::new(2026, 6, 21).unwrap());
    assert_eq!(solar_date(t, 11.5), Date::new(2026, 6, 22).unwrap());
}
```

- [ ] **Step 2: Run** `cargo test -p flight-core sun` — expected: compile errors (`sun` missing).
- [ ] **Step 3: Implement.** NOAA "general solar position" approximation: fractional year γ from day of year, equation of time and declination from the NOAA series, hour angle for zenith 90.833°; `cos(ha) > 1` → `AlwaysDown`, `< −1` → `AlwaysUp`; sunrise minutes UTC = `720 − 4·(lon + ha_deg) − eqtime`, added to `date.days_since_unix_epoch() * 86400`. Doc comment names the source and ±2 min accuracy.
- [ ] **Step 4: Run** `cargo test -p flight-core` — all pass.
- [ ] **Step 5: Commit** `Compute sunrise for the thermal map`.

### Task 2: Layer choice and thermal settings

**Files:**
- Create: `apps/sky/src-tauri/src/thermal_layer.rs`
- Modify: `apps/sky/src-tauri/src/lib.rs` (`mod thermal_layer;`)

**Interfaces:**
- Consumes: `flight_core::sun::{sunrise, solar_date, Sunrise}`, `flight_core::time::UtcDateTime`
- Produces:
  - `pub enum Season { All, Jan, Apr, Jul, Oct }` and `pub enum DayTime { All, Morning, Midday, Evening }` (Copy, Eq, serde `rename_all = "lowercase"`; wire names `all|jan|apr|jul|oct` and `all|morning|midday|evening`), each with `fn code(self) -> &'static str` (`all|jan|…` and `all|04|07|10`).
  - `pub enum Choice<T> { Auto, Fixed(T) }` — serialized as the plain string: `"auto"` or the value's wire name (custom `Serialize`/`Deserialize` or two concrete enums `SeasonChoice`/`DayTimeChoice` with an `Auto` variant — builder's choice, the JSON must be `{"enabled":false,"season":"auto","dayTime":"auto"}`).
  - `pub struct ThermalSettings { pub enabled: bool, pub season: …, pub day_time: … }` — serde `camelCase`, `default` (off, auto, auto); `parse(text: &str) -> Self` (anything unreadable → default).
  - `pub struct LayerKey { pub season: Season, pub day_time: DayTime }` with `fn as_string(self) -> String` (`"jul_07"`) and `fn parse(s: &str) -> Option<LayerKey>` (only the 20 valid keys).
  - `pub fn auto_season(month: u8) -> Season` — 12,1,2→Jan; 3–5→Apr; 6–8→Jul; 9–11→Oct.
  - `pub fn auto_day_time(hours_since_sunrise: Option<f64>) -> DayTime` — `None` (polar) → All; `< 5.5` (incl. negative) → Morning; `< 8.5` → Midday; else Evening.
  - `pub fn choose(settings: &ThermalSettings, unix_now: i64, latitude_deg: f64, longitude_deg: f64) -> LayerKey` — season from `solar_date(...)`'s month, day time from `sunrise(solar_date, lat, lon)`: `At` → `(now − at)/3600`, `AlwaysUp`/`AlwaysDown` → `None`. Fixed choices win over auto.
  - `pub fn fallbacks(key: LayerKey) -> Vec<LayerKey>` — order from the spec, without `key` itself and without duplicates.
  - `pub fn load(app: &AppHandle) -> ThermalSettings` / `pub fn save(app: &AppHandle, settings: &ThermalSettings) -> Result<(), String>` — file `thermal-map.json` via `crate::settings::{config_file, write_file}` (same pattern as `maps::load_look`).

- [ ] **Step 1: Write the failing tests:**

```rust
#[test] fn seasons_by_month() { use Season::*;
    let got: Vec<_> = (1..=12).map(auto_season).collect();
    assert_eq!(got, [Jan, Jan, Apr, Apr, Apr, Jul, Jul, Jul, Oct, Oct, Oct, Jan]); }
#[test] fn day_time_bounds() { use DayTime::*;
    assert_eq!(auto_day_time(Some(-1.0)), Morning); assert_eq!(auto_day_time(Some(5.49)), Morning);
    assert_eq!(auto_day_time(Some(5.5)), Midday);   assert_eq!(auto_day_time(Some(8.49)), Midday);
    assert_eq!(auto_day_time(Some(8.5)), Evening);  assert_eq!(auto_day_time(None), All); }
#[test] fn keys_round_trip_and_reject_unknown() {
    let k = LayerKey { season: Season::Jul, day_time: DayTime::Midday };
    assert_eq!(k.as_string(), "jul_07"); assert_eq!(LayerKey::parse("jul_07"), Some(k));
    for bad in ["jul_99", "may_07", "jul", "../x", "jul_07/../.."] { assert_eq!(LayerKey::parse(bad), None); } }
#[test] fn fallback_order() {
    let names = |k: &str| fallbacks(LayerKey::parse(k).unwrap()).iter().map(|k| k.as_string()).collect::<Vec<_>>();
    assert_eq!(names("oct_04"), ["oct_07", "oct_10", "oct_all", "all_all"]);
    assert_eq!(names("oct_10"), ["oct_07", "oct_04", "oct_all", "all_all"]);
    assert_eq!(names("oct_07"), ["oct_04", "oct_10", "oct_all", "all_all"]);
    assert_eq!(names("oct_all"), ["oct_07", "oct_04", "oct_10", "all_all"]);
    assert_eq!(names("all_all"), ["all_07", "all_04", "all_10"]); }
#[test] fn brauneck_midday_in_october() { // 2026-10-01 12:00 UTC, sunrise ≈ 05:11 UTC → 6.8 h
    let t = flight_core::time::Date::new(2026, 10, 1).unwrap().days_since_unix_epoch() * 86_400 + 12 * 3600;
    assert_eq!(choose(&ThermalSettings::default(), t, 47.68, 11.53).as_string(), "oct_07"); }
#[test] fn cusco_evening_picks_evening() { // 2026-06-22 00:30 UTC = 19:30 local on 21 June
    let t = flight_core::time::Date::new(2026, 6, 22).unwrap().days_since_unix_epoch() * 86_400 + 1800;
    assert_eq!(choose(&ThermalSettings::default(), t, -13.53, -71.97).as_string(), "jul_10"); }
#[test] fn fixed_choice_wins() {
    let s = ThermalSettings::parse(r#"{"enabled":true,"season":"apr","dayTime":"morning"}"#);
    assert!(s.enabled); assert_eq!(choose(&s, 0, 47.68, 11.53).as_string(), "apr_04"); }
#[test] fn settings_default_and_garbage() {
    assert_eq!(ThermalSettings::parse("nonsense"), ThermalSettings::default());
    assert_eq!(serde_json::to_string(&ThermalSettings::default()).unwrap(), r#"{"enabled":false,"season":"auto","dayTime":"auto"}"#); }
```

- [ ] **Step 2: Run** `cargo test -p vario-app thermal_layer` — fails to compile.
- [ ] **Step 3: Implement** the interfaces above.
- [ ] **Step 4: Run** `cargo test -p vario-app` — all pass.
- [ ] **Step 5: Commit** `Choose the thermal layer by season and time of day`.

### Task 3: Thermal tile cache

**Files:**
- Create: `apps/sky/src-tauri/src/thermal_cache.rs`
- Modify: `apps/sky/src-tauri/src/lib.rs` (`mod thermal_cache;`)

**Interfaces:**
- Consumes: `thermal_layer::{LayerKey, fallbacks}`
- Produces:
  - `pub enum FetchError { Offline, Http(u16), Other(String) }` — `Offline` = connection/DNS/timeout failure.
  - `pub trait Fetch: Send + Sync { fn get(&self, url: &str) -> Result<Vec<u8>, FetchError>; }`
  - `pub fn kk7_url(key: LayerKey, z: u8, x: u32, y: u32) -> String` — exact format from Global Constraints.
  - `pub struct ServedTile { pub data: Vec<u8>, pub fallback: bool }`
  - `pub enum Answer { Tile(ServedTile), Empty, BadRequest }`
  - `pub struct ThermalCache` with `pub fn new(dir: PathBuf, fetch: Box<dyn Fetch>) -> Self`, `pub fn answer(&self, path: &str, now: SystemTime) -> Answer` (`path` is the part after `/thermals/`, e.g. `jul_07/10/544/358`), `pub fn size_bytes(&self) -> u64`, `pub fn clear(&self) -> Result<(), String>`, and counters `pub fn requests(&self) -> u64` (calls of `answer`) and `pub fn fetches(&self) -> u64` (calls of `Fetch::get`) for the smoke test.
- Behaviour (values are this plan's calls):
  - Path: exactly `<key>/<z>/<x>/<y>`, key via `LayerKey::parse`, `z ≤ 12`, `x, y < 2^z`; anything else → `BadRequest`.
  - File `<dir>/<key>/<z>/<x>/<y>.png`; written via temp file + rename. Age = `now − mtime`.
  - Order: fresh (≤ 30 days) → serve; else fetch → store → serve; fetch failed → stale file of same key; else first stored tile among `fallbacks(key)` (any age) with `fallback: true`; else `Empty`.
  - Throttle: sliding window, at most 10 fetch starts in any 1 s; `struct RateLimiter` with `fn delay(&mut self, now: Instant) -> Duration` (pure, testable), the caller sleeps that long.
  - Offline backoff: after `FetchError::Offline`, skip the network for all tiles for 30 s. After `Http(_)` for one tile, skip that tile for 10 min.
  - Dedupe: concurrent `answer` calls for the same tile perform one fetch; the others wait (`Mutex<HashSet<PathBuf>>` + `Condvar`) and then read the file. No lock is held while fetching, so other tiles are served meanwhile.

- [ ] **Step 1: Write the failing tests** with a fake fetcher (`struct Fake { calls: AtomicU64, reply: Mutex<Result<Vec<u8>, FetchError>>, delay: Duration }`) and `tempfile`-free temp dirs (`std::env::temp_dir().join(unique name)`; check whether the crate already has a temp-dir helper in tests and reuse it):
  - `kk7_url_flips_y`: `kk7_url(jul_07, 10, 544, 358)` == `"https://thermal.kk7.ch/tiles/thermals_jul_07/10/544/665.png?src=ZeroGrid"`.
  - `fresh_tile_is_served_without_fetch`: file written, mtime now → `Tile{fallback:false}`, `fetches()==0`.
  - `missing_tile_is_fetched_and_stored`: reply `Ok(b"png")` → served, file exists, `fetches()==1`; second call `fetches()==1`.
  - `old_tile_is_refreshed_online`: file with mtime 31 days before `now` (use `now = SystemTime::now() + 31 days` instead of touching mtime) → fetch happens, new bytes served.
  - `old_tile_is_kept_offline`: same, reply `Err(Offline)` → old bytes, `fallback:false`.
  - `fallback_layer_offline`: only `oct_10/…` stored, ask `oct_07/…`, `Err(Offline)` → `oct_10` bytes with `fallback:true`.
  - `nothing_stored_offline_is_empty`: → `Empty`.
  - `offline_backoff_skips_network`: first missing tile `Err(Offline)` → second different missing tile within 30 s → `fetches()==1`; with `now` + 31 s → `fetches()==2`. (Backoff uses `Instant`; make the clock injectable in `ThermalCache::with_clock` for tests, or store the deadline as `SystemTime` derived from `now` — builder's choice, the test must not sleep 30 s.)
  - `empty_kk7_tile_is_stored`: reply 68-byte body → stored and served like any tile.
  - `rejects_bad_keys_and_paths`: `"../x/1/1/1"`, `"jul_99/1/1/1"`, `"jul_07/13/0/0"`, `"jul_07/2/4/0"`, `"jul_07/1/1"` → `BadRequest`.
  - `rate_limiter_allows_ten_per_second`: 10 × `delay(t0)` == 0, 11th > 0 and ≤ 1 s; at `t0 + 1 s` == 0.
  - `same_tile_is_fetched_once`: 4 threads, fake delay 200 ms → `fetches()==1`, all 4 get `Tile`.
  - `fetch_runs_outside_cache_lock`: tile A fetching with 500 ms delay, cached tile B answered in < 100 ms.
  - `clear_removes_everything`: after `clear()`, `size_bytes()==0` and next answer fetches.
- [ ] **Step 2: Run** `cargo test -p vario-app thermal_cache` — fails to compile.
- [ ] **Step 3: Implement** per the interfaces; explain `Condvar` and `Box<dyn Fetch>` briefly in comments.
- [ ] **Step 4: Run** `cargo test -p vario-app` — all pass, no test takes > 2 s.
- [ ] **Step 5: Commit** `Keep thermal tiles on the device`.

### Task 4: Serve thermal tiles and add the commands

**Files:**
- Modify: `apps/sky/src-tauri/src/maps.rs` (route in `respond`), `apps/sky/src-tauri/src/lib.rs` (state, commands, handler list), `apps/sky/src-tauri/src/thermal_cache.rs` (add `UreqFetch`)

**Interfaces:**
- Consumes: Tasks 2 and 3.
- Produces (Tauri commands, JS names in camelCase args):
  - `get_thermal_map() -> ThermalSettings`, `set_thermal_map(settings: ThermalSettings) -> Result<(), String>`
  - `thermal_tiles(latitude: f64, longitude: f64) -> ThermalTiles` where `#[serde(rename_all="camelCase")] struct ThermalTiles { key: String, url: String }`, `url` = `format!("{TILES_BASE}thermals/{key}/{{z}}/{{x}}/{{y}}")` (respect `maps::TILES_BASE` for Windows/Android); uses `SystemTime::now()` as unix seconds.
  - `thermal_cache_size() -> u64`, `clear_thermal_cache() -> Result<(), String>`
  - `pub struct UreqFetch` implementing `Fetch`: agent with connect timeout 3 s, global timeout 8 s, user agent `format!("ZeroGrid/{} (Sky)", env!("CARGO_PKG_VERSION"))`; connection/timeout errors → `Offline`, status ≥ 400 → `Http(code)`; body limit 1 MB.
  - Managed state: `ThermalCache` in `Arc`, dir `app.path().app_data_dir()?.join("thermals")` (not `data_dir()`, which on iOS is the Files-visible Documents folder; not the cache dir, which iOS may purge). The cache is created lazily on first request (the `AppHandle` is needed for the dir).
  - Smoke mode: when `VARIO_SMOKE_TEST` is set and `VARIO_SMOKE_KK7` is not, the fetcher is a `NoNetwork` fetch that always returns `Offline` (keep `NoNetwork` in `thermal_cache.rs`).
- In `maps::respond`: before `TilePath` parsing, `if let Some(rest) = path.strip_prefix("/thermals/")` → `ThermalCache::answer`; `Tile` → 200 `image/png` with header `Cache-Control: no-store`; `Empty` → 204; `BadRequest` → 400. Regional tiles must not go through this branch.

- [ ] **Step 1: Write the failing test** in `maps.rs` or `lib.rs` tests if routing is extracted: `thermal_tiles_url_template` checks `url` ends with `thermals/jul_07/{z}/{x}/{y}` for a fixed key (extract a pure `fn thermal_url(key: &str) -> String` for this).
- [ ] **Step 2: Run** `cargo test -p vario-app` — fails.
- [ ] **Step 3: Implement** the route, `UreqFetch`, `NoNetwork`, the commands, register them in `generate_handler!`.
- [ ] **Step 4: Run** all checks from Global Constraints, then `VARIO_SMOKE_TEST=1 cargo run -p vario-app` still reports no script errors.
- [ ] **Step 5: Commit** `Serve thermal tiles through the tiles scheme`.

### Task 5: Map button, layer and settings in the UI

**Files:**
- Modify: `apps/sky/ui/map.js`, `apps/sky/ui/app.js`, `apps/sky/ui/index.html`, `apps/sky/ui/style.css`

**Interfaces:**
- Consumes: commands from Task 4.
- Produces (`FlightMap` API, used by `app.js` and the smoke test):
  - `setThermal(url | null)` — `null` removes layer and source; a URL adds them (or `setTiles([url])` if the source exists and the URL changed). Remembered across style changes: `addOwnLayers()` re-adds it.
  - `thermalShown() -> boolean` — source present.
- Map layer: source id `thermals`, `{ type: "raster", tiles: [url], tileSize: 256, maxzoom: 10, attribution: "Thermik © thermal.kk7.ch (CC BY-NC-SA 4.0)" }`; layer id `thermals`, `raster-opacity` 0.6, `raster-fade-duration` 0, inserted `beforeId` = first `symbol` layer of the style, else `airspace-fill`; so it lies under labels, airspaces, track and landing cross.
- Button: `<button id="thermal" class="map-overlay">Thermik</button>` next to `#follow`; class `on` when enabled. Click → toggles `enabled`, saves via `set_thermal_map`, then `refreshThermal()`.
- `refreshThermal()` in `app.js`: if disabled → `FlightMap.setThermal(null)`; else `invoke("thermal_tiles", { latitude, longitude })` with the last GPS fix, else the map centre → `FlightMap.setThermal(url)`. Called at start, on toggle, on settings change and every 60 s (only does work while enabled).
- Settings section „Thermikkarte“ in `#page-settings` (after the airspace section): two `<select>`s.
  - Jahreszeit: `auto` Automatisch · `all` Ganzes Jahr · `jan` Winter (Mitte Nov. – Mitte März) · `apr` Frühling (Mitte Feb. – Mitte Juni) · `jul` Sommer (Mitte Mai – Mitte Sept.) · `oct` Herbst (Mitte Aug. – Mitte Dez.)
  - Tageszeit: `auto` Automatisch · `all` Ganzer Tag · `morning` Morgens (bis 7 h nach Sonnenaufgang) · `midday` Mittags (5–11 h nach Sonnenaufgang) · `evening` Abends (ab 7 h nach Sonnenaufgang)
  - Line „Gespeichert: <n> MB“ (`thermal_cache_size`, one decimal) and button „Thermik-Speicher leeren“ (asks `confirm()` first).
  - Hint text (exact): „Wahrscheinlichkeit von Thermik aus vergangenen Streckenflügen, keine Vorhersage. Geladen wird nur, was du bei eingeschalteter Thermikkarte ansiehst; es bleibt auf dem Gerät. Zur Vorbereitung Tageszeit auf „Mittags“ stellen und das Fluggebiet bei Zoom 10 oder näher ansehen, danach wieder „Automatisch“. Ohne Netz zeigt Sky, was gespeichert ist, notfalls eine andere Tageszeit. Daten © thermal.kk7.ch (M. von Känel), Lizenz CC BY-NC-SA 4.0.“

- [ ] **Step 1: Implement** map.js API, button, settings section, CSS for the button (same look as `#follow`).
- [ ] **Step 2: Check by hand** on the desktop with network (one short manual look, not in a test): `cargo run -p vario-app`, press „Thermik“ over the Brauneck, colours appear, attribution shows; press again, attribution and colours gone; switch dark mode with the map on → still there.
- [ ] **Step 3: Run** `VARIO_SMOKE_TEST=1 cargo run -p vario-app` — no script errors.
- [ ] **Step 4: Commit** `Show the thermal map with a button on the map`.

### Task 6: Smoke test and docs

**Files:**
- Modify: `apps/sky/src-tauri/src/smoke.rs`, `apps/sky/src-tauri/src/lib.rs` (command `smoke_thermal_counts() -> { requests: u64, fetches: u64 }`, registered like the other `smoke_*` commands), `docs/DATENQUELLEN.md`, `docs/ALGORITHMEN.md`, `apps/sky/CLAUDE.md`

- [ ] **Step 1: Extend the smoke test** (always, not behind an extra variable): on the map page press `#thermal` with real pointer/mouse events at the button's centre (same event sequence as the existing map tap, `pointerdown/mousedown/pointerup/mouseup/click`); wait 2 s; report `thermalShown()`, the attribution text contains `kk7`, and counts; press again; wait 2 s; record counts; wait 2 s more; counts unchanged and `thermalShown()==false`. Print one line `thermal report: {...}`. With `NoNetwork`, `fetches` stays 0 unless `VARIO_SMOKE_KK7=1`. Leave the setting as it was before (off).
- [ ] **Step 2: Run** `VARIO_SMOKE_TEST=1 cargo run -p vario-app` — report shows shown=true, then false, unchanged counts after off, fetches 0. Run once with `VARIO_SMOKE_KK7=1` and a temp HOME: fetches > 0 and ≤ about 30.
- [ ] **Step 3: Docs.**
  - `docs/DATENQUELLEN.md`: row for the thermal map (source, licence, attribution, „nur Anzeige auf dem Gerät, keine Weitergabe, kein Vorabladen“).
  - `docs/ALGORITHMEN.md`: new section „Thermikkarte: Layer-Wahl“ — season table, hours since sunrise thresholds 5,5/8,5 h, solar date, sunrise formula and accuracy, fallback order, 30-day freshness, backoff values.
  - `apps/sky/CLAUDE.md`: phase-table row „Thermikkarte“ (status: built, tested with unit tests and desktop smoke test; not checked on iPhone or in flight), and a paragraph in the notes: cache folder, `VARIO_SMOKE_KK7`, kk7 rules.
- [ ] **Step 4: Run** all checks from Global Constraints.
- [ ] **Step 5: Commit** `Check the thermal map in the smoke test and document it`.

### Afterwards (user, not part of the tasks)

Mac and iPhone: look at the Brauneck area at zoom 10 with „Mittags“, flight mode, zoom 13–15 → map present; choose „Abends“ in flight mode → fallback appears. Then push and open a PR (larger work → feature branch + PR, merge `--no-ff`), only after the user's okay.
