//! Guidance in the app: the guide follows the planned route with the app's
//! fixes, starts with a recording and is set anew when the route changes.
//!
//! Locking: the guide has its own mutex (`Shared::guide`), taken before
//! `inner` or the planned route where both are needed, never after them.
//! Taking in a fix may ask the router for a way back, which takes a while
//! (Valhalla, on iOS behind a foreign callback). That runs with no lock held
//! but `Shared::guide_turn`, which only orders the fixes: the guide is taken
//! out of its slot, updated, and put back unless a newer guide took its place
//! meanwhile (its generation tells). So a new plan, or a stop, never waits for
//! a way back, and a way back to the old route is dropped.

use std::sync::atomic::Ordering;
use std::sync::{MutexGuard, PoisonError};

use core_location::Fix;
use core_routing::{LatLon, Profile, RoutingError};
use uuid::Uuid;

use super::guide::{Guide, GuideParams, GuidePhase, GuideStatus, WayBack, WayBackRouter};
use super::line::RouteLine;
use crate::AppError;
use crate::app::{App, Shared, now_ms};
use crate::plan::{LegSpec, MAX_POSITION_AGE_S};

/// Setting that names the recording during which the user switched the
/// guidance off; it stays off for that recording, also after a restart.
const OFF_TRACK_KEY: &str = "guide_off_track";

/// The guide running.
pub(crate) struct ActiveGuide {
    /// `None` while a fix is being taken in.
    guide: Option<Guide>,
    /// Which guide this is; see [`Shared::guide_generation`].
    generation: u64,
    /// Last status, also while the guide is taken out.
    status: GuideStatus,
}

/// What to tell the listener, after the locks are released: a status, or
/// `None` for the end of the guidance.
type Told = Vec<Option<GuideStatus>>;

/// Places a new guide by a fix. A guide's first fix never makes a deviation
/// (that takes several fixes), so it is never asked.
struct NotAsked;

impl WayBackRouter for NotAsked {
    fn way_back(
        &self,
        _from: LatLon,
        _heading_deg: Option<f32>,
        _to: LatLon,
        _leg: LegSpec,
    ) -> Result<WayBack, AppError> {
        Err(RoutingError::Unavailable("no way back while placing the guide".into()).into())
    }
}

/// Ways back with the routing of the installed regions, like the legs of a
/// plan but with the direction of travel and without the cache of legs.
impl WayBackRouter for Shared {
    fn way_back(
        &self,
        from: LatLon,
        heading_deg: Option<f32>,
        to: LatLon,
        leg: LegSpec,
    ) -> Result<WayBack, AppError> {
        // Never a straight line in place of a way back.
        if leg.profile == Profile::Manual {
            return Err(RoutingError::Unavailable("a way back is always routed".into()).into());
        }
        let tiles = self.leg_tiles(from, to, &leg)?;
        let route = self.route_on(tiles.as_ref(), from, heading_deg, to, &leg)?;
        let measured = self.measure(route, &leg, tiles.as_ref())?.leg;
        Ok(WayBack {
            line: measured.line,
            distance_m: measured.distance_m,
            time_s: measured.time_s,
        })
    }
}

impl Shared {
    fn guide_slot(&self) -> MutexGuard<'_, Option<ActiveGuide>> {
        self.guide.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn tell(&self, told: Told) {
        for status in told {
            self.listener.on_guidance(status.as_ref());
        }
    }

    /// A guide along the planned route, from waypoint `waypoint` if given,
    /// placed by the last fix if it is recent. `None` without a route.
    fn new_guide(&self, waypoint: Option<usize>) -> Option<ActiveGuide> {
        let line = self
            .routing
            .planned()
            .as_ref()
            .filter(|plan| plan.request.waypoints.len() >= 2)
            .map(RouteLine::from_plan)
            .filter(|line| !line.is_empty())?;
        let mut guide = Guide::new(line, GuideParams::default());
        if let Some(index) = waypoint {
            guide.start_from_waypoint(index);
        }
        let last = self
            .inner()
            .last_fix
            .filter(|fix| (now_ms() - fix.time_ms) as f64 / 1000.0 <= MAX_POSITION_AGE_S);
        let status = match last {
            Some(fix) => guide.update(&fix, &NotAsked),
            None => guide.status(),
        };
        Some(ActiveGuide {
            guide: Some(guide),
            generation: self.guide_generation.fetch_add(1, Ordering::SeqCst) + 1,
            status,
        })
    }

    /// Puts `guide` in the slot, or ends the guidance with `None`. A guide
    /// that has already arrived ends right after its status.
    fn put_guide(slot: &mut Option<ActiveGuide>, guide: Option<ActiveGuide>) -> Told {
        let Some(guide) = guide else {
            return match slot.take() {
                Some(_) => vec![None],
                None => Vec::new(),
            };
        };
        let status = guide.status.clone();
        if status.phase == GuidePhase::Arrived {
            *slot = None;
            return vec![Some(status), None];
        }
        *slot = Some(guide);
        vec![Some(status)]
    }

    /// Whether the user switched the guidance off during recording `track`.
    fn switched_off(&self, track: Uuid) -> bool {
        let value = self.inner().storage.setting(OFF_TRACK_KEY);
        matches!(value, Ok(Some(id)) if id == track.to_string())
    }

    /// The planned route changed: a running guide is set anew on it, or ends
    /// with it. During a recording a new route starts the guidance, unless
    /// the user switched it off for this recording.
    pub(crate) fn plan_changed(&self) {
        let told = {
            let mut slot = self.guide_slot();
            let recording = self.inner().recording_track();
            let start = slot.is_some() || recording.is_some_and(|track| !self.switched_off(track));
            if start {
                let guide = self.new_guide(None);
                Self::put_guide(&mut slot, guide)
            } else {
                Vec::new()
            }
        };
        self.tell(told);
    }

    /// Recording `track` began or goes on: guidance starts along the planned
    /// route, unless it runs already or the user switched it off for `track`.
    pub(crate) fn guide_recording(&self, track: Uuid) {
        let told = {
            let mut slot = self.guide_slot();
            if slot.is_some() || self.switched_off(track) {
                return;
            }
            match self.new_guide(None) {
                Some(guide) => Self::put_guide(&mut slot, Some(guide)),
                None => Vec::new(),
            }
        };
        self.tell(told);
    }

    /// Takes in a fix for the guide, if it runs. See the module for the
    /// locking.
    pub(crate) fn guide_fix(&self, fix: &Fix) {
        let _turn = self
            .guide_turn
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let (mut guide, generation) = {
            let mut slot = self.guide_slot();
            let Some(active) = slot.as_mut() else {
                return;
            };
            let Some(guide) = active.guide.take() else {
                return;
            };
            (guide, active.generation)
        };
        let status = guide.update(fix, self);
        let told = {
            let mut slot = self.guide_slot();
            let Some(active) = slot.as_mut().filter(|a| a.generation == generation) else {
                // Stopped or set anew meanwhile: this result no longer counts.
                return;
            };
            let changed = active.status != status;
            if status.phase == GuidePhase::Arrived {
                *slot = None;
                vec![Some(status), None]
            } else {
                active.guide = Some(guide);
                active.status = status.clone();
                if changed {
                    vec![Some(status)]
                } else {
                    Vec::new()
                }
            }
        };
        self.tell(told);
    }
}

impl App {
    /// Starts guidance along the planned route by hand; it then also runs
    /// during the current recording. Without a recent position it waits for
    /// the first usable fix.
    pub fn start_guidance(&self) -> Result<GuideStatus, AppError> {
        self.start_guidance_at(None)
    }

    /// Like [`App::start_guidance`], with the progress starting at waypoint
    /// `index`; an index past the route starts like `start_guidance`.
    pub fn guide_from_waypoint(&self, index: usize) -> Result<GuideStatus, AppError> {
        self.start_guidance_at(Some(index))
    }

    fn start_guidance_at(&self, waypoint: Option<usize>) -> Result<GuideStatus, AppError> {
        let shared = &self.shared;
        let (status, told) = {
            let mut slot = shared.guide_slot();
            let guide = shared.new_guide(waypoint).ok_or(AppError::NoRoutePlanned)?;
            shared
                .inner()
                .storage
                .set_setting(OFF_TRACK_KEY, "", now_ms())?;
            let status = guide.status.clone();
            (status, Shared::put_guide(&mut slot, Some(guide)))
        };
        shared.tell(told);
        Ok(status)
    }

    /// Ends the guidance. During a recording it stays off for that recording.
    pub fn stop_guidance(&self) {
        let shared = &self.shared;
        let (told, error) = {
            let mut slot = shared.guide_slot();
            let mut inner = shared.inner();
            let error = match inner.recording_track() {
                Some(track) => inner
                    .storage
                    .set_setting(OFF_TRACK_KEY, &track.to_string(), now_ms())
                    .err(),
                None => None,
            };
            (Shared::put_guide(&mut slot, None), error)
        };
        shared.tell(told);
        if let Some(error) = error {
            shared
                .listener
                .on_error(&format!("could not keep the guidance off: {error}"));
        }
    }

    /// Where the guidance stands, if it runs.
    pub fn guidance_status(&self) -> Option<GuideStatus> {
        self.shared
            .guide_slot()
            .as_ref()
            .map(|active| active.status.clone())
    }
}
