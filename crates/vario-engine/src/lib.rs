//! The engine: takes sensor samples from a channel and runs them through the vario
//! filter, the tone control and the logs, on its own background thread.
//!
//! It knows nothing about the user interface. The UI only asks for a [`Status`] snapshot
//! now and then, so the engine keeps working when the UI is suspended (locked screen).
//!
//! * [`processor`] holds the actual logic, without threads, so it can be tested directly.
//! * [`Engine`] wraps it in a thread.
//! * `landing_worker` searches the landing point on a second thread, so a slow terrain
//!   search never delays the tone.

pub mod flights;
mod landing_worker;
pub mod processor;

use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use flight_core::airspace::Airspace;
use flight_core::airspace_warning::WarningSettings;
use flight_core::sensor::SensorEvent;
use flight_core::settings::VarioSettings;
use flight_core::terrain::Terrain;
use flight_core::time::UtcDateTime;
use vario_audio::alert::Alert;
use vario_audio::voice::ToneControl;

use crate::landing_worker::LandingWorker;
pub use crate::processor::{
    AirspaceWarning, FixInfo, LandingInfo, Processor, RecordingInfo, SensorStats, Status,
    TrackPoint, WindInfo,
};

/// How often a summary line is written and the sensor rates are refreshed.
const SUMMARY_INTERVAL: Duration = Duration::from_secs(10);
/// How long the engine thread sleeps at most while no samples arrive.
const IDLE_WAKEUP: Duration = Duration::from_millis(250);

/// State shared between the engine thread and its owner.
#[derive(Default)]
struct Shared {
    status: Mutex<Status>,
    stop: AtomicBool,
    /// Climb rate that overrides the filter output, for testing the tone.
    test_climb_mps: Mutex<Option<f64>>,
    /// Warning sound that overrides the airspace warnings, for testing it.
    test_alert: Mutex<Option<Alert>>,
    /// Settings changed by the user that the engine thread has not applied yet.
    pending_settings: Mutex<Option<VarioSettings>>,
    /// The flown track since the engine started, one point per second.
    track: Mutex<Vec<TrackPoint>>,
    /// Airspace data and settings not applied yet, like `pending_settings`.
    pending_airspaces: Mutex<Option<Arc<Vec<Airspace>>>>,
    pending_terrain: Mutex<Option<Arc<dyn Terrain>>>,
    pending_warning_settings: Mutex<Option<WarningSettings>>,
    /// Warnings the user silenced, by airspace index.
    pending_acknowledged: Mutex<Vec<usize>>,
}

/// A running engine. Stops when [`Engine::stop`] is called or the value is dropped.
pub struct Engine {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Engine {
    /// Starts the engine thread.
    ///
    /// `log_dir`: if given, a raw sensor log and a summary log are written there.
    /// `flights_dir`: if given, the flight is recorded there as an IGC file.
    pub fn start(
        events: Receiver<SensorEvent>,
        control: ToneControl,
        settings: &VarioSettings,
        log_dir: Option<PathBuf>,
        flights_dir: Option<PathBuf>,
    ) -> io::Result<Engine> {
        let mut processor = Processor::new(control, settings);
        if let Some(dir) = &flights_dir {
            // Sessions left over from a crash first, so they are not mixed up with this one.
            for note in flights::recover_sessions(dir) {
                processor.note(note);
            }
            processor.record_flights(dir);
        }
        if let Some(dir) = &log_dir {
            processor.open_logs(dir, &timestamp_for_file_name())?;
        }

        let shared = Arc::new(Shared::default());
        let thread_shared = Arc::clone(&shared);
        // Moved into the engine thread; it stops when that thread ends.
        let landing = LandingWorker::start()?;
        let thread = std::thread::Builder::new()
            .name("vario-engine".to_owned())
            .spawn(move || run(processor, events, &thread_shared, &landing))?;

        Ok(Engine {
            shared,
            thread: Some(thread),
        })
    }

    /// A snapshot of the current state.
    pub fn status(&self) -> Status {
        lock(&self.shared.status).clone()
    }

    /// Makes the tone play as if climbing at this rate, regardless of the sensors.
    /// `None` returns to normal operation.
    pub fn set_test_climb(&self, climb_rate_mps: Option<f64>) {
        *lock(&self.shared.test_climb_mps) = climb_rate_mps;
    }

    /// Plays a warning sound regardless of the airspaces. `None` returns to normal.
    pub fn set_test_alert(&self, alert: Option<Alert>) {
        *lock(&self.shared.test_alert) = alert;
    }

    /// The track points from index `from` on. The UI remembers how many points it
    /// already has and asks only for the new ones, so the amount of data per request
    /// stays small however long the flight is.
    pub fn track_since(&self, from: usize) -> Vec<TrackPoint> {
        lock(&self.shared.track)
            .get(from..)
            .map(<[TrackPoint]>::to_vec)
            .unwrap_or_default()
    }

    /// Changes the settings while running. The engine thread picks them up within a
    /// fraction of a second.
    pub fn set_settings(&self, settings: VarioSettings) {
        *lock(&self.shared.pending_settings) = Some(settings);
    }

    /// Replaces the airspaces to warn about. `Arc` shares the list with the caller
    /// without copying it.
    pub fn set_airspaces(&self, airspaces: Arc<Vec<Airspace>>) {
        *lock(&self.shared.pending_airspaces) = Some(airspaces);
    }

    /// Sets where the ground elevation comes from (AGL limits, glide gate, landing
    /// point). The engine thread hands the same `Arc` to the processor and to the
    /// landing thread: one elevation model, shared without copying. This is why
    /// `Terrain` requires `Send + Sync`: `Send` lets the model move to another thread,
    /// `Sync` lets both threads use it at the same time.
    pub fn set_terrain(&self, terrain: Arc<dyn Terrain>) {
        *lock(&self.shared.pending_terrain) = Some(terrain);
    }

    pub fn set_warning_settings(&self, settings: WarningSettings) {
        *lock(&self.shared.pending_warning_settings) = Some(settings);
    }

    /// Silences the near warning of the airspace with this index until it ends.
    pub fn acknowledge_warning(&self, index: usize) {
        lock(&self.shared.pending_acknowledged).push(index);
    }

    /// Stops the thread and waits until it has written its logs.
    pub fn stop(mut self) {
        self.shut_down();
    }

    fn shut_down(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            // An error here means the thread panicked; there is nothing left to do.
            let _ = thread.join();
        }
    }
}

// `Drop` runs when a value goes out of scope. This guarantees the thread never
// outlives the `Engine`, even if `stop` was not called.
impl Drop for Engine {
    fn drop(&mut self) {
        self.shut_down();
    }
}

fn run(
    mut processor: Processor,
    events: Receiver<SensorEvent>,
    shared: &Shared,
    landing: &LandingWorker,
) {
    let started = Instant::now();
    let mut last_summary = Instant::now();

    while !shared.stop.load(Ordering::Relaxed) {
        // `take` empties the slot, so each change is applied exactly once.
        if let Some(settings) = lock(&shared.pending_settings).take() {
            processor.set_settings(&settings);
        }
        apply_airspace_changes(&mut processor, shared, landing);
        processor.set_test_alert(*lock(&shared.test_alert));
        // Covers the case that no sensor delivers at all any more.
        processor.note_gps_age(processor.gps_age_s());
        // Sleep until a sample arrives, but wake up regularly to notice `stop`.
        match events.recv_timeout(IDLE_WAKEUP) {
            Ok(event) => {
                processor.set_test_climb(*lock(&shared.test_climb_mps));
                processor.handle(&event);
                // Handle everything else that is already waiting before publishing.
                for event in events.try_iter() {
                    processor.handle(&event);
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                processor.set_test_climb(*lock(&shared.test_climb_mps));
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }

        if last_summary.elapsed() >= SUMMARY_INTERVAL {
            processor.summarize(last_summary.elapsed().as_secs_f64(), &utc_clock_label());
            last_summary = Instant::now();
        }
        let new_points = processor.take_track_points();
        if !new_points.is_empty() {
            lock(&shared.track).extend(new_points);
        }
        if let Some(query) = processor.take_landing_query() {
            landing.submit(query);
        }
        let mut status = processor.status(started.elapsed().as_secs_f64(), true);
        // The landing point belongs to the shown glide: no glide, no point.
        if status.glide_ratio.is_none() {
            landing.clear();
        } else {
            status.landing = landing.result().map(LandingInfo::from);
        }
        *lock(&shared.status) = status;
    }

    processor.summarize(last_summary.elapsed().as_secs_f64(), &utc_clock_label());
    processor.finish_session();
    processor.silence();
    *lock(&shared.status) = processor.status(started.elapsed().as_secs_f64(), false);
}

fn apply_airspace_changes(processor: &mut Processor, shared: &Shared, landing: &LandingWorker) {
    if let Some(airspaces) = lock(&shared.pending_airspaces).take() {
        processor.set_airspaces(airspaces);
    }
    if let Some(terrain) = lock(&shared.pending_terrain).take() {
        landing.set_terrain(Arc::clone(&terrain));
        processor.set_terrain(terrain);
    }
    if let Some(settings) = lock(&shared.pending_warning_settings).take() {
        processor.set_warning_settings(settings);
    }
    for index in std::mem::take(&mut *lock(&shared.pending_acknowledged)) {
        processor.acknowledge_warning(index);
    }
}

/// Locks a mutex, ignoring "poisoning" (a panic in another thread while it held the
/// lock): the status data is still usable.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn utc_now() -> UtcDateTime {
    let unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64);
    UtcDateTime::from_unix_seconds(unix)
}

fn utc_clock_label() -> String {
    let time = utc_now().time;
    format!(
        "{:02}:{:02}:{:02}",
        time.hour(),
        time.minute(),
        time.second()
    )
}

fn timestamp_for_file_name() -> String {
    let now = utc_now();
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        now.date.year(),
        now.date.month(),
        now.date.day(),
        now.time.hour(),
        now.time.minute(),
        now.time.second()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use flight_core::sensor::{AccelSample, PressureSample};
    use flight_core::time::Timestamp;
    use std::sync::mpsc;

    fn wait_for(engine: &Engine, condition: impl Fn(&Status) -> bool) -> Status {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let status = engine.status();
            if condition(&status) {
                return status;
            }
            assert!(Instant::now() < deadline, "timed out, status {status:?}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn engine_processes_events_from_another_thread_and_stops() {
        let (sender, receiver) = mpsc::channel();
        let control = ToneControl::new(&VarioSettings::default());
        let engine =
            Engine::start(receiver, control, &VarioSettings::default(), None, None).unwrap();

        sender
            .send(SensorEvent::Pressure(PressureSample {
                time: Timestamp::from_secs(1.0),
                pressure_pa: 90_000.0,
            }))
            .unwrap();
        sender
            .send(SensorEvent::Accel(AccelSample {
                time: Timestamp::from_secs(1.02),
                vertical_mps2: 0.0,
            }))
            .unwrap();

        let status = wait_for(&engine, |status| status.accel.total == 1);
        assert!(status.running);
        assert_eq!(status.pressure.total, 1);
        assert!((status.altitude_m.unwrap() - 988.5).abs() < 1.0);

        engine.set_test_climb(Some(2.0));
        let status = wait_for(&engine, |status| status.test_tone);
        assert!(status.test_tone);

        engine.stop();
    }

    #[test]
    fn engine_reports_the_landing_point_of_a_glide() {
        use flight_core::terrain::FlatTerrain;
        use flight_core::time::{Date, TimeOfDay};
        use sensor_sim::synthetic::{Segment, SyntheticFlight};
        use sensor_sim::{SimConfig, Simulation};

        let (sender, receiver) = mpsc::channel();
        let settings = VarioSettings::default();
        let engine =
            Engine::start(receiver, ToneControl::new(&settings), &settings, None, None).unwrap();
        engine.set_terrain(Arc::new(FlatTerrain(700.0)));

        let flight = SyntheticFlight::builder()
            .start(47.665, 11.526, 1700.0)
            .segment(Segment::straight(60.0, -1.25))
            .build();
        let utc = UtcDateTime::new(
            Date::new(2026, 7, 28).unwrap(),
            TimeOfDay::new(10, 0, 0).unwrap(),
        );
        // `sender` stays alive until the end: a closed channel would stop the engine.
        Simulation::synthetic(flight, utc, &SimConfig::default())
            .run_fast(|event| sender.send(event).unwrap());

        let status = wait_for(&engine, |status| status.landing.is_some());
        let expected = (status.fix.as_ref().unwrap().altitude_m - 700.0) * 8.0;
        let distance = status.landing.unwrap().distance_m;
        assert!(
            (distance - expected).abs() < expected * 0.1,
            "distance {distance}, expected {expected}"
        );
        engine.stop();
    }

    /// Starts an engine over flat ground at 700 m and sends it the events of a glide
    /// flight with these segments up to `until_s`; returns the engine, the sender (a
    /// closed channel would stop the engine) and the remaining events.
    fn engine_with_flight(
        segments: Vec<sensor_sim::synthetic::Segment>,
        until_s: f64,
    ) -> (Engine, mpsc::Sender<SensorEvent>, Vec<SensorEvent>) {
        use flight_core::terrain::FlatTerrain;
        use flight_core::time::{Date, TimeOfDay};
        use sensor_sim::synthetic::SyntheticFlight;
        use sensor_sim::{SimConfig, Simulation};

        let (sender, receiver) = mpsc::channel();
        let settings = VarioSettings::default();
        let engine =
            Engine::start(receiver, ToneControl::new(&settings), &settings, None, None).unwrap();
        engine.set_terrain(Arc::new(FlatTerrain(700.0)));
        let mut builder = SyntheticFlight::builder().start(47.665, 11.526, 1700.0);
        for segment in segments {
            builder = builder.segment(segment);
        }
        let utc = UtcDateTime::new(
            Date::new(2026, 7, 28).unwrap(),
            TimeOfDay::new(10, 0, 0).unwrap(),
        );
        let mut rest = Vec::new();
        Simulation::synthetic(builder.build(), utc, &SimConfig::default()).run_fast(|event| {
            if event.time().as_secs() <= until_s {
                sender.send(event).unwrap();
            } else {
                rest.push(event);
            }
        });
        (engine, sender, rest)
    }

    #[test]
    fn engine_hides_the_landing_point_when_the_glide_ends() {
        use sensor_sim::synthetic::Segment;
        let (engine, sender, rest) = engine_with_flight(
            vec![
                Segment::straight(60.0, -1.25),
                Segment::circling(40.0, 1.0, 20.0),
            ],
            60.0,
        );
        let status = wait_for(&engine, |status| status.landing.is_some());
        let fixes = status.gps.total
            + rest
                .iter()
                .filter(|event| matches!(event, SensorEvent::Gps(_)))
                .count() as u64;
        for event in rest {
            sender.send(event).unwrap();
        }
        let status = wait_for(&engine, |status| status.gps.total == fixes);
        assert_eq!(status.glide_ratio, None);
        assert_eq!(status.landing, None);
        engine.stop();
    }

    #[test]
    fn engine_hides_the_glide_and_landing_point_when_the_gps_is_lost() {
        use sensor_sim::synthetic::Segment;
        let (engine, _sender, _rest) =
            engine_with_flight(vec![Segment::straight(60.0, -1.25)], 60.0);
        wait_for(&engine, |status| status.landing.is_some());
        // No sample arrives any more: after 3 s by the engine's clock both disappear.
        let status = wait_for(&engine, |status| status.glide_ratio.is_none());
        assert_eq!(status.landing, None);
        assert!(status.gps.age_s.unwrap() > 3.0, "{:?}", status.gps.age_s);
        engine.stop();
    }

    #[test]
    fn engine_ends_when_the_channel_closes() {
        let (sender, receiver) = mpsc::channel::<SensorEvent>();
        let control = ToneControl::new(&VarioSettings::default());
        let engine =
            Engine::start(receiver, control, &VarioSettings::default(), None, None).unwrap();
        drop(sender);
        let status = wait_for(&engine, |status| !status.running && status.uptime_s > 0.0);
        assert!(!status.running);
    }
}
