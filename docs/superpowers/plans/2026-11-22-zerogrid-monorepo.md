# ZeroGrid Monorepo Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Merge `outdoor_app` and `zerogrid-sky` into one repository and rename the apps to ZeroGrid Base, Trail and Sky without losing data or history.

**Architecture:** Sky's history joins through one path-moving commit and an unrelated-histories merge. One Cargo workspace holds all crates; Trail and Sky keep separate Xcode projects. Base adopts its old data directory before Tauri starts.

**Tech Stack:** Rust 2024, Cargo workspace, Tauri 2, SwiftUI, XcodeGen, GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-11-22-zerogrid-monorepo-design.md`

## Global Constraints

- Identifiers: `de.zeibig.zerogrid.base`, `de.zeibig.zerogrid.trail`, `de.zeibig.zerogrid.sky`.
- Display names: `ZeroGrid Base`, `ZeroGrid Trail`, `ZeroGrid Sky`.
- Unchanged: `outdoor.pmtiles`, layer names, `outdoor.sqlite`, `outdoor-tiles://`, `OUTDOOR_*`, crates `core-*` and `vario-*`, `outdoor_core`, `OutdoorCore.xcframework`, the iOS data folder `outdoor`.
- Historic files in `docs/superpowers/` stay unchanged, except this plan and its spec.
- Dependency versions change only as far as Cargo requires.
- Public repository: no home-network addresses, home coordinates or credentials.
- Never push, rename or archive on GitHub without asking the user first.
- Before each commit: `cargo fmt --all --check`, `cargo clippy --workspace --exclude vario-app --exclude tauri-plugin-vario-sensors --all-targets -- -D warnings`, and the matching tests.
- Code, comments and commits in English; user-visible app text in German.

## Review Focus

1. Tauri creates the new data directory before the setup hook: adoption must run first in `main` (Task 3, self-test on a seeded `XDG_DATA_HOME`).
2. Old and new data directories both exist: neither is changed (Task 3, unit test).
3. `OUTDOOR_DATA_DIR` is set: the real user data directory is not touched (Task 3, unit test).
4. Sky now uses `core-maps` from `main`, 9 commits newer than its pin: its region tests and share self-test must still pass (Task 1).
5. Imported history or files contain private network addresses (Task 1, scan before merge).

---

## Linux

### Task 1: Import Sky and join the workspace

**Files:**
- Move in a Sky clone: `apps/vario-app/` → `apps/sky/`, `docs/` → `docs/sky/`, `testdata/` → `testdata/sky/`, `CLAUDE.md` and `STARTPROMPT.md` → `apps/sky/`
- Delete in that clone: root `Cargo.toml`, `Cargo.lock`, `.gitignore`
- Modify: `Cargo.toml`, `Cargo.lock`, `.github/workflows/ci.yml`, `apps/sky/src-tauri/Cargo.toml`, `apps/sky/plugins/vario-sensors/Cargo.toml`, `crates/{flight-core,sensor-bridge,sensor-sim,vario-audio,vario-cli,vario-engine}/Cargo.toml`, `apps/sky/CLAUDE.md`

**Interfaces:**
- Produces: workspace members `vario-app`, `tauri-plugin-vario-sensors`, and the six Sky crates.

- [ ] **Step 1: Confirm the Sky head with the user, clone it fresh, and record its commit.** Expected start: `ad2ced0`, unless the Mac has pushed more.
- [ ] **Step 2: Scan the Sky tree and history.**

Run: `git log -p --all | grep -nE '\b(192\.168|10\.[0-9]+\.[0-9]+|172\.(1[6-9]|2[0-9]|3[01]))\.[0-9]+'`
Expected: only documentation placeholders such as `192.168.x.y`. Stop and ask the user on any real address.

- [ ] **Step 3: Commit the moves and deletions in the clone** as `Move ZeroGrid Sky to its place in the monorepo`.
- [ ] **Step 4: Merge with `--allow-unrelated-histories --no-commit`** into `feature/zerogrid-monorepo`.
- [ ] **Step 5: Adapt the workspace inside the merge.**
  - Root members: `["crates/*", "tools/*", "apps/desktop/src-tauri", "apps/sky/src-tauri", "apps/sky/plugins/*"]`.
  - `default-members`: `["crates/*", "tools/*", "apps/desktop/src-tauri"]`.
  - Add workspace dependencies `flight-core`, `sensor-sim`, `vario-audio`, `sensor-bridge`, `vario-engine`.
  - In all eight Sky manifests replace `[lints] workspace = true` with `[lints.rust] unsafe_code = "deny"`.
  - In the Sky app use `core-maps.workspace = true`.
  - In CI add `libasound2-dev`, and add `--exclude vario-app --exclude tauri-plugin-vario-sensors` to workspace clippy and test.
- [ ] **Step 6: Verify.**

Run: `cargo test --workspace --exclude vario-app --exclude tauri-plugin-vario-sensors`
Expected: all pass.

Run: `cargo test -p vario-app`
Expected: all pass. If only Linux-specific build problems appear, report them and leave the app out of CI as the spec allows.

- [ ] **Step 7: Check history.** `git log --follow crates/flight-core/Cargo.toml` shows original Sky commits, and `ad2ced0` is an ancestor of `HEAD`.
- [ ] **Step 8: Commit the merge** as `Merge ZeroGrid Sky into the repository`.

### Task 2: Move the app folders

**Files:**
- Move: `apps/desktop/` → `apps/base/`, `apps/ios/` → `apps/trail/`
- Modify: every reference outside `docs/superpowers/`: `Cargo.toml`, `.github/workflows/ci.yml`, `crates/core-routing/tests/routing.rs:335`, `crates/core-app/tests/plan_edit.rs`, `crates/core-routing/src/model.rs`, `tools/contour-tiles/src/lib.rs`, `apps/trail/scripts/build-core.sh`, `README.md`, `PLAN.md`, `docs/DATENQUELLEN.md`, `docs/MESSPROTOKOLL.md` and the Swift comments that name these paths.

- [ ] **Step 1: Move with `git mv` and update the references.**
- [ ] **Step 2: Verify no stale path remains.**

Run: `git grep -nE 'apps/(desktop|ios)' -- . ':!docs/superpowers'`
Expected: no output.

- [ ] **Step 3: Run the Global Constraints checks.** Expected: pass.
- [ ] **Step 4: Commit** as `Move the apps to apps/base and apps/trail`.

### Task 3: Rename Base and adopt its data

**Files:**
- Create: `apps/base/src-tauri/src/data_adoption.rs`
- Modify: `apps/base/src-tauri/src/main.rs`, `apps/base/src-tauri/Cargo.toml` (name `zerogrid-base`, add `dirs = "7"`), `apps/base/src-tauri/tauri.conf.json`, `apps/base/ui/index.html`, `apps/base/ui/package.json` (`zerogrid-base-ui`), `scripts/measure-performance.sh`, `README.md`

**Interfaces:**
- Produces:
  - `pub enum Adoption { Adopted, NotNeeded }`
  - `pub fn adopt_legacy_dir(legacy: &Path, current: &Path) -> std::io::Result<Adoption>`
  - `pub fn adopt_user_data(data_override: Option<&OsStr>, data_root: Option<&Path>) -> std::io::Result<Adoption>`, using `de.outdoor-app.desktop` and `de.zeibig.zerogrid.base` under `data_root`

- [ ] **Step 1: Write failing tests in `data_adoption.rs`.**

```rust
#[test] fn moves_legacy_dir_when_current_is_missing() { /* file arrives in current; legacy is gone; Adopted */ }
#[test] fn leaves_both_dirs_alone_when_both_exist() { /* both files unchanged; NotNeeded */ }
#[test] fn does_nothing_without_legacy_dir() { /* current not created; NotNeeded */ }
#[test] fn override_never_touches_the_user_data_root() { /* Some(override): legacy stays; NotNeeded */ }
#[test] fn failed_move_is_an_error_and_creates_no_current_dir() { /* parent of current is a file: Err; legacy intact */ }
```

- [ ] **Step 2: Run them.** `cargo test -p zerogrid-base data_adoption`. Expected: FAIL because the functions are missing.
- [ ] **Step 3: Implement with `std::fs::rename`.** Do not copy and never delete.
- [ ] **Step 4: Call `adopt_user_data` first in `main`,** with `OUTDOOR_DATA_DIR` and `dirs::data_dir()`. On error print a German message naming both directories and return `ExitCode::FAILURE` before `tauri::Builder`.
- [ ] **Step 5: Set `productName`, window title, `<title>`, `<h1>` to `ZeroGrid Base`, and `identifier` to `de.zeibig.zerogrid.base`.** Change the GPX `creator` to `ZeroGrid` in `crates/core-gpx/src/write.rs` and its test.
- [ ] **Step 6: Run tests.** Expected: pass.
- [ ] **Step 7: Self-test with real startup order.** Seed a temporary `XDG_DATA_HOME/de.outdoor-app.desktop` with a copy of a small test database, run the app with `OUTDOOR_SELFTEST_SHARE=1`. Expected: the data sits in `de.zeibig.zerogrid.base`, the old directory is gone, the self-test passes. Never use the real 7.7 GB directory here.
- [ ] **Step 8: Commit** as `Rename the desktop app to ZeroGrid Base and adopt its data`.

### Task 4: Rename Trail and update names in Sky

**Files:**
- Move: `apps/trail/Outdoor/` → `ZeroGridTrail/`, `OutdoorTests/` → `ZeroGridTrailTests/`, `OutdoorUITests/` → `ZeroGridTrailUITests/`, `OutdoorApp.swift` → `ZeroGridTrailApp.swift`
- Modify: `apps/trail/project.yml`, `apps/trail/scripts/test.sh`, all `@testable import Outdoor`, the `de.zeibig.outdoor` loggers and queues, the camera hint in `ScanSheet.swift`, `apps/sky/src-tauri/Info.ios.plist`, `apps/sky/src-tauri/gen/apple/vario-app_iOS/Info.plist`, the Sky settings text, `apps/sky/CLAUDE.md`, `README.md`

- [ ] **Step 1: Rename project, targets and scheme to `ZeroGridTrail`, display name to `ZeroGrid Trail`, bundle ID to `de.zeibig.zerogrid.trail`.** Keep `OutdoorCore`, `outdoor_core`, `outdoor-tiles` and the data folder `outdoor`.
- [ ] **Step 2: In Sky change “Desktop im WLAN (outdoor_app)” to “ZeroGrid Base im WLAN” and the permission text to `Lädt Kartenregionen von ZeroGrid Base am Computer im Heimnetz.`**
- [ ] **Step 3: Verify.**

Run: `git grep -nE 'de\.zeibig\.outdoor|import Outdoor$|CFBundleDisplayName: Outdoor' -- apps`
Expected: no output.

Run: `python3 -c "import yaml;yaml.safe_load(open('apps/trail/project.yml'))"`
Expected: no error. Xcode builds happen in Task 7.

- [ ] **Step 4: Run the Global Constraints checks and `cargo test -p vario-app`.** Expected: pass.
- [ ] **Step 5: Commit** as `Rename the iOS app to ZeroGrid Trail and update names in Sky`.

### Task 5: Review and pull request

- [ ] **Step 1: Whole-branch review; fix Critical and Important findings.**
- [ ] **Step 2: Ask the user, then push the branch and open a PR.** Stop here and show the result.

## After the merge (each step needs the user's explicit okay)

### Task 6: GitHub rename and first real data move

- [ ] **Step 1: Merge the PR with `--no-ff`.**
- [ ] **Step 2: Rename `Isvinter/outdoor_app` to `Isvinter/zerogrid`** and update `origin` on Linux.
- [ ] **Step 3: User starts Base once.** Expected: maps and routes are all there, under `~/.local/share/de.zeibig.zerogrid.base`.

## Mac

### Task 7: Build Trail and Sky

- [ ] **Step 1: Point the Mac clone at `zerogrid`, pull `main`, stop using the old Sky clone.**
- [ ] **Step 2: Trail:** `apps/trail/scripts/build-core.sh && xcodegen generate`, sign, run on the iPhone. Expected: new app ID registered, app `ZeroGrid Trail` installed beside the old one. Run `scripts/test.sh`.
- [ ] **Step 3: Load maps and routes into Trail over Wi-Fi.** Delete the old app only after the user confirms.
- [ ] **Step 4: Sky:** build from `apps/sky` as a release on the iPhone. Expected: existing maps, flights and settings remain; vario tone and sensors work with the shared LTO profile.

## Finish

### Task 8: Archive and end-to-end Wi-Fi test

- [ ] **Step 1: After the user's okay, archive `Isvinter/zerogrid-sky`.**
- [ ] **Step 2: Run the test from `docs/sky/handover-linux.md`:** real Base share, list, download, cancel, resume, update after rebuild, offline flight map on the iPhone.
- [ ] **Step 3: Record the result in `docs/sky/ios-test.md` and phase 6b of `apps/sky/CLAUDE.md`, then commit.**
