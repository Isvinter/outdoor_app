use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use core_track::TrackPoint;

use crate::worker::{Worker, ensure_idle};
use crate::{Fix, FixSink, LocationConfig, LocationError, LocationSource, SourceStatus};

/// Simulated receiver that plays back a recorded track, for development and tests.
///
/// Fixes keep the spacing of the recording but are stamped relative to the
/// moment the replay starts. `speed` only changes how fast they are delivered,
/// so a track replayed at 20x still yields realistic statistics.
pub struct GpxReplaySource {
    points: Vec<(i64, TrackPoint)>,
    speed: f64,
    worker: Option<Worker>,
}

impl GpxReplaySource {
    /// `speed` is the playback rate: 1.0 is real time, `f64::INFINITY` delivers
    /// everything at once. Points without a timestamp are skipped.
    pub fn new(points: Vec<TrackPoint>, speed: f64) -> Self {
        let points = points
            .into_iter()
            .filter_map(|p| Some((p.time_ms?, p)))
            .collect();
        Self {
            points,
            speed: if speed > 0.0 { speed } else { 1.0 },
            worker: None,
        }
    }
}

impl LocationSource for GpxReplaySource {
    fn start(
        &mut self,
        config: LocationConfig,
        sink: Arc<dyn FixSink>,
    ) -> Result<(), LocationError> {
        ensure_idle(&mut self.worker)?;
        let points = self.points.clone();
        let speed = self.speed;
        self.worker = Some(Worker::spawn("gpx-replay", move |stop| {
            sink.on_status(SourceStatus::Active);
            let base_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
            let Some(&(first_ms, _)) = points.first() else {
                sink.on_status(SourceStatus::Finished);
                return;
            };
            let mut previous_ms = first_ms;
            let mut last_emitted_ms: Option<i64> = None;
            for (time_ms, point) in points {
                let delay_s = (time_ms - previous_ms).max(0) as f64 / 1000.0 / speed;
                previous_ms = time_ms;
                let stopped = if delay_s > 0.0 {
                    stop.wait(Duration::from_secs_f64(delay_s))
                } else {
                    stop.is_stopped()
                };
                if stopped {
                    return;
                }
                if last_emitted_ms
                    .is_some_and(|last| time_ms - last < i64::from(config.min_interval_ms))
                {
                    continue;
                }
                last_emitted_ms = Some(time_ms);
                sink.on_fix(Fix {
                    lat: point.lat,
                    lon: point.lon,
                    ele: point.ele,
                    time_ms: base_ms + (time_ms - first_ms),
                    h_accuracy_m: point.h_accuracy_m,
                    speed_mps: point.speed_mps,
                    course_deg: None,
                });
            }
            sink.on_status(SourceStatus::Finished);
        })?);
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(worker) = self.worker.take() {
            worker.stop();
        }
    }
}

impl Drop for GpxReplaySource {
    fn drop(&mut self) {
        self.stop();
    }
}
