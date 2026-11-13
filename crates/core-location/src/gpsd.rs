use std::io::{BufRead, BufReader, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde::Deserialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::worker::{StopSignal, Worker, ensure_idle};
use crate::{Fix, FixSink, LocationConfig, LocationError, LocationSource, SourceStatus};

const WATCH_COMMAND: &[u8] = b"?WATCH={\"enable\":true,\"json\":true}\n";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const FIRST_RETRY_DELAY: Duration = Duration::from_secs(2);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(30);

/// Receiver connected through the gpsd daemon (Linux), using its JSON protocol.
///
/// gpsd streams reports once subscribed, so the source blocks on the socket
/// instead of polling. If the daemon is unreachable or the connection drops,
/// the source reports [`SourceStatus::Lost`] and retries with growing delays.
pub struct GpsdSource {
    address: String,
    worker: Option<Worker>,
    shared: Arc<Shared>,
}

/// State shared with the worker thread so `stop` can interrupt a blocking read.
#[derive(Default)]
struct Shared {
    stopped: AtomicBool,
    stream: Mutex<Option<TcpStream>>,
}

impl GpsdSource {
    /// `address` is `host:port`; gpsd listens on `localhost:2947` by default.
    pub fn new(address: impl Into<String>) -> Self {
        Self {
            address: address.into(),
            worker: None,
            shared: Arc::default(),
        }
    }
}

impl LocationSource for GpsdSource {
    fn start(
        &mut self,
        config: LocationConfig,
        sink: Arc<dyn FixSink>,
    ) -> Result<(), LocationError> {
        ensure_idle(&mut self.worker)?;
        self.shared = Arc::default();
        let shared = Arc::clone(&self.shared);
        let address = self.address.clone();
        self.worker = Some(Worker::spawn("gpsd", move |stop| {
            run(&address, config, &*sink, &shared, &stop)
        })?);
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(worker) = self.worker.take() {
            self.shared.stopped.store(true, Ordering::SeqCst);
            if let Some(stream) = self
                .shared
                .stream
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .as_ref()
            {
                let _ = stream.shutdown(Shutdown::Both);
            }
            worker.stop();
        }
    }
}

impl Drop for GpsdSource {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run(
    address: &str,
    config: LocationConfig,
    sink: &dyn FixSink,
    shared: &Shared,
    stop: &StopSignal,
) {
    let mut retry_delay = FIRST_RETRY_DELAY;
    loop {
        sink.on_status(SourceStatus::Connecting);
        let reason = match connect(address) {
            Ok(stream) => {
                retry_delay = FIRST_RETRY_DELAY;
                read_reports(stream, config, sink, shared)
            }
            Err(e) => format!("cannot connect to gpsd at {address}: {e}"),
        };
        if shared.stopped.load(Ordering::SeqCst) {
            return;
        }
        sink.on_status(SourceStatus::Lost(reason));
        if stop.wait(retry_delay) {
            return;
        }
        retry_delay = (retry_delay * 2).min(MAX_RETRY_DELAY);
    }
}

fn connect(address: &str) -> std::io::Result<TcpStream> {
    let mut last_error = None;
    for socket_address in address.to_socket_addrs()? {
        match TcpStream::connect_timeout(&socket_address, CONNECT_TIMEOUT) {
            Ok(stream) => return Ok(stream),
            Err(e) => last_error = Some(e),
        }
    }
    Err(last_error.unwrap_or_else(|| std::io::Error::other("address did not resolve")))
}

/// Subscribes and forwards fixes until the connection ends. Returns why it ended.
fn read_reports(
    mut stream: TcpStream,
    config: LocationConfig,
    sink: &dyn FixSink,
    shared: &Shared,
) -> String {
    match stream.try_clone() {
        Ok(clone) => *shared.stream.lock().unwrap_or_else(PoisonError::into_inner) = Some(clone),
        Err(e) => return e.to_string(),
    }
    // `stop` may have run before the stream was registered above.
    if shared.stopped.load(Ordering::SeqCst) {
        return String::new();
    }
    if let Err(e) = stream.write_all(WATCH_COMMAND) {
        return e.to_string();
    }
    sink.on_status(SourceStatus::Active);

    let mut last_emitted_ms: Option<i64> = None;
    for line in BufReader::new(stream).lines() {
        let line = match line {
            Ok(line) => line,
            Err(e) => return e.to_string(),
        };
        let Some(fix) = parse_report(&line) else {
            continue;
        };
        if last_emitted_ms
            .is_some_and(|last| fix.time_ms - last < i64::from(config.min_interval_ms))
        {
            continue;
        }
        last_emitted_ms = Some(fix.time_ms);
        sink.on_fix(fix);
    }
    "connection closed by gpsd".to_owned()
}

/// The fields of a gpsd `TPV` (time-position-velocity) report that are used.
#[derive(Deserialize)]
struct Report {
    class: String,
    /// 0/1 = no fix, 2 = 2D, 3 = 3D
    mode: Option<u8>,
    time: Option<String>,
    lat: Option<f64>,
    lon: Option<f64>,
    #[serde(rename = "altMSL")]
    alt_msl: Option<f64>,
    /// Altitude field of gpsd before 3.20.
    alt: Option<f64>,
    eph: Option<f32>,
    speed: Option<f32>,
    track: Option<f32>,
}

/// Turns one line from gpsd into a fix, if it is a position report with a fix.
fn parse_report(line: &str) -> Option<Fix> {
    let report: Report = serde_json::from_str(line).ok()?;
    if report.class != "TPV" || report.mode? < 2 {
        return None;
    }
    let time = OffsetDateTime::parse(report.time.as_deref()?, &Rfc3339).ok()?;
    Some(Fix {
        lat: report.lat?,
        lon: report.lon?,
        ele: report
            .alt_msl
            .or(report.alt)
            .filter(|_| report.mode >= Some(3)),
        time_ms: i64::try_from(time.unix_timestamp_nanos() / 1_000_000).ok()?,
        h_accuracy_m: report.eph,
        speed_mps: report.speed,
        course_deg: report.track,
    })
}
