# Start- und Landeerkennung – Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ZeroGrid Sky detects takeoff and landing by itself. "Start" only arms the vario; a flight runs from the detected takeoff to the detected landing, as on the Flymaster GPS M, with the vario silent before takeoff and nothing lost when the detection fails.

**Architecture:** A pure state machine in `flight-core` (`flight_phase`) takes one timestamped sample per GPS fix (position, ground speed, climb rate, height above terrain) and reports takeoff and landing, both dated back to when they really happened. `vario-engine`'s `Processor` feeds it, opens a flight file at takeoff (starting with a 60 s buffer of samples from before), closes it at landing, mutes tone and warning sound outside a flight, and keeps a hidden whole-session file so that a session without detected takeoff can still be saved. The UI only shows the phase.

**Tech Stack:** Rust (workspace crates), Tauri 2, plain JS.

**Spec:** `docs/superpowers/specs/2026-11-26-start-landeerkennung-design.md` (approved 26.11.2026 with all four proposals: takeoff by height above terrain, "Ton erst ab Start" on by default, stay ready after landing, save sessions without takeoff and mark them).

## Global Constraints

- Code, comments, commit messages in English; visible UI texts in German. The user is a Rust beginner: explain non-obvious Rust concepts briefly in comments where they first appear (e.g. `enum` with data, `VecDeque`, `matches!`).
- `flight-core`: no UI/OS/audio dependencies, no system clock; time comes as `f64` seconds from the samples.
- Units SI inside; km/h only in the UI and the settings page.
- No new external dependencies, no new thread.
- Values (constants in `flight_phase.rs`, named and documented):
  - Takeoff with terrain: height above terrain ≥ **20 m** for **5 s**.
  - Takeoff without terrain: ground speed ≥ **takeoff speed** (setting, default **15 km/h** = 4.17 m/s) for **5 s**.
  - Landing: ground speed < **5 km/h** (1.39 m/s) and |climb| < **1.5 m/s**, with terrain also height above terrain < **20 m**; held **30 s** with terrain, **60 s** without.
  - "At rest" for dating the takeoff back: ground speed < 5 km/h and, with terrain, height above terrain < 20 m.
  - Pre-takeoff buffer: **60 s**.
- Height above terrain = GPS altitude (MSL) − `Terrain::elevation_m`, only with a 3D fix; otherwise treated as unknown (falls back to the rules without terrain for that sample).
- Show nothing rather than something wrong; a flight must never be lost.
- Before every commit (exit status must not be hidden by pipes):
  ```
  cargo fmt --all -- --check
  cargo clippy --workspace --exclude vario-app --exclude tauri-plugin-vario-sensors --all-targets -- -D warnings
  cargo test --workspace --exclude vario-app --exclude tauri-plugin-vario-sensors
  cargo test -p vario-app
  ```
  On the Mac the ZeroGrid Base crates cannot be built (cmake/Valhalla are Linux-only): there run the Sky crates (`-p flight-core -p sensor-sim -p vario-engine -p vario-cli -p vario-audio -p sensor-bridge -p core-maps -p core-share -p vario-app`) and leave the full workspace to CI.
- Commit messages end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- State plainly what was only checked in the simulator; nothing here is verified at a real launch.

## Review Focus

1. **Soaring in strong wind** (Brauneck-Sattel): ground speed ≈ 0 and climb ≈ 0 for minutes, high above the slope. No landing with terrain; without terrain only after 60 s. Test in Task 2.
2. **Car ride** with terrain: 50 km/h on the ground, altitude changing along the road: no takeoff. Test in Task 2.
3. **Strong-wind launch**: ground speed stays below 15 km/h, height above terrain rises: takeoff detected and dated back to the last moment at rest. Test in Task 2.
4. **GPS lost in the air** (no fixes, or only 2D): the phase stays airborne, no landing from missing data. Test in Task 2.
5. **Top landing and relaunch**: two flight files, the second one starting with its own launch. Test in Task 4.
6. **"Stopp" without takeoff, and app killed without takeoff**: the session is still saved as a flight marked "ohne erkannten Start" if there was movement; nothing written if not. Test in Task 4.
7. **Takeoff while no barometer yet** (climb unknown): landing rule treats unknown climb as calm, takeoff does not need climb. Test in Task 2.

---

### Task 1: Ground phases in the synthetic flights

**Files:**
- Modify: `crates/sensor-sim/src/synthetic.rs`

**Interfaces:**
- Produces: `Segment::on_ground(duration_s: f64, speed_mps: f64) -> Segment` – moves along the current heading at `speed_mps` over ground, altitude constant, **no wind drift** (the glider is not in the air mass). `Segment::is_on_ground()`. Standing is `on_ground(d, 0.0)`.

- [ ] **Step 1: Write the failing tests** in `synthetic.rs` `mod tests`:

```rust
#[test]
fn standing_on_the_ground_does_not_drift_with_the_wind() {
    let flight = SyntheticFlight::builder()
        .wind(270.0, 6.0)
        .segment(Segment::on_ground(30.0, 0.0))
        .build();
    let start = flight.state_at(0.0);
    let end = flight.state_at(30.0);
    assert_eq!((start.latitude_deg, start.longitude_deg), (end.latitude_deg, end.longitude_deg));
    assert_eq!(end.ground_speed_mps, 0.0);
    assert_eq!(end.altitude_m, start.altitude_m);
}

#[test]
fn driving_moves_at_the_given_speed_regardless_of_wind() {
    let flight = SyntheticFlight::builder()
        .heading(90.0)
        .wind(270.0, 6.0)
        .segment(Segment::on_ground(10.0, 14.0))
        .build();
    assert!((flight.state_at(5.0).ground_speed_mps - 14.0).abs() < 1e-9);
}

#[test]
fn wind_drift_starts_only_when_airborne() {
    // 20 s standing, then 10 s straight in 4 m/s wind from the west, heading north:
    // the drift east is 40 m, not 120 m.
    let flight = SyntheticFlight::builder()
        .wind(270.0, 4.0)
        .segment(Segment::on_ground(20.0, 0.0))
        .segment(Segment::straight(10.0, -1.0))
        .build();
    let start = flight.state_at(0.0);
    let end = flight.state_at(30.0);
    let east = distance_m(start.latitude_deg, start.longitude_deg, start.latitude_deg, end.longitude_deg);
    assert!((east - 40.0).abs() < 0.5, "{east}");
}
```

- [ ] **Step 2:** `cargo test -p sensor-sim` → FAIL (no `on_ground`).
- [ ] **Step 3:** Implement. Today the wind offset is added from absolute time; change it to accumulate only over airborne legs (e.g. keep the drift at the start of each leg in `LegStart`, add `wind * t_in_leg` only for airborne legs). Ground legs: `climb_rate_mps = 0`, `turn_period_s = None`, speed is speed over ground, course = heading. Existing tests unchanged.
- [ ] **Step 4:** Checks → pass. **Commit** `Let synthetic flights start and end on the ground`.

---

### Task 2: Flight phase detector in `flight-core`

**Files:**
- Create: `crates/flight-core/src/flight_phase.rs`
- Modify: `crates/flight-core/src/lib.rs` (`pub mod flight_phase;`)

**Interfaces:**

```rust
/// One sample per GPS fix.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhaseSample {
    pub time_s: f64,
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    /// Speed over ground; `None` if the GPS did not report it.
    pub ground_speed_mps: Option<f64>,
    /// Climb rate of the vario; `None` before the filter has a reading.
    pub climb_rate_mps: Option<f64>,
    /// GPS altitude minus terrain; `None` without terrain or without a 3D fix.
    pub height_above_terrain_m: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase { Ready, Airborne, Landed }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PhaseEvent {
    /// Dated back to the last sample at rest before the takeoff.
    Takeoff { time_s: f64, latitude_deg: f64, longitude_deg: f64 },
    /// Dated back to the start of the calm phase that ended the flight.
    Landing { time_s: f64 },
}

pub struct FlightPhaseDetector { /* … */ }
impl FlightPhaseDetector {
    pub fn new(takeoff_speed_mps: f64) -> Self;
    pub fn set_takeoff_speed(&mut self, takeoff_speed_mps: f64);
    pub fn push(&mut self, sample: PhaseSample) -> Option<PhaseEvent>;
    pub fn phase(&self) -> Phase;
    /// Time of the takeoff of the current or last flight.
    pub fn takeoff_time_s(&self) -> Option<f64>;
    /// Time of the landing of the last flight, `None` while airborne.
    pub fn landing_time_s(&self) -> Option<f64>;
}
```

`Ready` and `Landed` behave the same for detection (both wait for a takeoff); they differ only for the UI.

- [ ] **Step 1: Write the failing tests.** Build sample streams with a small helper (1 Hz, fixed position unless moving), and with `SyntheticFlight` + `FlatTerrain` from Task 1 for whole flights. At least:

```rust
#[test] fn standing_at_launch_stays_ready()               // 5 min at rest, GPS noise ±3 m height
#[test] fn takeoff_by_height_above_terrain()              // stand 30 s, then climb away: Takeoff once
#[test] fn takeoff_is_dated_back_to_the_last_rest()       // Takeoff.time_s == last sample at rest
#[test] fn strong_wind_launch_is_detected()               // ground speed 1 m/s, height rising to 40 m
#[test] fn car_ride_with_terrain_is_no_takeoff()          // 14 m/s, height above terrain 0–3 m, 10 min
#[test] fn takeoff_by_speed_without_terrain()             // height None, 5 m/s for 5 s
#[test] fn short_speed_spike_without_terrain_is_no_takeoff() // 3 s above takeoff speed
#[test] fn landing_after_thirty_calm_seconds_near_ground() // dated back to start of the calm
#[test] fn soaring_high_with_no_ground_speed_is_no_landing() // 5 min, speed 0.5, climb 0, height 150
#[test] fn without_terrain_landing_needs_sixty_seconds()  // 45 s calm → still airborne; 61 s → landed
#[test] fn gps_loss_in_the_air_keeps_airborne()           // 2 min without samples, then high again
#[test] fn unknown_climb_counts_as_calm_for_landing()     // climb None near ground, slow → landing
#[test] fn relaunch_after_top_landing()                   // Takeoff, Landing, Takeoff
```

- [ ] **Step 2:** `cargo test -p flight-core flight_phase` → FAIL.
- [ ] **Step 3:** Implement as a small state machine:
  - Keep the samples of the last 60 s (`VecDeque`) for dating back.
  - Waiting (`Ready`/`Landed`): with height known, count how long it stays ≥ 20 m; without, how long speed ≥ takeoff speed. Reset the count on any sample that fails. After 5 s → `Takeoff`, dated back to the newest buffered sample "at rest" before the count started (or the oldest buffered sample if none).
  - `Airborne`: a sample is "calm" when speed < 5 km/h (or unknown), |climb| < 1.5 (or unknown) and, with height known, height < 20 m. Remember when the calm run started; any non-calm sample resets it. Held 30 s (all samples of the run with known height) or 60 s (otherwise) → `Landing` at the start of the run.
  - Gaps: samples further apart than 3 s reset the running counts (no takeoff or landing across a gap); a gap alone never ends a flight.
- [ ] **Step 4:** Document the rules in `docs/ALGORITHMEN.md`, new section **9. Start- und Landeerkennung (`flight-core`)**, including the Flymaster comparison and the known limits (cable car, car without terrain).
- [ ] **Step 5:** Checks → pass. **Commit** `Detect takeoff and landing`.

---

### Task 3: Settings "Ton erst ab Start" and takeoff speed

**Files:**
- Modify: `crates/flight-core/src/settings.rs`
- Modify: `apps/sky/src-tauri/src/settings.rs` (`SettingsDto` + both `From`)
- Modify: `apps/sky/ui/index.html` (settings page, Vario section), `apps/sky/ui/app.js` (settings code)

**Interfaces:**
- Produces: `VarioSettings::tone_after_takeoff: bool` (default `true`, key `tone_after_takeoff`, values `true`/`false`), `VarioSettings::takeoff_speed_mps: f64` (default `15.0 / 3.6`, key `takeoff_speed_mps`, valid 1–15 m/s, `SettingsError::TakeoffSpeedInvalid`). JS fields `toneAfterTakeoff`, `takeoffSpeedKmh` (converted in the DTO).

- [ ] **Step 1: Failing tests** in `settings.rs`: defaults; round trip through `to_text`/`from_text`; old file without the keys gets the defaults; invalid speed (0, 100) rejected; `tone_after_takeoff = maybe` rejected.
- [ ] **Step 2:** FAIL → **Step 3:** implement. Note: `VarioSettings` is `Copy`; both new fields are `Copy` too, so nothing changes for its users.
- [ ] **Step 4:** UI: in the Vario settings a switch "Ton erst ab Start" with the hint "Vario- und Warnton bleiben bis zum erkannten Start stumm, wie „Auto Silent“ am Flymaster." and a number field "Startgeschwindigkeit ohne Gelände (km/h)" with the hint "Nur wo keine Region installiert ist; sonst erkennt Sky den Start an der Höhe über Gelände."
- [ ] **Step 5:** Checks → pass. **Commit** `Add the settings for takeoff detection`.

---

### Task 4: Flights from takeoff to landing in the engine

**Files:**
- Modify: `crates/vario-engine/src/processor.rs` (phase detector, recordings, session file, status)
- Modify: `crates/vario-engine/src/lib.rs` (`Engine::start` leftover session, save at stop)
- Modify: `crates/vario-engine/src/flights.rs` (`FlightEntry::takeoff_detected`)
- Modify: `crates/flight-core/src/igc.rs` only if a helper for `L` records is needed

**Interfaces:**
- `Status::flight: FlightInfo { phase: "ready" | "airborne" | "landed", flight_s: Option<f64> }` – `flight_s` is the time since takeoff while airborne, the length of the last flight when landed, `None` when ready.
- `FlightEntry::takeoff_detected: bool` – `false` for files that carry the mark below.
- Session files: `flights/.session-<start>.igc` (hidden: `is_flight_file_name` already skips names starting with `.`).
- Mark for a session without takeoff: IGC comment record `LXZG NO TAKEOFF DETECTED` right after the header.

Behaviour:
- `Processor` holds a `FlightPhaseDetector` and, per GPS fix, builds a `PhaseSample` (height from `self.terrain` like the glide gate does today).
- **Session recorder:** a `TrackRecorder` from the first fix of the session, saved like a flight every 10 s to the hidden session file.
- **Buffer:** the sensor events of the last 60 s (pressure and GPS; acceleration is not needed for IGC) in a `VecDeque`.
- **Takeoff:** a new flight `Recording` whose `TrackRecorder` is first fed the buffered events from the takeoff time on, then every new event. File name from its first fix, as today.
- **Landing:** feed nothing after the landing time (events arrive in order, fixes of the calm run are already in: cut the recorder's fixes after the landing time before the final save, or delay feeding by the calm run length – Builder's choice, test either way), save, and drop the flight recording. A later takeoff opens a new one.
- **Stop** (`Engine::stop` → final save): if at least one flight was recorded, delete the session file. If none: if the session moved (more than 200 m from its first fix or more than 50 m of altitude range), rename the session file to a normal flight name with the `LXZG NO TAKEOFF DETECTED` record; otherwise delete it.
- **Leftover session file** at `Engine::start` (the app was killed): apply the same rule, except that if a flight file overlapping the session's time exists, keep that and delete the session file. Never delete a session file that could not be read; leave it and push a summary line.

- [ ] **Step 1: Failing tests** in `processor.rs`/`lib.rs` tests with a temp folder, `FlatTerrain`, and synthetic flights from Task 1:
  - whole flight (stand 60 s, fly 3 min, land, stand 60 s): one flight file, first fix within 2 s of the last rest before takeoff, last fix within 2 s of the landing time; no session file left after stop.
  - top landing and relaunch: two flight files.
  - stop while standing only: no flight file, no session file.
  - stop after walking 300 m without takeoff: one flight file with the mark; `flights::list` reports `takeoff_detected: false`.
  - leftover session file without overlapping flight → becomes a marked flight; with an overlapping flight → removed.
  - `Status::flight` goes ready → airborne → landed with plausible `flight_s`.
- [ ] **Step 2:** FAIL → **Step 3:** implement. Remove `record_flight`'s "file from the first fix of the session" behaviour; `Engine::start` with `flights_dir` now enables session file + flights.
- [ ] **Step 4:** Checks → pass. **Commit** `Record flights from takeoff to landing`.

---

### Task 5: Tone, warning sound and glide only in the air

**Files:**
- Modify: `crates/vario-audio/src/voice.rs` (`ToneControl::set_vario_muted(bool)`, lock-free like the other parameters)
- Modify: `crates/vario-engine/src/processor.rs` (`update_tone`, `update_alert`, `shown_glide`)

Behaviour:
- With `tone_after_takeoff` and phase ≠ `Airborne`: vario muted, warning sound `Alert::None`. Banner and map highlight of warnings stay. Test tone and test alert play regardless (they bypass the mute).
- `shown_glide`: only while `Airborne`; remove `height_ok` and `MIN_HEIGHT_ABOVE_GROUND_M`. The 5 km/h gate in `glide` stays.
- Changing the setting during a session applies at once.

- [ ] **Step 1: Failing tests:** muted before takeoff, audible after, muted again after landing; muted never with the setting off; test tone audible while ready; warning sound silent while ready but warning present in `Status`; no glide ratio while ready even high above flat terrain (e.g. sitting in a cable car); glide shown after takeoff. Adapt the existing tests of the 30 m gate to the new rule.
- [ ] **Step 2:** FAIL → **Step 3:** implement. In `voice.rs` the mute is an `AtomicBool` read in the audio callback (no lock, no allocation); the tone fades out like a volume change, no click.
- [ ] **Step 4:** Checks → pass. **Commit** `Keep the tones and the glide ratio for the flight`.

---

### Task 6: Phase in the UI and the flight list

**Files:**
- Modify: `apps/sky/ui/app.js` (`renderRecording`, flight list), `apps/sky/ui/style.css` if needed

Behaviour (texts exactly):
- Ready: `Bereit · wartet auf Start`
- Airborne: `● In der Luft · 12:07 min` (from `flight.flightS`, `duration()` format)
- Landed: `Gelandet · Flugzeit 1:12 h`
- GPS missing and saving errors as today, before the phase text.
- Flight list: entries with `takeoffDetected === false` get the suffix ` · ohne erkannten Start`. The running flight is marked "läuft" only while airborne.

- [ ] **Step 1:** Implement; keep the text of the instruments page short enough for the iPhone width.
- [ ] **Step 2:** Desktop self-test (Task 7) shows the texts. **Commit** `Show the flight phase in Sky`.

---

### Task 7: Desktop simulator and self-test

**Files:**
- Modify: `apps/sky/src-tauri/src/sim.rs` (`long_flight`)
- Modify: `apps/sky/src-tauri/src/smoke.rs` (report line)

- [ ] **Step 1:** `long_flight` starts with **20 s standing** at a launch point on the Brauneck inside the test region, at the DEM height of that point (look it up once in `dem.pmtiles` of the test region, e.g. with a short throwaway program against `core-maps`, and write the value with a comment where it comes from), then the existing rounds, and ends with a glide to the ground and **60 s standing** (landing field in the valley, DEM height looked up the same way). Wind as today.
- [ ] **Step 2:** Smoke report: new line `flight phase: <phase> · <flight_s>` and `takeoff detected: yes/no`.
- [ ] **Step 3:** Run the self-tests (screen unlocked, see `apps/sky/CLAUDE.md`), each with an empty `HOME`:
  - 14 s: `Bereit`, no flight file yet.
  - 150 s with region: airborne, glide/wind/landing point as before.
  - 150 s without region: takeoff by speed.
  - Full flight (run time over the whole profile): landed, one flight file in the flight list, starting at the launch.
  - "Ton erst ab Start" off: no change in detection.
- [ ] **Step 4:** Update the useful commands in `apps/sky/CLAUDE.md` if the run times change. **Commit** `Fly the desktop simulator from launch to landing`.

---

### Task 8: Status, iOS build, handover

- [ ] **Step 1:** `apps/sky/CLAUDE.md`: new row in the phase table, "Start-/Landeerkennung … gebaut, mit Tests und im Desktop-Simulator geprüft; nicht an einem echten Start geprüft".
- [ ] **Step 2:** iOS build for the device (`cargo tauri ios build --debug --target aarch64 --ci`), install on the iPhone 13 Pro Max; check by hand at the desk: starts in `Bereit`, no tone while ready, walking around does not start a flight (with a region installed).
- [ ] **Step 3:** Report to the user what was checked where, and what only a real launch can show (threshold of 20 m at a flat launch, strong-wind launch, landing in a sloped landing field). **Commit** `Record the checks of takeoff and landing detection`.
