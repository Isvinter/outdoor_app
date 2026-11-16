//! A small read-only HTTP/1.1 server on `std::net`.
//!
//! Only `GET` is answered, every response carries `Content-Length` and closes
//! the connection afterwards. Stopping the server shuts down the connections
//! of running responses, so a client sees its download end early and can
//! continue it later with `Range`.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use core_app::{App, SharedRoute, SharedRouteInfo};
use core_maps::Catalog;
use uuid::Uuid;

/// What the share hands out. Implemented by [`App`]; tests bring their own.
pub trait ShareContent: Send + Sync {
    fn catalog(&self) -> Catalog;
    /// A file of region `id` named in its manifest (or the manifest), with
    /// its length. Nothing else is ever served from the file system.
    fn region_file(&self, id: &str, name: &str) -> Option<(PathBuf, u64)>;
    fn routes(&self) -> Result<Vec<SharedRouteInfo>, String>;
    fn route(&self, id: Uuid) -> Option<SharedRoute>;
}

impl ShareContent for App {
    fn catalog(&self) -> Catalog {
        self.shared_catalog()
    }

    fn region_file(&self, id: &str, name: &str) -> Option<(PathBuf, u64)> {
        self.shared_region_file(id, name)
    }

    fn routes(&self) -> Result<Vec<SharedRouteInfo>, String> {
        self.shared_routes().map_err(|e| e.to_string())
    }

    fn route(&self, id: Uuid) -> Option<SharedRoute> {
        self.shared_route(id).ok()
    }
}

#[derive(Debug, Clone, Default)]
pub struct ShareOptions {
    /// Sends region files no faster than this; only for tests, which need a
    /// download that is reliably still running when they interrupt it.
    pub throttle_bytes_per_s: Option<u64>,
}

const WORKERS: usize = 4;
/// Bodies are read and sent in pieces of this size; the stop flag is checked
/// before each one.
const CHUNK: usize = 64 << 10;
/// Requests with a longer head are refused.
const MAX_HEAD: usize = 16 << 10;
const WRITE_TIMEOUT: Duration = Duration::from_secs(60);
/// How often idle workers look for a new connection.
const ACCEPT_POLL: Duration = Duration::from_millis(100);

/// Overall time limits of one connection, so that a client trickling bytes
/// cannot hold a worker.
#[derive(Debug, Clone, Copy)]
struct Deadlines {
    /// For the whole request head, from accepting the connection.
    head: Duration,
    /// For reading what the client still sends after the response.
    finish: Duration,
}

impl Default for Deadlines {
    fn default() -> Self {
        Deadlines {
            head: Duration::from_secs(15),
            finish: Duration::from_secs(2),
        }
    }
}

/// The running share. Dropping it stops it, like [`ShareServer::stop`].
pub struct ShareServer {
    addr: SocketAddr,
    shared: Arc<Shared>,
    workers: Vec<JoinHandle<()>>,
}

struct Shared {
    listener: TcpListener,
    content: Arc<dyn ShareContent>,
    options: ShareOptions,
    deadlines: Deadlines,
    stop: AtomicBool,
    /// Connections being answered, to shut them down on stop.
    active: Mutex<HashMap<u64, TcpStream>>,
    next_id: AtomicU64,
}

impl ShareServer {
    /// Listens on `addr` and answers with four worker threads.
    pub fn start(
        addr: SocketAddr,
        content: Arc<dyn ShareContent>,
        options: ShareOptions,
    ) -> io::Result<ShareServer> {
        Self::start_with(addr, content, options, Deadlines::default())
    }

    fn start_with(
        addr: SocketAddr,
        content: Arc<dyn ShareContent>,
        options: ShareOptions,
        deadlines: Deadlines,
    ) -> io::Result<ShareServer> {
        let listener = TcpListener::bind(addr)?;
        // Workers poll, so that stopping never depends on waking a blocked
        // `accept`.
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let shared = Arc::new(Shared {
            listener,
            content,
            options,
            deadlines,
            stop: AtomicBool::new(false),
            active: Mutex::default(),
            next_id: AtomicU64::new(0),
        });
        let mut server = ShareServer {
            addr,
            shared,
            workers: Vec::with_capacity(WORKERS),
        };
        for n in 0..WORKERS {
            let shared = Arc::clone(&server.shared);
            // On failure `server` is dropped, which stops the threads started so far.
            let worker = thread::Builder::new()
                .name(format!("share-{n}"))
                .spawn(move || shared.serve())?;
            server.workers.push(worker);
        }
        Ok(server)
    }

    /// The address the server listens on (with the actual port for port 0).
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// Stops the server: no new connections are accepted, running responses
    /// end early, and the worker threads are finished when this returns.
    pub fn stop(self) {
        // Drop does the work.
    }

    fn shutdown(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        for stream in self
            .shared
            .active
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
        {
            let _ = stream.shutdown(Shutdown::Both);
        }
        // Idle workers see the flag within one poll interval.
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
        // The listener closes when `shared` is dropped right after this.
    }
}

impl Drop for ShareServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Shared {
    fn stopped(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }

    fn serve(&self) {
        while !self.stopped() {
            let stream = match self.listener.accept() {
                Ok((stream, _)) => stream,
                // Nothing waiting, or e.g. too many open files: do not spin.
                Err(_) => {
                    thread::sleep(ACCEPT_POLL);
                    continue;
                }
            };
            // Accepted sockets inherit non-blocking mode on some systems.
            if stream.set_nonblocking(false).is_err() {
                continue;
            }
            if self.stopped() {
                break;
            }
            self.connection(stream);
        }
    }

    fn connection(&self, mut stream: TcpStream) {
        let accepted = Instant::now();
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let _registered = Registered::new(self, id, &stream);
        // Checked after registering: either `shutdown` sees this connection
        // or this sees the flag.
        if self.stopped() {
            return;
        }
        let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
        // A panic in the content (or here) ends only this connection; the
        // worker goes on, and `_registered` still removes the entry.
        let answered = catch_unwind(AssertUnwindSafe(|| {
            self.answer(&mut stream, accepted + self.deadlines.head)
        }));
        if matches!(answered, Ok(Ok(()))) && !self.stopped() {
            finish(&mut stream, Instant::now() + self.deadlines.finish);
        }
    }

    fn answer(&self, stream: &mut TcpStream, head_deadline: Instant) -> io::Result<()> {
        let Some(request) = read_request(stream, head_deadline)? else {
            return respond_empty(stream, 400, &[]);
        };
        if request.method != "GET" {
            return respond_empty(stream, 405, &[("Allow", "GET".into())]);
        }
        match route(&request.target) {
            Target::Catalog => respond_json(stream, &self.content.catalog()),
            Target::Routes => match self.content.routes() {
                Ok(routes) => respond_json(stream, &routes),
                Err(_) => respond_empty(stream, 500, &[]),
            },
            Target::Route(id) => match self.content.route(id) {
                Some(route) => respond_json(stream, &route),
                None => respond_empty(stream, 404, &[]),
            },
            Target::RegionFile(region, name) => match self.content.region_file(region, name) {
                Some((path, len)) => self.respond_file(stream, &request, name, &path, len),
                None => respond_empty(stream, 404, &[]),
            },
            Target::NotFound => respond_empty(stream, 404, &[]),
        }
    }

    fn respond_file(
        &self,
        stream: &mut TcpStream,
        request: &Request,
        name: &str,
        path: &std::path::Path,
        len: u64,
    ) -> io::Result<()> {
        let start = request.range_start();
        if start.is_some_and(|start| start >= len) {
            return respond_empty(stream, 416, &[("Content-Range", format!("bytes */{len}"))]);
        }
        let Ok(mut file) = File::open(path) else {
            return respond_empty(stream, 404, &[]);
        };
        let content_type = if name.ends_with(".json") {
            "application/json"
        } else {
            "application/octet-stream"
        };
        let mut headers = vec![
            ("Content-Type", content_type.to_string()),
            ("Accept-Ranges", "bytes".to_string()),
        ];
        let (status, from) = match start {
            Some(start) => {
                headers.push(("Content-Range", format!("bytes {start}-{}/{len}", len - 1)));
                (206, start)
            }
            None => (200, 0),
        };
        file.seek(SeekFrom::Start(from))?;
        write_head(stream, status, &headers, len - from)?;
        let mut body = StopReader {
            inner: file.take(len - from),
            stop: &self.stop,
            throttle: self.options.throttle_bytes_per_s.filter(|&rate| rate > 0),
            started: Instant::now(),
            sent: 0,
        };
        let mut buf = vec![0u8; CHUNK];
        let mut sent = 0;
        loop {
            let n = body.read(&mut buf)?;
            if n == 0 {
                break;
            }
            stream.write_all(&buf[..n])?;
            sent += n as u64;
        }
        if sent == len - from {
            Ok(())
        } else {
            // The file shrank under us; the client must not take this as complete.
            Err(io::Error::new(io::ErrorKind::UnexpectedEof, "file changed"))
        }
    }
}

/// Lets the client read the whole response before the connection closes:
/// closing with unread input would reset it and could drop the response.
fn finish(stream: &mut TcpStream, deadline: Instant) {
    let _ = stream.flush();
    let _ = stream.shutdown(Shutdown::Write);
    let mut sink = [0u8; 4096];
    let mut drained = 0;
    while drained < CHUNK {
        match read_until(stream, &mut sink, deadline) {
            Ok(0) | Err(_) => break,
            Ok(n) => drained += n,
        }
    }
}

/// Reads with the time left until `deadline` as the timeout; fails with
/// `TimedOut` once it has passed.
fn read_until(stream: &mut TcpStream, buf: &mut [u8], deadline: Instant) -> io::Result<usize> {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return Err(io::ErrorKind::TimedOut.into());
    }
    stream.set_read_timeout(Some(left))?;
    stream.read(buf)
}

/// Keeps a connection in `Shared::active` for as long as it lives, also
/// when answering panics.
struct Registered<'a> {
    shared: &'a Shared,
    id: u64,
}

impl<'a> Registered<'a> {
    fn new(shared: &'a Shared, id: u64, stream: &TcpStream) -> Self {
        if let Ok(clone) = stream.try_clone() {
            shared
                .active
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(id, clone);
        }
        Registered { shared, id }
    }
}

impl Drop for Registered<'_> {
    fn drop(&mut self) {
        self.shared
            .active
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.id);
    }
}

/// Reads at most [`CHUNK`] bytes at a time and fails once the share stops;
/// optionally no faster than `throttle` bytes per second.
struct StopReader<'a, R> {
    inner: R,
    stop: &'a AtomicBool,
    throttle: Option<u64>,
    started: Instant,
    sent: u64,
}

impl<R: Read> Read for StopReader<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let stopped = || io::Error::other("the share was stopped");
        if self.stop.load(Ordering::SeqCst) {
            return Err(stopped());
        }
        let mut max = buf.len().min(CHUNK);
        if let Some(rate) = self.throttle {
            // Small pieces keep the rate even.
            let piece = usize::try_from(rate / 10).unwrap_or(CHUNK).max(1024);
            max = max.min(piece);
            let due = Duration::from_secs_f64(self.sent as f64 / rate as f64);
            loop {
                let elapsed = self.started.elapsed();
                if elapsed >= due {
                    break;
                }
                if self.stop.load(Ordering::SeqCst) {
                    return Err(stopped());
                }
                thread::sleep((due - elapsed).min(Duration::from_millis(20)));
            }
        }
        let n = self.inner.read(&mut buf[..max])?;
        self.sent += n as u64;
        Ok(n)
    }
}

struct Request {
    method: String,
    target: String,
    range: Option<String>,
}

impl Request {
    /// The start of a `Range: bytes=<n>-` header. Other forms are ignored.
    fn range_start(&self) -> Option<u64> {
        let digits = self
            .range
            .as_deref()?
            .trim()
            .strip_prefix("bytes=")?
            .strip_suffix('-')?;
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        digits.parse().ok()
    }
}

/// Reads the request line and headers. `None` for a malformed request.
/// The whole head must arrive before `deadline`.
fn read_request(stream: &mut TcpStream, deadline: Instant) -> io::Result<Option<Request>> {
    let mut head = Vec::with_capacity(1024);
    let mut buf = [0u8; 2048];
    let end = loop {
        if head.len() > MAX_HEAD {
            return Ok(None);
        }
        let n = read_until(stream, &mut buf, deadline)?;
        if n == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        // Only the new bytes can complete the end of the head.
        let scan_from = head.len().saturating_sub(3);
        head.extend_from_slice(&buf[..n]);
        if let Some(at) = head[scan_from..].windows(4).position(|w| w == b"\r\n\r\n") {
            break scan_from + at;
        }
    };
    let Ok(head) = std::str::from_utf8(&head[..end]) else {
        return Ok(None);
    };
    let mut lines = head.split("\r\n");
    let mut parts = lines.next().unwrap_or_default().split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Ok(None);
    };
    if method.is_empty() || !version.starts_with("HTTP/1.") {
        return Ok(None);
    }
    let range = lines
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("range"))
        .map(|(_, value)| value.trim().to_string());
    Ok(Some(Request {
        method: method.to_string(),
        target: target.to_string(),
        range,
    }))
}

enum Target<'a> {
    Catalog,
    Routes,
    Route(Uuid),
    RegionFile(&'a str, &'a str),
    NotFound,
}

/// Maps the raw request target, before any percent-decoding, to what it
/// asks for. Besides the two fixed documents only exactly two plain segments
/// are accepted, so nothing can climb out of a region or encode its way out.
fn route(target: &str) -> Target<'_> {
    match target {
        "/catalog.json" => return Target::Catalog,
        "/routes.json" => return Target::Routes,
        _ => {}
    }
    let Some(rest) = target.strip_prefix('/') else {
        return Target::NotFound;
    };
    let mut segments = rest.split('/');
    let (Some(first), Some(second), None) = (segments.next(), segments.next(), segments.next())
    else {
        return Target::NotFound;
    };
    if !is_plain_segment(first) || !is_plain_segment(second) {
        return Target::NotFound;
    }
    if first == "routes" {
        return second
            .strip_suffix(".json")
            .and_then(|id| Uuid::try_parse(id).ok())
            .map_or(Target::NotFound, Target::Route);
    }
    Target::RegionFile(first, second)
}

/// Lower-case letters, digits, `-`, `_` and `.`, without `..`.
fn is_plain_segment(segment: &str) -> bool {
    let allowed =
        |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_' | b'.');
    !segment.is_empty() && segment != "." && !segment.contains("..") && segment.bytes().all(allowed)
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        206 => "Partial Content",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        416 => "Range Not Satisfiable",
        _ => "Internal Server Error",
    }
}

fn write_head(
    stream: &mut TcpStream,
    status: u16,
    headers: &[(&str, String)],
    len: u64,
) -> io::Result<()> {
    let mut head = format!("HTTP/1.1 {status} {}\r\n", reason(status));
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str(&format!(
        "Content-Length: {len}\r\nConnection: close\r\n\r\n"
    ));
    stream.write_all(head.as_bytes())
}

fn respond_empty(
    stream: &mut TcpStream,
    status: u16,
    headers: &[(&str, String)],
) -> io::Result<()> {
    write_head(stream, status, headers, 0)
}

fn respond_json(stream: &mut TcpStream, value: &impl serde::Serialize) -> io::Result<()> {
    let Ok(body) = serde_json::to_vec(value) else {
        return respond_empty(stream, 500, &[]);
    };
    write_head(
        stream,
        200,
        &[("Content-Type", "application/json".into())],
        body.len() as u64,
    )?;
    stream.write_all(&body)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    struct Empty;

    impl ShareContent for Empty {
        fn catalog(&self) -> Catalog {
            Catalog {
                format: core_maps::FORMAT,
                regions: Vec::new(),
            }
        }
        fn region_file(&self, _: &str, _: &str) -> Option<(PathBuf, u64)> {
            None
        }
        fn routes(&self) -> Result<Vec<SharedRouteInfo>, String> {
            Ok(Vec::new())
        }
        fn route(&self, _: Uuid) -> Option<SharedRoute> {
            None
        }
    }

    #[test]
    fn slow_head_is_dropped_after_the_deadline() {
        let deadlines = Deadlines {
            head: Duration::from_millis(400),
            finish: Duration::from_millis(400),
        };
        let server = ShareServer::start_with(
            "127.0.0.1:0".parse().unwrap(),
            Arc::new(Empty),
            ShareOptions::default(),
            deadlines,
        )
        .unwrap();
        let mut client = TcpStream::connect(server.local_addr()).unwrap();
        client
            .set_read_timeout(Some(Duration::from_millis(50)))
            .unwrap();
        let started = Instant::now();
        // One byte every 50 ms: each read on its own would be in time.
        let closed = loop {
            if started.elapsed() > Duration::from_secs(5) {
                break None;
            }
            let _ = client.write_all(b"G");
            match client.read(&mut [0u8; 64]) {
                Ok(0) => break Some(started.elapsed()),
                Ok(_) => panic!("answered a request that never ended"),
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) => {}
                Err(_) => break Some(started.elapsed()),
            }
        };
        let closed = closed.expect("the connection stayed open");
        assert!(closed >= Duration::from_millis(350), "{closed:?}");
        assert!(closed < Duration::from_secs(2), "{closed:?}");

        // The worker is free again.
        let mut next = TcpStream::connect(server.local_addr()).unwrap();
        next.write_all(b"GET /catalog.json HTTP/1.1\r\nHost: x\r\n\r\n")
            .unwrap();
        next.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut response = Vec::new();
        let _ = next.read_to_end(&mut response);
        assert!(response.starts_with(b"HTTP/1.1 200 OK"));
    }

    #[test]
    fn head_end_split_across_reads_is_found() {
        let server = ShareServer::start(
            "127.0.0.1:0".parse().unwrap(),
            Arc::new(Empty),
            ShareOptions::default(),
        )
        .unwrap();
        let mut client = TcpStream::connect(server.local_addr()).unwrap();
        for piece in ["GET /catalog.json HTTP/1.1\r\nHost: x\r", "\n\r", "\n"] {
            client.write_all(piece.as_bytes()).unwrap();
            client.flush().unwrap();
            thread::sleep(Duration::from_millis(30));
        }
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut response = Vec::new();
        let _ = client.read_to_end(&mut response);
        assert!(response.starts_with(b"HTTP/1.1 200 OK"));
    }
}
