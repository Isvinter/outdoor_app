//! Desktop only: feeds a simulated flight into the sensor channel, in place of the
//! native sensors.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use flight_core::time::UtcDateTime;
use sensor_sim::clock::Pacer;
use sensor_sim::synthetic::{Segment, SyntheticFlight};
use sensor_sim::{SimConfig, Simulation};

pub struct Simulator {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Simulator {
    pub fn start() -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("vario-simulator".to_owned())
            .spawn(move || run(&thread_stop))
            .ok();
        Simulator { stop, thread }
    }
}

impl Drop for Simulator {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(stop: &AtomicBool) {
    let unix_now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64);
    let config = SimConfig {
        // Phone-like sensor noise.
        pressure_noise_pa: 3.0,
        accel_noise_mps2: 0.15,
        accel_bias_mps2: 0.15,
        ..SimConfig::default()
    };
    let mut simulation = Simulation::synthetic(
        long_flight(),
        UtcDateTime::from_unix_seconds(unix_now),
        &config,
    );

    let pacer = Pacer::start(simulation.clock.clone(), 1.0);
    while !stop.load(Ordering::Relaxed) && !simulation.finished() {
        pacer.update();
        while let Some(event) = simulation.sensors.next_event() {
            sensor_bridge::push(event);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// A whole flight of about 20 minutes, from launch to landing: 20 s standing at a
/// launch just east of the Brauneck summit near Lenggries (a public launch site), a
/// thermal, two rounds of glides, thermals and sink, a spiral down and a final glide
/// into the Isar valley south of Lenggries, then 60 s standing in the landing field.
/// All of it lies inside ZeroGrid Base's test map region, so the offline map and the
/// takeoff and landing detection can be tried on the desktop. A constant wind of
/// 4 m/s (about 14 km/h) from the west blows throughout.
///
/// The heights come from the test region's elevation tiles (`dem.pmtiles`, zoom 13),
/// looked up once with a throwaway program: ground 1470.9 m at the launch, 719.6 m
/// where the final glide (87.1 s) touches down at 47.6542 N 11.5703 E. The phone is
/// 1 m above the ground. Profile and wind were chosen so that the glider stays above
/// the terrain all the way.
fn long_flight() -> SyntheticFlight {
    let mut builder = SyntheticFlight::builder()
        .start(47.6640, 11.5270, 1471.9)
        .heading(90.0)
        .wind(270.0, 4.0)
        .segment(Segment::on_ground(20.0, 0.0))
        .segment(Segment::straight(30.0, -1.2))
        .segment(Segment::circling(200.0, 2.5, 20.0));
    for _ in 0..2 {
        builder = builder
            .segment(Segment::straight(30.0, -1.2))
            .segment(Segment::circling(40.0, 1.0, 20.0))
            .segment(Segment::circling(60.0, 3.0, 18.0))
            .segment(Segment::straight(30.0, -1.2))
            .segment(Segment::straight(20.0, -3.5))
            .segment(Segment::straight(88.0, -1.0));
    }
    builder
        .segment(Segment::circling(273.0, -4.0, 20.0))
        .segment(Segment::straight(87.1, -1.2))
        .segment(Segment::on_ground(60.0, 0.0))
        .build()
}
