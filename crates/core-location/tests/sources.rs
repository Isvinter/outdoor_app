#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use core_location::{
    Fix, FixSink, GpsdSource, GpxReplaySource, LocationConfig, LocationSource, SourceStatus,
};
use core_track::TrackPoint;

#[derive(Debug, PartialEq)]
enum Event {
    Fix(Fix),
    Status(SourceStatus),
}

struct ChannelSink(Mutex<Sender<Event>>);

impl FixSink for ChannelSink {
    fn on_fix(&self, fix: Fix) {
        let _ = self.0.lock().unwrap().send(Event::Fix(fix));
    }
    fn on_status(&self, status: SourceStatus) {
        let _ = self.0.lock().unwrap().send(Event::Status(status));
    }
}

fn sink() -> (Arc<dyn FixSink>, Receiver<Event>) {
    let (tx, rx) = channel();
    (Arc::new(ChannelSink(Mutex::new(tx))), rx)
}

/// Collects events until `last` arrives (inclusive) or the timeout hits.
fn collect_until(rx: &Receiver<Event>, last: &Event) -> Vec<Event> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut events = Vec::new();
    while let Ok(e) = rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        let done = &e == last;
        events.push(e);
        if done {
            return events;
        }
    }
    panic!("timed out waiting for {last:?}; got {events:?}");
}

fn fixes(events: &[Event]) -> Vec<&Fix> {
    events
        .iter()
        .filter_map(|e| if let Event::Fix(f) = e { Some(f) } else { None })
        .collect()
}

fn points(n: i64, dt_ms: i64) -> Vec<TrackPoint> {
    (0..n)
        .map(|i| {
            let mut p = TrackPoint::new(47.0 + i as f64 * 1e-4, 11.0);
            p.ele = Some(700.0 + i as f64);
            p.time_ms = Some(1_000_000 + i * dt_ms);
            p
        })
        .collect()
}

const INSTANT: f64 = f64::INFINITY;

#[test]
fn replay_emits_every_point_with_original_spacing_rebased_to_now() {
    let (sink, rx) = sink();
    let mut source = GpxReplaySource::new(points(5, 2000), INSTANT);
    let before = now_ms();
    source.start(LocationConfig::default(), sink).unwrap();
    let events = collect_until(&rx, &Event::Status(SourceStatus::Finished));

    assert_eq!(events.first(), Some(&Event::Status(SourceStatus::Active)));
    let fixes = fixes(&events);
    assert_eq!(fixes.len(), 5);
    assert!(
        (before..=now_ms()).contains(&fixes[0].time_ms),
        "first fix is stamped with the start time"
    );
    for (i, f) in fixes.iter().enumerate() {
        assert_eq!(f.time_ms - fixes[0].time_ms, i as i64 * 2000);
        assert_eq!(
            (f.lat, f.ele),
            (47.0 + i as f64 * 1e-4, Some(700.0 + i as f64))
        );
    }
}

#[test]
fn replay_respects_the_minimum_interval() {
    let (sink, rx) = sink();
    let mut source = GpxReplaySource::new(points(10, 1000), INSTANT);
    source
        .start(
            LocationConfig {
                min_interval_ms: 3000,
            },
            sink,
        )
        .unwrap();
    let events = collect_until(&rx, &Event::Status(SourceStatus::Finished));
    let offsets: Vec<i64> = fixes(&events)
        .iter()
        .map(|f| f.time_ms - fixes(&events)[0].time_ms)
        .collect();
    assert_eq!(offsets, [0, 3000, 6000, 9000]);
}

#[test]
fn replay_paces_fixes_in_real_time_scaled_by_the_speed_factor() {
    let (sink, rx) = sink();
    // 4 points, 1 s apart, 20x speed: about 150 ms in total
    let mut source = GpxReplaySource::new(points(4, 1000), 20.0);
    let started = Instant::now();
    source.start(LocationConfig::default(), sink).unwrap();
    collect_until(&rx, &Event::Status(SourceStatus::Finished));
    let elapsed = started.elapsed();
    assert!(
        elapsed >= Duration::from_millis(140) && elapsed < Duration::from_millis(1000),
        "{elapsed:?}"
    );
}

#[test]
fn replay_stops_promptly_and_emits_nothing_afterwards() {
    let (sink, rx) = sink();
    let mut source = GpxReplaySource::new(points(100, 60_000), 1.0);
    source.start(LocationConfig::default(), sink).unwrap();
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(5)).unwrap(),
        Event::Status(SourceStatus::Active)
    );
    assert!(matches!(
        rx.recv_timeout(Duration::from_secs(5)).unwrap(),
        Event::Fix(_)
    ));
    let started = Instant::now();
    source.stop();
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "stop must not wait for the next fix"
    );
    assert!(rx.recv_timeout(Duration::from_millis(200)).is_err());
}

#[test]
fn replay_skips_points_without_time_and_can_be_restarted() {
    let mut pts = points(3, 1000);
    pts[1].time_ms = None;
    let mut source = GpxReplaySource::new(pts, INSTANT);
    for _ in 0..2 {
        let (sink, rx) = sink();
        source.start(LocationConfig::default(), sink).unwrap();
        let events = collect_until(&rx, &Event::Status(SourceStatus::Finished));
        assert_eq!(fixes(&events).len(), 2);
        source.stop();
    }
}

#[test]
fn starting_a_running_source_is_an_error() {
    let (sink_a, _rx_a) = sink();
    let (sink_b, _rx_b) = sink();
    let mut source = GpxReplaySource::new(points(100, 60_000), 1.0);
    source.start(LocationConfig::default(), sink_a).unwrap();
    assert!(source.start(LocationConfig::default(), sink_b).is_err());
    source.stop();
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

// --- gpsd ---------------------------------------------------------------

const GPSD_SESSION: &str = concat!(
    r#"{"class":"VERSION","release":"3.27.5","rev":"3.27.5","proto_major":3,"proto_minor":16}"#,
    "\n",
    r#"{"class":"DEVICES","devices":[{"class":"DEVICE","path":"/dev/ttyACM0"}]}"#,
    "\n",
    r#"{"class":"WATCH","enable":true,"json":true}"#,
    "\n",
    r#"{"class":"TPV","device":"/dev/ttyACM0","mode":1,"time":"2026-11-13T10:00:00.000Z"}"#,
    "\n",
    r#"{"class":"SKY","device":"/dev/ttyACM0","hdop":0.9,"satellites":[]}"#,
    "\n",
    r#"{"class":"TPV","device":"/dev/ttyACM0","mode":3,"time":"2026-11-13T10:00:01.000Z","lat":47.6812,"lon":11.5734,"altHAE":752.1,"altMSL":705.4,"epx":3.0,"epy":4.0,"eph":5.5,"track":182.5,"speed":4.25,"climb":0.1}"#,
    "\n",
    "this is not json\n",
    r#"{"class":"TPV","device":"/dev/ttyACM0","mode":2,"time":"2026-11-13T10:00:02.500Z","lat":47.6813,"lon":11.5735}"#,
    "\n",
    r#"{"class":"TPV","device":"/dev/ttyACM0","mode":3,"lat":47.6814,"lon":11.5736}"#,
    "\n",
);

/// A one-shot fake gpsd: serves `session` to the first client, then hangs up.
fn fake_gpsd(session: &'static str) -> (String, Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut command = String::new();
        BufReader::new(stream.try_clone().unwrap())
            .read_line(&mut command)
            .unwrap();
        tx.send(command).unwrap();
        stream.write_all(session.as_bytes()).unwrap();
    });
    (addr, rx)
}

#[test]
fn gpsd_source_subscribes_and_turns_position_reports_into_fixes() {
    let (addr, commands) = fake_gpsd(GPSD_SESSION);
    let (sink, rx) = sink();
    let mut source = GpsdSource::new(addr);
    source.start(LocationConfig::default(), sink).unwrap();

    assert_eq!(
        commands.recv_timeout(Duration::from_secs(5)).unwrap(),
        "?WATCH={\"enable\":true,\"json\":true}\n"
    );
    let events = collect_until(
        &rx,
        &Event::Status(SourceStatus::Lost("connection closed by gpsd".into())),
    );
    source.stop();

    assert_eq!(events[0], Event::Status(SourceStatus::Connecting));
    assert_eq!(events[1], Event::Status(SourceStatus::Active));
    // mode 1 (no fix), SKY, garbage and a report without time yield no fix
    assert_eq!(
        fixes(&events),
        [
            &Fix {
                lat: 47.6812,
                lon: 11.5734,
                ele: Some(705.4),
                time_ms: 1_794_564_001_000,
                h_accuracy_m: Some(5.5),
                speed_mps: Some(4.25),
                course_deg: Some(182.5),
            },
            &Fix {
                lat: 47.6813,
                lon: 11.5735,
                ele: None,
                time_ms: 1_794_564_002_500,
                h_accuracy_m: None,
                speed_mps: None,
                course_deg: None,
            },
        ]
    );
}

#[test]
fn gpsd_source_reports_when_the_daemon_is_unreachable_and_stops_promptly() {
    // a port nobody listens on
    let addr = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .to_string();
    let (sink, rx) = sink();
    let mut source = GpsdSource::new(addr);
    source.start(LocationConfig::default(), sink).unwrap();
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(5)).unwrap(),
        Event::Status(SourceStatus::Connecting)
    );
    assert!(matches!(
        rx.recv_timeout(Duration::from_secs(5)).unwrap(),
        Event::Status(SourceStatus::Lost(_))
    ));
    let started = Instant::now();
    source.stop();
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "stop must interrupt the retry delay"
    );
}

#[test]
fn fix_converts_to_a_track_point() {
    let fix = Fix {
        lat: 1.0,
        lon: 2.0,
        ele: Some(3.0),
        time_ms: 4,
        h_accuracy_m: Some(5.0),
        speed_mps: Some(6.0),
        course_deg: Some(7.0),
    };
    let p = fix.to_track_point();
    assert_eq!(
        (p.lat, p.lon, p.ele, p.time_ms, p.h_accuracy_m, p.speed_mps),
        (1.0, 2.0, Some(3.0), Some(4), Some(5.0), Some(6.0))
    );
}
