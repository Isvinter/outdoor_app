//! On-device cache for the KK7 thermal map tiles. Tiles are fetched on demand,
//! kept on disk, refreshed after 30 days and replaced by a neighbouring layer
//! when nothing else is available (e.g. offline in the mountains).

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use tauri::{AppHandle, Manager};

use crate::thermal_layer::{LayerKey, fallbacks};

/// Tiles younger than this are served without asking the network.
const FRESH_FOR: Duration = Duration::from_secs(30 * 24 * 3600);
/// After a connection failure the network is skipped for this long.
const OFFLINE_BACKOFF: Duration = Duration::from_secs(30);
/// After an HTTP error a tile is skipped for this long.
const HTTP_BACKOFF: Duration = Duration::from_secs(10 * 60);
const MAX_ZOOM: u8 = 12;
const FETCHES_PER_SECOND: usize = 10;

#[derive(Debug, Clone, PartialEq)]
pub enum FetchError {
    /// Connection, DNS or timeout failure.
    Offline,
    Http(u16),
    Other(String),
}

/// Something that can download a URL. A trait, so tests can plug in a fake;
/// `Send + Sync` because several request threads share one fetcher.
pub trait Fetch: Send + Sync {
    fn get(&self, url: &str) -> Result<Vec<u8>, FetchError>;
}

/// Real downloads with ureq (the HTTP library the airspace update uses too).
pub struct UreqFetch {
    agent: ureq::Agent,
}

impl UreqFetch {
    pub fn new() -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(3)))
            .timeout_global(Some(Duration::from_secs(8)))
            .user_agent(format!("ZeroGrid/{} (Sky)", env!("CARGO_PKG_VERSION")))
            // Let `call` return 4xx/5xx as a response, so we can tell `Http(code)` apart.
            .http_status_as_error(false)
            .build()
            .into();
        Self { agent }
    }
}

impl Fetch for UreqFetch {
    fn get(&self, url: &str) -> Result<Vec<u8>, FetchError> {
        let mut response = self.agent.get(url).call().map_err(|error| match error {
            ureq::Error::Timeout(_)
            | ureq::Error::Io(_)
            | ureq::Error::HostNotFound
            | ureq::Error::ConnectionFailed => FetchError::Offline,
            other => FetchError::Other(other.to_string()),
        })?;
        let status = response.status().as_u16();
        if status >= 400 {
            return Err(FetchError::Http(status));
        }
        response
            .body_mut()
            .with_config()
            .limit(1024 * 1024)
            .read_to_vec()
            .map_err(|error| match error {
                ureq::Error::Timeout(_) | ureq::Error::Io(_) => FetchError::Offline,
                other => FetchError::Other(other.to_string()),
            })
    }
}

/// Smoke runs must not touch the network: every download counts as offline.
pub struct NoNetwork;

impl Fetch for NoNetwork {
    fn get(&self, _url: &str) -> Result<Vec<u8>, FetchError> {
        Err(FetchError::Offline)
    }
}

/// Tauri state holding the cache. It is created on the first tile request, because
/// the folder needs the `AppHandle`, which does not exist yet when the state is set up.
#[derive(Default)]
pub struct ThermalState {
    // `OnceLock`: set once, then shared; the first caller fills it.
    cache: OnceLock<Arc<ThermalCache>>,
}

impl ThermalState {
    /// The cache; `None` if the app data folder is unknown (then no tiles are served).
    pub fn get(&self, app: &AppHandle) -> Option<Arc<ThermalCache>> {
        if let Some(cache) = self.cache.get() {
            return Some(cache.clone());
        }
        // Not `data_dir()`: on iOS that is the Files-visible Documents folder.
        // Not the cache dir either: iOS may purge it.
        let dir = app.path().app_data_dir().ok()?.join("thermals");
        let fetch: Box<dyn Fetch> = if std::env::var_os("VARIO_SMOKE_TEST").is_some()
            && std::env::var_os("VARIO_SMOKE_KK7").is_none()
        {
            Box::new(NoNetwork)
        } else {
            Box::new(UreqFetch::new())
        };
        // If two threads race, one value wins and the other is dropped.
        Some(
            self.cache
                .get_or_init(|| Arc::new(ThermalCache::new(dir, fetch)))
                .clone(),
        )
    }
}

/// The KK7 tile URL. KK7 counts tile rows from the south (TMS), the map from the north.
pub fn kk7_url(key: LayerKey, z: u8, x: u32, y: u32) -> String {
    let flipped = (1u32 << z) - 1 - y;
    format!(
        "https://thermal.kk7.ch/tiles/thermals_{}/{z}/{x}/{flipped}.png?src=ZeroGrid",
        key.as_string()
    )
}

#[derive(Debug, PartialEq)]
pub struct ServedTile {
    pub data: Vec<u8>,
    /// True when the tile comes from another layer than the requested one.
    pub fallback: bool,
}

#[derive(Debug, PartialEq)]
pub enum Answer {
    Tile(ServedTile),
    Empty,
    BadRequest,
}

/// Sliding window: at most `FETCHES_PER_SECOND` fetch starts in any second.
/// Pure (the time is passed in), so it can be tested without waiting.
struct RateLimiter {
    starts: VecDeque<Instant>,
}

impl RateLimiter {
    fn new() -> Self {
        Self {
            starts: VecDeque::new(),
        }
    }

    /// How long the caller has to sleep before it may start a fetch; books the start.
    fn delay(&mut self, now: Instant) -> Duration {
        let second = Duration::from_secs(1);
        while let Some(&oldest) = self.starts.front() {
            if oldest + second <= now {
                self.starts.pop_front();
            } else {
                break;
            }
        }
        if self.starts.len() < FETCHES_PER_SECOND {
            self.starts.push_back(now);
            return Duration::ZERO;
        }
        // Book the slot one second after the 10th-most-recent start (not the oldest),
        // so a burst is spread out instead of piling up on one instant.
        let slot = self.starts[self.starts.len() - FETCHES_PER_SECOND] + second;
        let wait = slot.saturating_duration_since(now);
        self.starts.push_back(now + wait);
        wait
    }
}

/// Deadlines are derived from the `now` given to `answer`, so tests need no sleeping.
#[derive(Default)]
struct Backoff {
    offline_until: Option<SystemTime>,
    tile_until: HashMap<PathBuf, SystemTime>,
}

pub struct ThermalCache {
    dir: PathBuf,
    // `Box<dyn Fetch>`: an owned value of any type implementing `Fetch`,
    // chosen at run time (real HTTP in the app, a fake in tests).
    fetch: Box<dyn Fetch>,
    /// Tiles being fetched right now. Waiters sleep on `done` until their tile leaves it.
    in_flight: Mutex<HashSet<PathBuf>>,
    /// A `Condvar` lets a thread sleep until another thread says "something changed".
    done: Condvar,
    limiter: Mutex<RateLimiter>,
    backoff: Mutex<Backoff>,
    requests: AtomicU64,
    fetches: AtomicU64,
}

/// Locks a mutex; a panic in another thread must not block the cache forever.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// Marks a tile as in flight. Dropping it (also on early return or panic)
/// removes the mark and wakes all waiters, so none can hang.
struct InFlight<'a> {
    cache: &'a ThermalCache,
    path: PathBuf,
}

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        lock(&self.cache.in_flight).remove(&self.path);
        self.cache.done.notify_all();
    }
}

impl ThermalCache {
    pub fn new(dir: PathBuf, fetch: Box<dyn Fetch>) -> Self {
        Self {
            dir,
            fetch,
            in_flight: Mutex::new(HashSet::new()),
            done: Condvar::new(),
            limiter: Mutex::new(RateLimiter::new()),
            backoff: Mutex::new(Backoff::default()),
            requests: AtomicU64::new(0),
            fetches: AtomicU64::new(0),
        }
    }

    /// Calls of `answer`.
    // Only the smoke report (a later task) reads the counters.
    #[allow(dead_code)]
    pub fn requests(&self) -> u64 {
        self.requests.load(Ordering::Relaxed)
    }

    /// Calls of `Fetch::get`.
    #[allow(dead_code)]
    pub fn fetches(&self) -> u64 {
        self.fetches.load(Ordering::Relaxed)
    }

    /// `path` is `<key>/<z>/<x>/<y>`, the part after `/thermals/`.
    pub fn answer(&self, path: &str, now: SystemTime) -> Answer {
        self.requests.fetch_add(1, Ordering::Relaxed);
        let Some((key, z, x, y)) = parse_path(path) else {
            return Answer::BadRequest;
        };
        let file = self.tile_path(key, z, x, y);

        if let Some(data) = read_if_fresh(&file, now) {
            return served(data, false);
        }

        // Only one thread fetches a given tile; the others wait and read the result.
        let guard = {
            let mut busy = lock(&self.in_flight);
            loop {
                if is_fresh(&file, now) {
                    drop(busy);
                    if let Some(data) = read_if_fresh(&file, now) {
                        return served(data, false);
                    }
                    busy = lock(&self.in_flight);
                    continue;
                }
                if !busy.contains(&file) {
                    busy.insert(file.clone());
                    break InFlight {
                        cache: self,
                        path: file.clone(),
                    };
                }
                busy = self.done.wait(busy).unwrap_or_else(|e| e.into_inner());
            }
        };

        // No lock is held from here on, so other tiles are served meanwhile.
        if let Some(data) = self.try_fetch(key, z, x, y, &file, now) {
            drop(guard);
            return served(data, false);
        }
        drop(guard);

        // Fetch failed or skipped: stale tile of the same layer, then other layers.
        if let Ok(data) = std::fs::read(&file) {
            return served(data, false);
        }
        for other in fallbacks(key) {
            if let Ok(data) = std::fs::read(self.tile_path(other, z, x, y)) {
                return served(data, true);
            }
        }
        Answer::Empty
    }

    /// Total size of all stored tiles in bytes.
    pub fn size_bytes(&self) -> u64 {
        dir_size(&self.dir)
    }

    pub fn clear(&self) -> Result<(), String> {
        match std::fs::remove_dir_all(&self.dir) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }

    fn tile_path(&self, key: LayerKey, z: u8, x: u32, y: u32) -> PathBuf {
        self.dir
            .join(key.as_string())
            .join(z.to_string())
            .join(x.to_string())
            .join(format!("{y}.png"))
    }

    /// Downloads and stores the tile unless a backoff says to stay away from the network.
    fn try_fetch(
        &self,
        key: LayerKey,
        z: u8,
        x: u32,
        y: u32,
        file: &Path,
        now: SystemTime,
    ) -> Option<Vec<u8>> {
        {
            let backoff = lock(&self.backoff);
            if backoff.offline_until.is_some_and(|until| until > now)
                || backoff
                    .tile_until
                    .get(file)
                    .is_some_and(|until| *until > now)
            {
                return None;
            }
        }
        let wait = lock(&self.limiter).delay(Instant::now());
        if !wait.is_zero() {
            std::thread::sleep(wait);
        }
        self.fetches.fetch_add(1, Ordering::Relaxed);
        match self.fetch.get(&kk7_url(key, z, x, y)) {
            Ok(data) => {
                // A failed write is not fatal: the caller still gets the tile.
                let _ = store(file, &data);
                Some(data)
            }
            Err(error) => {
                let mut backoff = lock(&self.backoff);
                match error {
                    FetchError::Offline => backoff.offline_until = Some(now + OFFLINE_BACKOFF),
                    FetchError::Http(_) => {
                        backoff
                            .tile_until
                            .insert(file.to_path_buf(), now + HTTP_BACKOFF);
                    }
                    FetchError::Other(_) => {}
                }
                None
            }
        }
    }
}

fn served(data: Vec<u8>, fallback: bool) -> Answer {
    Answer::Tile(ServedTile { data, fallback })
}

fn parse_path(path: &str) -> Option<(LayerKey, u8, u32, u32)> {
    let parts: Vec<&str> = path.split('/').collect();
    let [key, z, x, y] = parts[..] else {
        return None;
    };
    let key = LayerKey::parse(key)?;
    let z: u8 = z.parse().ok()?;
    let x: u32 = x.parse().ok()?;
    let y: u32 = y.parse().ok()?;
    if z > MAX_ZOOM || x >= 1 << z || y >= 1 << z {
        return None;
    }
    Some((key, z, x, y))
}

fn is_fresh(file: &Path, now: SystemTime) -> bool {
    std::fs::metadata(file)
        .and_then(|meta| meta.modified())
        .is_ok_and(|mtime| now.duration_since(mtime).unwrap_or(Duration::ZERO) <= FRESH_FOR)
}

fn read_if_fresh(file: &Path, now: SystemTime) -> Option<Vec<u8>> {
    if is_fresh(file, now) {
        std::fs::read(file).ok()
    } else {
        None
    }
}

/// Writes to a temp file first, then renames, so readers never see half a tile.
fn store(file: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = file.with_extension("png.tmp");
    std::fs::write(&temp, data)?;
    std::fs::rename(&temp, file)
}

fn dir_size(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| match entry.metadata() {
            Ok(meta) if meta.is_dir() => dir_size(&entry.path()),
            Ok(meta) => meta.len(),
            Err(_) => 0,
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct Fake {
        calls: AtomicU64,
        reply: Mutex<Result<Vec<u8>, FetchError>>,
        delay: Duration,
    }

    impl Fake {
        fn new(reply: Result<Vec<u8>, FetchError>, delay: Duration) -> Arc<Fake> {
            Arc::new(Fake {
                calls: AtomicU64::new(0),
                reply: Mutex::new(reply),
                delay,
            })
        }
    }

    // The cache owns a `Box<dyn Fetch>`; tests keep a second handle to the fake via `Arc`.
    impl Fetch for Arc<Fake> {
        fn get(&self, _url: &str) -> Result<Vec<u8>, FetchError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(self.delay);
            lock(&self.reply).clone()
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("vario-thermal-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cache(name: &str, reply: Result<Vec<u8>, FetchError>) -> (ThermalCache, PathBuf) {
        let dir = temp_dir(name);
        let fake = Fake::new(reply, Duration::ZERO);
        (ThermalCache::new(dir.clone(), Box::new(fake)), dir)
    }

    fn put(dir: &Path, rel: &str, data: &[u8]) {
        let file = dir.join(rel);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, data).unwrap();
    }

    fn tile(data: &[u8], fallback: bool) -> Answer {
        served(data.to_vec(), fallback)
    }

    const PATH: &str = "jul_07/10/544/358";
    const FILE: &str = "jul_07/10/544/358.png";

    #[test]
    fn kk7_url_flips_y() {
        let key = LayerKey::parse("jul_07").unwrap();
        assert_eq!(
            kk7_url(key, 10, 544, 358),
            "https://thermal.kk7.ch/tiles/thermals_jul_07/10/544/665.png?src=ZeroGrid"
        );
    }

    #[test]
    fn fresh_tile_is_served_without_fetch() {
        let (c, dir) = cache("fresh", Err(FetchError::Offline));
        put(&dir, FILE, b"old");
        assert_eq!(c.answer(PATH, SystemTime::now()), tile(b"old", false));
        assert_eq!(c.fetches(), 0);
        assert_eq!(c.requests(), 1);
    }

    #[test]
    fn missing_tile_is_fetched_and_stored() {
        let (c, dir) = cache("missing", Ok(b"png".to_vec()));
        assert_eq!(c.answer(PATH, SystemTime::now()), tile(b"png", false));
        assert!(dir.join(FILE).exists());
        assert_eq!(c.fetches(), 1);
        c.answer(PATH, SystemTime::now());
        assert_eq!(c.fetches(), 1);
    }

    fn in_31_days() -> SystemTime {
        SystemTime::now() + Duration::from_secs(31 * 24 * 3600)
    }

    #[test]
    fn old_tile_is_refreshed_online() {
        let (c, dir) = cache("refresh", Ok(b"new".to_vec()));
        put(&dir, FILE, b"old");
        assert_eq!(c.answer(PATH, in_31_days()), tile(b"new", false));
        assert_eq!(c.fetches(), 1);
    }

    #[test]
    fn old_tile_is_kept_offline() {
        let (c, dir) = cache("keep", Err(FetchError::Offline));
        put(&dir, FILE, b"old");
        assert_eq!(c.answer(PATH, in_31_days()), tile(b"old", false));
    }

    #[test]
    fn fallback_layer_offline() {
        let (c, dir) = cache("fallback", Err(FetchError::Offline));
        put(&dir, "oct_10/10/544/358.png", b"evening");
        assert_eq!(
            c.answer("oct_07/10/544/358", SystemTime::now()),
            tile(b"evening", true)
        );
    }

    #[test]
    fn nothing_stored_offline_is_empty() {
        let (c, _dir) = cache("empty", Err(FetchError::Offline));
        assert_eq!(c.answer(PATH, SystemTime::now()), Answer::Empty);
    }

    #[test]
    fn offline_backoff_skips_network() {
        let (c, _dir) = cache("backoff", Err(FetchError::Offline));
        let now = SystemTime::now();
        c.answer("jul_07/10/544/358", now);
        c.answer("jul_07/10/545/358", now + Duration::from_secs(5));
        assert_eq!(c.fetches(), 1);
        c.answer("jul_07/10/546/358", now + Duration::from_secs(31));
        assert_eq!(c.fetches(), 2);
    }

    #[test]
    fn http_error_skips_only_that_tile() {
        let (c, _dir) = cache("http", Err(FetchError::Http(500)));
        let now = SystemTime::now();
        c.answer("jul_07/10/544/358", now);
        c.answer("jul_07/10/544/358", now + Duration::from_secs(60));
        assert_eq!(c.fetches(), 1);
        c.answer("jul_07/10/545/358", now + Duration::from_secs(60));
        assert_eq!(c.fetches(), 2);
        c.answer("jul_07/10/544/358", now + Duration::from_secs(601));
        assert_eq!(c.fetches(), 3);
    }

    #[test]
    fn empty_kk7_tile_is_stored() {
        let (c, dir) = cache("kk7empty", Ok(vec![7; 68]));
        assert_eq!(c.answer(PATH, SystemTime::now()), tile(&[7; 68], false));
        assert_eq!(std::fs::read(dir.join(FILE)).unwrap().len(), 68);
    }

    #[test]
    fn rejects_bad_keys_and_paths() {
        let (c, _dir) = cache("bad", Ok(b"x".to_vec()));
        for bad in [
            "../x/1/1/1",
            "jul_99/1/1/1",
            "jul_07/13/0/0",
            "jul_07/2/4/0",
            "jul_07/1/1",
        ] {
            assert_eq!(
                c.answer(bad, SystemTime::now()),
                Answer::BadRequest,
                "{bad}"
            );
        }
        assert_eq!(c.fetches(), 0);
    }

    #[test]
    fn rate_limiter_allows_ten_per_second() {
        let mut limiter = RateLimiter::new();
        let t0 = Instant::now();
        for _ in 0..10 {
            assert_eq!(limiter.delay(t0), Duration::ZERO);
        }
        let wait = limiter.delay(t0);
        assert!(wait > Duration::ZERO && wait <= Duration::from_secs(1));
        assert_eq!(limiter.delay(t0 + Duration::from_secs(1)), Duration::ZERO);
    }

    #[test]
    fn rate_limiter_spreads_a_burst() {
        let mut limiter = RateLimiter::new();
        let t0 = Instant::now();
        let starts: Vec<Instant> = (0..30).map(|_| t0 + limiter.delay(t0)).collect();
        for (i, start) in starts.iter().enumerate() {
            assert_eq!(
                *start,
                t0 + Duration::from_secs((i / 10) as u64),
                "call {i}"
            );
        }
        // No window of one second holds more than 10 starts.
        for start in &starts {
            let in_window = starts
                .iter()
                .filter(|s| **s >= *start && **s < *start + Duration::from_secs(1))
                .count();
            assert!(in_window <= 10);
        }
    }

    #[test]
    fn same_tile_is_fetched_once() {
        let dir = temp_dir("dedupe");
        let fake = Fake::new(Ok(b"png".to_vec()), Duration::from_millis(200));
        let cache = Arc::new(ThermalCache::new(dir, Box::new(fake)));
        let threads: Vec<_> = (0..4)
            .map(|_| {
                let cache = cache.clone();
                std::thread::spawn(move || cache.answer(PATH, SystemTime::now()))
            })
            .collect();
        for thread in threads {
            assert_eq!(thread.join().unwrap(), tile(b"png", false));
        }
        assert_eq!(cache.fetches(), 1);
    }

    #[test]
    fn failed_fetch_wakes_waiters() {
        let dir = temp_dir("failwake");
        let fake = Fake::new(
            Err(FetchError::Other("boom".into())),
            Duration::from_millis(200),
        );
        let cache = Arc::new(ThermalCache::new(dir, Box::new(fake)));
        let threads: Vec<_> = (0..3)
            .map(|_| {
                let cache = cache.clone();
                std::thread::spawn(move || cache.answer(PATH, SystemTime::now()))
            })
            .collect();
        for thread in threads {
            assert_eq!(thread.join().unwrap(), Answer::Empty);
        }
    }

    #[test]
    fn fetch_runs_outside_cache_lock() {
        let dir = temp_dir("outside");
        put(&dir, "jul_07/10/1/1.png", b"b");
        let fake = Fake::new(Ok(b"a".to_vec()), Duration::from_millis(500));
        let cache = Arc::new(ThermalCache::new(dir, Box::new(fake)));
        let slow = {
            let cache = cache.clone();
            std::thread::spawn(move || cache.answer(PATH, SystemTime::now()))
        };
        std::thread::sleep(Duration::from_millis(50));
        let started = Instant::now();
        assert_eq!(
            cache.answer("jul_07/10/1/1", SystemTime::now()),
            tile(b"b", false)
        );
        assert!(started.elapsed() < Duration::from_millis(100));
        slow.join().unwrap();
    }

    #[test]
    fn clear_removes_everything() {
        let (c, _dir) = cache("clear", Ok(b"png".to_vec()));
        c.answer(PATH, SystemTime::now());
        assert!(c.size_bytes() > 0);
        c.clear().unwrap();
        assert_eq!(c.size_bytes(), 0);
        c.answer(PATH, SystemTime::now());
        assert_eq!(c.fetches(), 2);
    }
}
