# Gleitzahl, Landepunkt und Wind – Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ZeroGrid Sky shows the glide ratio over ground, the point where the current glide meets the terrain, and the wind from circling with 1–3 stars.

**Architecture:** Three pure modules in `flight-core` (`glide`, `landing`, `wind`) take timestamped values and know nothing about threads or UI. `vario-engine`'s `Processor` feeds them on every GPS fix and puts the results into `Status`; the terrain search runs on a small worker thread owned by the engine loop, so cold DEM tiles never stall tone, recording or airspace warnings. The web UI only draws (layout B: GZ line in the top-left readout, wind box under "Folgen", cross on the map, edge hint when off-screen).

**Tech Stack:** Rust (workspace crates), Tauri 2, plain JS + MapLibre GL JS 5.

**Spec:** `docs/superpowers/specs/2026-11-25-gleitzahl-landepunkt-wind-design.md`

## Global Constraints

- Code, comments, commit messages in English; visible UI texts in German. The user is a Rust beginner: explain non-obvious Rust concepts briefly in comments where they first appear (e.g. `Condvar`, `Arc<dyn Trait>`, `Send + Sync`).
- `flight-core`: no UI/OS/audio dependencies, no system clock; time comes as `f64` seconds from the samples.
- Units SI inside (m, m/s, s, degrees); km/h only in the UI.
- No new external dependencies.
- Glide window choices exactly **10, 20, 30, 60 s**, default **20 s**.
- Gates: mean ground speed over the window ≥ **2.0 m/s (≈7 km/h)**; height above terrain ≥ **30 m** when terrain is known.
- Wind direction internally "from" (aviation); UI arrow points where the wind blows **to**; speed in km/h.
- Show nothing rather than something wrong: circling, climbing, no GPS, unknown terrain → hidden.
- Before every commit (exit status must not be hidden by pipes):
  ```
  cargo fmt --all -- --check
  cargo clippy --workspace --exclude vario-app --exclude tauri-plugin-vario-sensors --all-targets -- -D warnings
  cargo test --workspace --exclude vario-app --exclude tauri-plugin-vario-sensors
  cargo test -p vario-app
  ```
- Commit messages end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- State plainly what was only checked in the desktop simulator; nothing here is verified in real flight.

## Review Focus

1. **Headwind stronger than the glider's speed through the air** (user's Brauneck case taken further): the track over ground goes backwards. GZ and landing point must follow the displacement over ground, so the cross lies behind the pilot. Test in Task 3 and Task 4.
2. **GPS gap mid-window** (fix missing for > 3 s, e.g. phone in pocket, tunnel of a cable car): no GZ computed across the gap. Test in Task 3.
3. **Course wrapping 359° → 1°** in circle detection and turn summing: no false circle, no missed circle. Test in Task 5.
4. **Glide line leaves the installed regions** before hitting ground: no point at the region edge. Test in Task 4.
5. **Glide window changed in the settings mid-flight**: new window applies immediately from the kept history, no stale value from the old window. Test in Task 6.

---

### Task 1: Wind in the synthetic flights

**Files:**
- Modify: `crates/sensor-sim/src/synthetic.rs` (Segment field, builder, `Leg::at`, `state_at`, tests)
- Modify: `apps/sky/src-tauri/src/sim.rs` (`long_flight`)

**Interfaces:**
- Produces: `Segment::airspeed_mps: f64` (renamed from `ground_speed_mps`; speed through the air); `SyntheticFlightBuilder::wind(self, from_deg: f64, speed_mps: f64) -> Self` (default calm). `TruthState::ground_speed_mps` / `course_deg` become the vector sum of air velocity and wind.

- [ ] **Step 1: Write the failing tests** in `synthetic.rs` `mod tests`:

```rust
#[test]
fn wind_drifts_a_full_circle_downwind() {
    // 4 m/s from the west: after one 20 s circle the glider is 80 m further east.
    let flight = SyntheticFlight::builder()
        .wind(270.0, 4.0)
        .segment(Segment::circling(20.0, 0.0, 20.0))
        .build();
    let start = flight.state_at(0.0);
    let end = flight.state_at(20.0);
    let east = distance_m(start.latitude_deg, start.longitude_deg, start.latitude_deg, end.longitude_deg);
    assert!((east - 80.0).abs() < 0.5, "{east}");
    assert!((distance_m(start.latitude_deg, 0.0, end.latitude_deg, 0.0)).abs() < 0.5);
}

#[test]
fn ground_speed_is_air_speed_plus_wind_along_the_course() {
    let flight = SyntheticFlight::builder()
        .heading(90.0) // flying east, downwind
        .wind(270.0, 4.0)
        .segment(Segment::straight(10.0, -1.0))
        .build();
    let state = flight.state_at(5.0);
    assert!((state.ground_speed_mps - 14.0).abs() < 1e-9);
    assert!((state.course_deg - 90.0).abs() < 1e-9);
}

#[test]
fn headwind_stronger_than_airspeed_flies_backwards() {
    let flight = SyntheticFlight::builder()
        .heading(0.0)
        .wind(0.0, 12.0) // from the north, faster than the 10 m/s trim speed
        .segment(Segment::straight(10.0, -1.0))
        .build();
    let state = flight.state_at(5.0);
    assert!((state.ground_speed_mps - 2.0).abs() < 1e-9);
    assert!((state.course_deg - 180.0).abs() < 1e-9);
}
```

- [ ] **Step 2:** Run `cargo test -p sensor-sim` → FAIL (no method `wind`).
- [ ] **Step 3:** Implement. Wind vector (towards) = `(-cos(from), -sin(from)) * speed` as (north, east). Position adds `wind * t` (time since flight start, so `LegStart` keeps continuity automatically if added in `state_at` from absolute time rather than per leg). Ground velocity = air velocity along `heading_rad` + wind; `course_deg = atan2(east, north)` in 0..360; `ground_speed_mps = hypot`. Rename the field everywhere it is used (`sources.rs` uses `TruthState`, not `Segment`, so only `synthetic.rs`). Existing tests must still pass unchanged (calm default).
- [ ] **Step 4:** In `apps/sky/src-tauri/src/sim.rs` `long_flight`, add `.wind(270.0, 4.0)` (≈14 km/h from the west) and say so in its doc comment.
- [ ] **Step 5:** Run the checks from Global Constraints → all pass.
- [ ] **Step 6: Commit** `Add wind to synthetic flights`.

---

### Task 2: Glide window setting

**Files:**
- Modify: `crates/flight-core/src/settings.rs`
- Modify: `apps/sky/src-tauri/src/settings.rs` (`SettingsDto` + both `From`)
- Modify: `apps/sky/ui/index.html` (settings page), `apps/sky/ui/app.js` (settings code near `settingInputs`)

**Interfaces:**
- Produces: `VarioSettings::glide_window_s: f64` (default `20.0`), `pub const GLIDE_WINDOWS_S: [f64; 4] = [10.0, 20.0, 30.0, 60.0];` in `settings.rs`, `SettingsError::GlideWindowInvalid`, file key `glide_window_s`. JS field `glideWindowS`.

- [ ] **Step 1: Write the failing tests:** extend `text_round_trip_keeps_all_values` with `glide_window_s: 60.0`; add

```rust
#[test]
fn glide_window_must_be_one_of_the_choices() {
    assert_eq!(VarioSettings::default().glide_window_s, 20.0);
    let odd = VarioSettings { glide_window_s: 15.0, ..VarioSettings::default() };
    assert_eq!(odd.validate(), Err(SettingsError::GlideWindowInvalid));
    // Older files without the entry keep the default.
    assert_eq!(VarioSettings::from_text("volume = 0.3").unwrap().glide_window_s, 20.0);
}
```

- [ ] **Step 2:** `cargo test -p flight-core settings` → FAIL.
- [ ] **Step 3:** Implement field, constant, validation (`GLIDE_WINDOWS_S.contains(&value)`), text key; error text `"the glide window must be 10, 20, 30 or 60 seconds"`. Add `glide_window_s` to `SettingsDto` and both conversions.
- [ ] **Step 4:** UI: after the damping `.setting` block add
  ```html
  <div class="setting">
    <label for="set-glide-window">Gleitzahl-Zeitraum</label>
    <select id="set-glide-window">
      <option value="10">10 s</option><option value="20">20 s</option>
      <option value="30">30 s</option><option value="60">60 s</option>
    </select>
    <small>Länger ist ruhiger, zeigt nach einem Windsprung aber länger den alten Wert.</small>
  </div>
  ```
  In `app.js`: `showSettings()` sets `$("set-glide-window").value = String(settings.glideWindowS)`; a `change` listener calls `applySettings({ ...settings, glideWindowS: Number(value) })`. Style the select like the other controls in `style.css` if it looks foreign.
- [ ] **Step 5:** Checks pass; `VARIO_SMOKE_TEST=1 cargo run -p vario-app` reports no script errors and the settings page text contains `Gleitzahl-Zeitraum`.
- [ ] **Step 6: Commit** `Add the glide window setting`.

---

### Task 3: Glide ratio over ground (`flight-core::glide`)

**Files:**
- Create: `crates/flight-core/src/glide.rs`; register `pub mod glide;` in `lib.rs`
- Modify: `docs/ALGORITHMEN.md` (new section `## 8. Gleitzahl, Landepunkt, Wind (flight-core)`, subsection Gleitzahl)

**Interfaces:**
- Produces:
  ```rust
  pub struct GlideSample { pub time_s: f64, pub latitude_deg: f64, pub longitude_deg: f64, pub altitude_m: f64 }
  pub struct Glide { pub ratio: f64, pub course_deg: f64, pub ground_speed_mps: f64 }
  pub struct GlideEstimator { .. }
  impl GlideEstimator {
      pub fn new(window_s: f64) -> Self;
      pub fn set_window(&mut self, window_s: f64);
      pub fn push(&mut self, sample: GlideSample);
      pub fn glide(&self) -> Option<Glide>;
  }
  pub const MIN_GROUND_SPEED_MPS: f64 = 2.0;
  pub const MAX_RATIO: f64 = 40.0;
  ```
  All four structs `#[derive(Debug, Clone, Copy, PartialEq)]` (estimator: `Debug, Clone`).

Rules (pin these):
- The buffer always keeps the last **62 s** of samples, whatever the window, so a window change applies at once.
- A gap > **3 s** between consecutive samples clears the buffer.
- Evaluate over the samples within `window_s` of the newest; needs a span ≥ `window_s - 1.0`.
- Displacement `d` = great-circle distance first→last; path `p` = sum of consecutive distances. Straight only if `d / p >= 0.8` (circling gives far less; GPS course noise does not matter because positions are used).
- Mean ground speed `d / span >= MIN_GROUND_SPEED_MPS`.
- Altitude loss `h = first.altitude_m - last.altitude_m`; mean sink `h / span >= 0.1` m/s, else `None` (climbing, zero sinker).
- `ratio = min(d / h, MAX_RATIO)`; `course_deg` = bearing first→last (over ground, so it points backwards when drifting backwards).

- [ ] **Step 1: Write the failing tests** (build samples with `geo::destination` from a start point, 1 Hz):
  - `straight_glide_gives_distance_over_height`: 10 m/s north, −1.25 m/s, window 20 → `ratio` ≈ 8.0 (±0.05), `course_deg` ≈ 0, `ground_speed_mps` ≈ 10.
  - `nothing_until_the_window_is_full`: 10 samples with window 20 → `None`; at 20 samples → `Some`.
  - `circling_gives_nothing`: positions on a 20 s circle of radius 25 m, sinking → `None`.
  - `climbing_and_zero_sink_give_nothing`: +0.5 m/s → `None`; −0.05 m/s → `None`.
  - `very_flat_glides_are_capped`: 10 m/s, −0.2 m/s (ratio 50) → `Some(ratio == 40.0)`.
  - `walking_speed_gives_nothing`: 1.4 m/s, −0.5 m/s → `None`.
  - `drifting_backwards_points_backwards`: heading north but displacement 2 m/s south, −1 m/s → `ratio` ≈ 2.0, `course_deg` ≈ 180.
  - `a_gps_gap_starts_over`: 25 samples, then a 5 s jump, then 5 samples → `None`.
  - `changing_the_window_applies_at_once`: 60 s of straight glide at GZ 8, then 20 s at GZ 4 (sink doubled); window 60 → ratio between 4 and 8; `set_window(10.0)` → ≈4.0 immediately.
- [ ] **Step 2:** `cargo test -p flight-core glide` → FAIL.
- [ ] **Step 3:** Implement with a `VecDeque<GlideSample>`.
- [ ] **Step 4:** Tests pass.
- [ ] **Step 5:** Docs: add section 8 to `docs/ALGORITHMEN.md` with the rules above in German (short, like the other sections).
- [ ] **Step 6:** Checks pass. **Commit** `Compute the glide ratio over ground`.

---

### Task 4: Landing point against the terrain (`flight-core::landing`)

**Files:**
- Create: `crates/flight-core/src/landing.rs`; register in `lib.rs`
- Modify: `crates/flight-core/src/terrain.rs` (trait bound)
- Modify: `docs/ALGORITHMEN.md` (subsection Landepunkt)

**Interfaces:**
- Produces:
  ```rust
  pub struct LandingQuery { pub latitude_deg: f64, pub longitude_deg: f64, pub altitude_m: f64, pub course_deg: f64, pub glide_ratio: f64 }
  pub struct LandingPoint { pub latitude_deg: f64, pub longitude_deg: f64, pub distance_m: f64 }
  pub const LANDING_STEP_M: f64 = 50.0;
  pub const LANDING_MAX_RANGE_M: f64 = 50_000.0;
  pub fn find_landing(query: &LandingQuery, terrain: &dyn Terrain) -> Option<LandingPoint>;
  ```
  `altitude_m` is MSL (GPS altitude), like the terrain. Both structs `Debug, Clone, Copy, PartialEq`.
- Produces: `pub trait Terrain: Send + Sync` (was `Send`) so one terrain can be shared by the engine and the worker thread via `Arc<dyn Terrain>`. Doc comment explains `Sync` ("may be used from several threads at once").

Algorithm: start below ground at distance 0 or unknown ground there → `None`. Step `k * LANDING_STEP_M` along `course_deg` with `geo::destination`; line height `altitude_m - distance / glide_ratio`. Unknown ground at any step → `None`. First step where line ≤ ground: interpolate linearly between this and the previous step on (line − ground) for `distance_m`, recompute the position at that distance. Nothing found within `LANDING_MAX_RANGE_M` → `None`.

- [ ] **Step 1: Write the failing tests** (test terrains as small structs in the test module):
  - `flat_ground_lands_at_height_times_ratio`: `FlatTerrain(600.0)`, altitude 1600, ratio 8 → `distance_m` ≈ 8000 (±1), position ≈ `destination(start, course, 8000)`.
  - `a_slope_ahead_stops_the_glide_early`: terrain rising 1 m per 10 m northwards from 600 m, start 1600 m, ratio 8, course north → hit where `1600 − d/8 = 600 + d/10` → d ≈ 4444 (±5).
  - `backwards_course_lands_behind`: course 180 → point south of the start.
  - `leaving_the_regions_gives_nothing`: terrain known only within 3 km of the start, flat 600 m, expected hit at 8 km → `None`.
  - `no_hit_within_range_gives_nothing`: altitude 8000, ratio 40, `FlatTerrain(0.0)` → `None`.
  - `below_ground_gives_nothing`: altitude 500 over `FlatTerrain(600.0)` → `None`.
- [ ] **Step 2:** `cargo test -p flight-core landing` → FAIL.
- [ ] **Step 3:** Implement; change the trait bound. `cargo build --workspace` and `cargo check -p vario-app` still compile (`RegionTerrain` wraps `Arc<RegionStore>`, which is `Sync`; if the compiler says otherwise, stop and report).
- [ ] **Step 4:** Tests pass; docs subsection written.
- [ ] **Step 5:** Checks pass. **Commit** `Find where the glide meets the terrain`.

---

### Task 5: Wind from full circles (`flight-core::wind`)

**Files:**
- Create: `crates/flight-core/src/wind.rs`; register in `lib.rs`
- Modify: `docs/ALGORITHMEN.md` (subsection Wind)

**Interfaces:**
- Produces:
  ```rust
  pub struct WindSample { pub time_s: f64, pub ground_speed_mps: f64, pub course_deg: f64 }
  pub struct WindEstimate { pub from_deg: f64, pub speed_mps: f64, pub stars: u8 }
  pub struct WindEstimator { .. }
  impl WindEstimator {
      pub fn new() -> Self;            // plus Default
      pub fn push(&mut self, sample: WindSample);
      pub fn estimate(&self, now_s: f64) -> Option<WindEstimate>;
  }
  ```

Rules (pin these):
- **Velocity points:** `x = gs·sin(course)` (east), `y = gs·cos(course)` (north).
- **Turn per sample:** course difference wrapped into (−180°, 180°], divided by dt. A sample gap > 3 s aborts the circle in progress.
- **Circle in progress:** continues while |turn rate| ≥ 4°/s with the same sign; up to 2 consecutive weaker samples are tolerated, a third aborts it. When the summed turn reaches |360°| the circle is complete; the next circle starts at that sample.
- **Accept a circle** only with ≥ 8 samples, duration ≤ 60 s, fitted radius ≥ 3 m/s and RMS residual / radius ≤ 0.3.
- **Fit:** algebraic least-squares circle (Kåsa): solve for D, E, F minimising Σ(x² + y² + Dx + Ey + F)² (3×3 normal equations); centre `(cx, cy) = (−D/2, −E/2)`, radius `√(cx² + cy² − F)`. The centre is the wind vector (towards). `from_deg = atan2(−cx, −cy)` in 0..360, `speed = hypot(cx, cy)`.
- **Thermal:** an accepted circle that starts more than 120 s after the previous accepted circle ended opens a new thermal (the list of circles is replaced by this one). The list keeps at most 6 circles.
- **Mean:** weights 0.7^k, k = 0 for the newest; wind = weighted mean of centres, quality = weighted mean of residual ratios. A new circle whose centre is more than 2.5 m/s from the current mean replaces the list.
- **Stars:** `min(count stars, quality stars)`; count: 1 → 1, 2 → 2, ≥3 → 3; quality: ≤ 0.1 → 3, ≤ 0.2 → 2, else 1. Minus 1 if the newest circle ended more than 10 min before `now_s`, minus 2 beyond 20 min; minus 1 while a circle in progress has summed ≥ 180° and the newest accepted circle ended > 120 s before it started (old wind in a new thermal). Beyond 30 min, or stars < 1 → `None`.

Test helper in the test module: `fn circles(start_s, n_circles, period_s, airspeed, wind_from, wind_speed, noise_seed) -> Vec<WindSample>` computing ground velocity = air velocity + wind at 1 Hz, with optional small deterministic noise (simple LCG, ±0.3 m/s, ±2°).

- [ ] **Step 1: Write the failing tests:**
  - `one_circle_gives_the_wind_with_one_star`: 1 circle, period 20, airspeed 8, wind 270°/4 m/s → `from_deg` ≈ 270 (±3), `speed_mps` ≈ 4 (±0.3), `stars == 1`.
  - `more_clean_circles_give_more_stars`: 3 circles → `stars == 3`.
  - `noisy_circles_cap_the_stars`: 3 circles with large noise (±1.5 m/s) → `stars <= 2`, estimate still within ±20° / ±1 m/s.
  - `straight_flight_gives_no_wind`: 120 s straight → `None`.
  - `left_and_right_circles_both_work`: negative period → same result.
  - `course_wrapping_does_not_fake_a_circle`: straight flight wobbling 355°↔5° for 120 s → `None`.
  - `a_wind_shift_replaces_the_old_wind`: 3 circles at 180°/5 m/s, then within 60 s 1 circle at 0°/7 m/s → `from_deg` ≈ 0, `stars == 1`.
  - `old_wind_loses_stars_and_disappears`: 3 clean circles at t 0–60; `estimate(60 + 11*60)` → 2 stars; at +21 min → 1; at +31 min → `None`.
  - `old_wind_in_a_new_thermal_loses_a_star`: 3 circles, 5 min straight, then 200° of a new circle → stars 2; after that circle completes → stars 1 with the new circle's wind.
- [ ] **Step 2:** `cargo test -p flight-core wind` → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Tests pass; docs subsection written.
- [ ] **Step 5:** Checks pass. **Commit** `Estimate the wind from full circles`.

---

### Task 6: Engine processor computes glide, wind and the landing query

**Files:**
- Modify: `crates/vario-engine/src/processor.rs`

**Interfaces:**
- Consumes: Tasks 2–5.
- Produces in `processor.rs`:
  ```rust
  #[derive(Debug, Clone, PartialEq, Serialize)] #[serde(rename_all = "camelCase")]
  pub struct WindInfo { pub from_deg: f64, pub speed_mps: f64, pub stars: u8 }
  #[derive(Debug, Clone, PartialEq, Serialize)] #[serde(rename_all = "camelCase")]
  pub struct LandingInfo { pub latitude_deg: f64, pub longitude_deg: f64, pub distance_m: f64 }
  impl From<LandingPoint> for LandingInfo
  // new Status fields:
  pub glide_ratio: Option<f64>, pub wind: Option<WindInfo>, pub landing: Option<LandingInfo>,
  impl Processor { pub fn take_landing_query(&mut self) -> Option<LandingQuery> }
  ```
  Re-export `WindInfo`, `LandingInfo` from `vario-engine/src/lib.rs`. `Processor::status` leaves `landing: None` (the engine loop fills it, Task 7). `Processor::set_terrain` now takes `Arc<dyn Terrain>` (field type too).

Behaviour on each GPS fix (in `handle`, after the filter has seen the event):
- Glide sample: fix time/position + **filter altitude** (`self.filter.reading()`; skip the sample without a reading). Differences of pressure altitude are accurate; its absolute value is not used.
- `glide_ratio` shown only if `GlideEstimator::glide()` is `Some` **and** the height gate passes: ground = `terrain.elevation_m(fix)`; if ground is known and the fix is 3D, require `fix.altitude_m - ground >= 30.0`; if ground is known but the fix is 2D → hidden; if ground is unknown → speed gate only.
- Landing query set for this fix only when `glide_ratio` is shown **and** `fix.fix_3d`: `LandingQuery { position, altitude_m: fix.altitude_m, course_deg: glide.course_deg, glide_ratio }`. `take_landing_query` hands it out once (`Option::take`).
- Wind sample when the fix has both `ground_speed_mps` and `course_deg`; `wind` in status from `estimate(last fix time)`.
- `set_settings` forwards `glide_window_s` to the estimator.
- GPS lost (`expire_warnings` path, age > 30 s): `glide_ratio` hidden (estimator keeps its history; the 3 s gap rule clears it on the next fix anyway).

- [ ] **Step 1: Write the failing tests** (use `sensor_sim` like the existing processor tests; build flights with `SyntheticFlight::builder().start(47.665, 11.526, 1700.0)`):
  - `straight_glide_shows_the_glide_ratio`: `Segment::straight(60.0, -1.25)` (10 m/s) → after 30 s `status().glide_ratio` ≈ 8 (±0.5).
  - `circling_hides_the_glide_ratio`: `Segment::circling(60.0, -1.0, 20.0)` → `None` throughout after 25 s.
  - `low_above_the_terrain_hides_the_glide_ratio`: terrain `FlatTerrain(1690.0)` → `None`; with `FlatTerrain(1000.0)` → `Some`.
  - `landing_query_follows_a_shown_glide_ratio`: query `Some` with ratio ≈ 8 after a fix in the glide; `None` on the next call without a new fix; `None` while circling.
  - `wind_appears_after_circling_in_wind`: `.wind(270.0, 4.0)`, 30 s straight then `circling(80.0, 1.0, 20.0)` → `status().wind` from ≈ 270 (±10), speed ≈ 4 (±0.7), stars ≥ 1.
  - `changed_glide_window_applies_at_once`: 60 s at −1.25 then 20 s at −2.5 with window 60; `set_settings` with window 10 → next status ≈ 4 (±0.5) without new fixes needed beyond one.
- [ ] **Step 2:** `cargo test -p vario-engine` → FAIL.
- [ ] **Step 3:** Implement; update callers of `set_terrain` in `lib.rs` (`Shared::pending_terrain: Mutex<Option<Arc<dyn Terrain>>>`, `Engine::set_terrain(&self, terrain: Arc<dyn Terrain>)`) and in `apps/sky/src-tauri/src/lib.rs` (lines ~141 and ~227: `Arc::new(maps::RegionTerrain(store))`). Existing tests that pass a `Box` switch to `Arc`.
- [ ] **Step 4:** Tests pass; checks pass.
- [ ] **Step 5: Commit** `Show glide ratio and wind in the engine status`.

---

### Task 7: Landing search on its own thread

**Files:**
- Create: `crates/vario-engine/src/landing_worker.rs`; `mod landing_worker;` in `lib.rs`
- Modify: `crates/vario-engine/src/lib.rs` (`Engine::start`, `Engine::set_terrain`, `run`)

**Interfaces:**
- Consumes: `find_landing`, `LandingQuery`, `LandingPoint` (Task 4); `Processor::take_landing_query`, `LandingInfo` (Task 6).
- Produces (crate-private is fine):
  ```rust
  pub(crate) struct LandingWorker { .. }
  impl LandingWorker {
      pub fn start() -> io::Result<LandingWorker>;
      pub fn set_terrain(&self, terrain: Arc<dyn Terrain>);
      pub fn submit(&self, query: LandingQuery);   // replaces a waiting query: latest only
      pub fn clear(&self);                          // drops waiting query and result
      pub fn result(&self) -> Option<LandingPoint>;
  }
  impl Drop for LandingWorker  // sets stop, wakes the thread, joins
  ```
  One slot `Mutex<Option<LandingQuery>>` + `Condvar` (explain in a comment: a `Condvar` lets the thread sleep until `submit` wakes it), a `Mutex<Option<LandingPoint>>` result, a `Mutex<Arc<dyn Terrain>>` (starts as `NoTerrain`), an `AtomicBool` stop. Thread name `vario-landing`. The thread clones the `Arc` of the terrain, releases the lock, then searches, so `set_terrain` never waits for a search. A result computed for a query that `clear` dropped meanwhile is discarded (keep a generation counter bumped by `clear`).

Engine loop (`run`): the worker is created in `Engine::start` and moved into the thread. After the events of a round: `if let Some(q) = processor.take_landing_query() { worker.submit(q) }`; build the status; `if status.glide_ratio.is_none() { worker.clear() } else { status.landing = worker.result().map(LandingInfo::from) }`. `Engine::set_terrain` hands the same `Arc` to the processor (pending slot) and to the worker.

- [ ] **Step 1: Write the failing tests:**
  - In `landing_worker.rs`: `finds_the_point_and_forgets_it_on_clear`: `FlatTerrain(600.0)`, submit query 1600 m / GZ 8 → poll `result()` up to 2 s → distance ≈ 8000; `clear()` → `None`; dropping the worker returns (no hang).
  - In `lib.rs` tests: `engine_reports_the_landing_point_of_a_glide`: run a 60 s straight synthetic glide (−1.25 m/s, start 1700 m) through the channel with `Simulation::run_fast`, `engine.set_terrain(Arc::new(FlatTerrain(700.0)))` before sending; `wait_for(|s| s.landing.is_some())`; distance ≈ (GPS altitude − 700) × 8 within 10 %.
- [ ] **Step 2:** `cargo test -p vario-engine` → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Tests pass; checks pass (including `cargo test -p vario-app`).
- [ ] **Step 5: Commit** `Search the landing point on its own thread`.

---

### Task 8: Show it in Sky (layout B)

**Files:**
- Modify: `apps/sky/ui/index.html`, `apps/sky/ui/app.js` (`render`), `apps/sky/ui/map.js`, `apps/sky/ui/style.css`
- Modify: `apps/sky/src-tauri/src/smoke.rs` (report)
- Modify: `apps/sky/CLAUDE.md` (status table row)

**Interfaces:**
- Consumes: `status.glideRatio`, `status.wind {fromDeg, speedMps, stars}`, `status.landing {latitudeDeg, longitudeDeg, distanceM}` (Tasks 6–7).
- Produces: `FlightMap.setLanding(landing | null)`.

Elements and rules (German copy is fixed as written):
- **Readout** `#map-readout` gets a second line `<div id="map-glide" hidden>GZ <span id="map-glide-value"></span></div>`. Format: one decimal with comma (`6,8`); `>= 40` → `>40`.
- **Wind box** `<div id="wind-box" class="map-overlay" hidden>` right under `#follow` (`top: calc(env(safe-area-inset-top) + 64px); right: 10px`), containing an inline SVG arrow rotated by `fromDeg + 180` (points where the wind goes), `<b id="wind-speed"></b> <small>km/h</small>` (rounded km/h), `<div id="wind-stars"></div>` showing `★` × stars and `☆` for the rest of 3. Below 1.4 m/s: arrow hidden, text `< 5`.
- **Landing cross**: GeoJSON source `landing` + symbol layer using a new `drawSymbol("cross")` (black X with white edge, like the other symbols). Re-added in `addOwnLayers` after style changes, same as the track layers. Above the track, below the own position marker.
- **Edge hint** `<div id="landing-hint" class="map-overlay" hidden>` with `▲` rotated to the bearing from the own position to the point (minus map bearing) and the distance (`9,4 km`, one decimal; below 1 km in metres). Shown when the projected point lies outside the map container inset by 20 px; placed where the line from the map centre to the point crosses that inset rectangle, kept clear of `#map-readout`, `#follow` and `#wind-box` by clamping its top to below the wind box. Updated on `setLanding` and on the map's `move` event.
- **Instruments page**: two more tiles in `.tiles`: `<div><b id="glide">–</b><span>GZ</span></div>` and `<div><b id="wind">–</b><span id="wind-label">Wind km/h</span></div>`; the label becomes `Wind aus SW ★★☆` using the existing `compass()` helper on `fromDeg`.
- Hidden means `hidden` attribute set, not empty text. Everything in the dark mode uses the existing CSS variables (check with the dark look on).

- [ ] **Step 1:** Extend the smoke report in `smoke.rs` with one line `glide/wind/landing: <#map-glide-value text>|<#wind-box text>|<landing feature count>` (expose `FlightMap.landingCount()` returning 0/1 for this).
- [ ] **Step 2:** Implement the elements above.
- [ ] **Step 3:** Run `VARIO_SMOKE_TEST=1 cargo run -p vario-app` → no script errors; the report line is present. Values may be empty if the run ends before the first glide window or circle; say so in the report.
- [ ] **Step 4:** Manual check on the desktop (`cargo run -p vario-app`, a Base region with `dem.pmtiles` linked into `maps` covering the Brauneck): within ~1 min a GZ during the glides (≈ 8 at −1.2 m/s), hidden while circling; wind ≈ "aus W, 14 km/h" after the first thermal; cross on the map; panning the map away shows the edge hint; dark look keeps all of it readable. Report which of these were seen; the user tests interactions by hand afterwards.
- [ ] **Step 5:** `apps/sky/CLAUDE.md`: add a status row "Gleitzahl, Landepunkt, Wind" stating "gebaut, mit Tests und im Desktop-Simulator geprüft; nicht im Flug geprüft".
- [ ] **Step 6:** Checks pass. **Commit** `Show glide ratio, landing point and wind in Sky`.
