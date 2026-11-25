//! Searches the landing point on its own thread.
//!
//! The search walks up to 50 km of terrain and may take a while with a real elevation
//! model. Running it on the engine thread would delay the tone, so the engine only
//! hands the latest query over and picks up the latest result.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

use flight_core::landing::{LandingPoint, LandingQuery, find_landing};
use flight_core::terrain::{NoTerrain, Terrain};

use crate::lock;

/// State shared between the worker thread and its owner.
struct Shared {
    /// The query waiting to be searched; a new one replaces an old one (latest only).
    query: Mutex<Option<LandingQuery>>,
    /// A `Condvar` (condition variable) lets the thread sleep until `submit` wakes it,
    /// instead of checking the slot again and again.
    wake: Condvar,
    /// Bumped by `clear`, so a search that was running meanwhile throws its result away.
    generation: Mutex<u64>,
    result: Mutex<Option<LandingPoint>>,
    /// `Arc<dyn Terrain>`: a shared pointer to any type that implements `Terrain`.
    terrain: Mutex<Arc<dyn Terrain>>,
    stop: AtomicBool,
}

/// The landing search thread. Stops when the value is dropped.
pub(crate) struct LandingWorker {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl LandingWorker {
    pub fn start() -> io::Result<LandingWorker> {
        let shared = Arc::new(Shared {
            query: Mutex::new(None),
            wake: Condvar::new(),
            generation: Mutex::new(0),
            result: Mutex::new(None),
            terrain: Mutex::new(Arc::new(NoTerrain)),
            stop: AtomicBool::new(false),
        });
        let thread_shared = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("vario-landing".to_owned())
            .spawn(move || run(&thread_shared))?;
        Ok(LandingWorker {
            shared,
            thread: Some(thread),
        })
    }

    /// Replaces the ground the search runs against. Never waits for a running search:
    /// the search holds its own clone of the `Arc`, not the lock.
    pub fn set_terrain(&self, terrain: Arc<dyn Terrain>) {
        *lock(&self.shared.terrain) = terrain;
    }

    /// Hands a query over. One still waiting is replaced: only the latest counts.
    pub fn submit(&self, query: LandingQuery) {
        *lock(&self.shared.query) = Some(query);
        self.shared.wake.notify_one();
    }

    /// Drops the waiting query and the result, e.g. when the glide is hidden.
    pub fn clear(&self) {
        // Both under the query lock, so the thread either took the query before (and its
        // result will be discarded) or finds the slot empty.
        {
            let mut query = lock(&self.shared.query);
            *lock(&self.shared.generation) += 1;
            *query = None;
        }
        *lock(&self.shared.result) = None;
    }

    /// The latest landing point found, if any.
    pub fn result(&self) -> Option<LandingPoint> {
        *lock(&self.shared.result)
    }
}

impl Drop for LandingWorker {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        // Take the query lock while waking, so the thread cannot miss the wake-up
        // between checking `stop` and going to sleep.
        {
            let _query = lock(&self.shared.query);
            self.shared.wake.notify_one();
        }
        if let Some(thread) = self.thread.take() {
            // An error here means the thread panicked; there is nothing left to do.
            let _ = thread.join();
        }
    }
}

fn run(shared: &Shared) {
    loop {
        let (query, generation) = {
            let mut slot = lock(&shared.query);
            loop {
                if shared.stop.load(Ordering::Relaxed) {
                    return;
                }
                if let Some(query) = slot.take() {
                    break (query, *lock(&shared.generation));
                }
                // `wait` releases the lock while sleeping and takes it again on waking.
                slot = shared
                    .wake
                    .wait(slot)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
        };
        // Clone the `Arc` (cheap: only a counter goes up) and release the lock at once.
        let terrain = Arc::clone(&*lock(&shared.terrain));
        let point = find_landing(&query, terrain.as_ref());
        // Hold the generation lock while storing, so `clear` cannot slip in between.
        let current = lock(&shared.generation);
        if *current == generation {
            *lock(&shared.result) = point;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flight_core::landing::LandingQuery;
    use flight_core::terrain::FlatTerrain;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    #[test]
    fn finds_the_point_and_forgets_it_on_clear() {
        let worker = LandingWorker::start().unwrap();
        worker.set_terrain(Arc::new(FlatTerrain(600.0)));
        worker.submit(LandingQuery {
            latitude_deg: 47.665,
            longitude_deg: 11.526,
            altitude_m: 1600.0,
            course_deg: 90.0,
            glide_ratio: 8.0,
        });
        let deadline = Instant::now() + Duration::from_secs(2);
        let point = loop {
            if let Some(point) = worker.result() {
                break point;
            }
            assert!(Instant::now() < deadline, "no result in time");
            std::thread::sleep(Duration::from_millis(5));
        };
        assert!(
            (point.distance_m - 8000.0).abs() < 1.0,
            "distance {}",
            point.distance_m
        );

        worker.clear();
        assert_eq!(worker.result(), None);
        // Dropping joins the thread; the test would hang here if it did not stop.
        drop(worker);
    }
}
