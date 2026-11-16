#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use core_app::{
    App, AppListener, LegSpec, PlanRequest, RecordingStatus, SharedRoute, SharedRouteInfo,
};
use core_location::{Fix, SourceStatus};
use core_maps::{Catalog, CatalogEntry, DefaultFetcher, MapsError, RegionStore};
use core_routing::{LatLon, Profile, RouteOptions};
use core_share::{ShareContent, ShareOptions, ShareServer};
use uuid::Uuid;

const BIG_LEN: usize = 300 * 1024;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("core-share-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn big_bytes() -> Vec<u8> {
    (0..BIG_LEN).map(|i| (i * 7 % 251) as u8).collect()
}

fn mini_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/regions/mini")
}

/// Content of region `region`, whose files are those in `files` (name to path).
struct Fake {
    region: String,
    files: BTreeMap<String, PathBuf>,
    routes: Vec<SharedRoute>,
    /// Every `region_file` call, to see what got past the server.
    asked: Mutex<Vec<String>>,
}

impl ShareContent for Fake {
    fn catalog(&self) -> Catalog {
        Catalog {
            format: core_maps::FORMAT,
            regions: vec![CatalogEntry {
                id: self.region.clone(),
                name: "Test".into(),
                bbox: [11.54, 47.57, 11.56, 47.58],
                version: "202611130000".into(),
                size: 1,
                path: self.region.clone(),
            }],
        }
    }

    fn region_file(&self, id: &str, name: &str) -> Option<(PathBuf, u64)> {
        self.asked.lock().unwrap().push(format!("{id}/{name}"));
        if id != self.region {
            return None;
        }
        let path = self.files.get(name)?;
        Some((path.clone(), std::fs::metadata(path).ok()?.len()))
    }

    fn routes(&self) -> Result<Vec<SharedRouteInfo>, String> {
        Ok(self.routes.iter().map(|route| route.info.clone()).collect())
    }

    fn route(&self, id: Uuid) -> Option<SharedRoute> {
        self.routes
            .iter()
            .find(|route| route.info.id == id)
            .cloned()
    }
}

fn route(name: &str) -> SharedRoute {
    SharedRoute {
        info: SharedRouteInfo {
            id: Uuid::now_v7(),
            name: name.into(),
            profile: "manual".into(),
            distance_m: 1234.5,
            ascent_m: Some(12.0),
            updated_at_ms: 1_700_000_000_000,
            rev: 3,
        },
        plan: serde_json::json!({ "format": 1, "legs": [] }),
    }
}

/// A fake with region `r` holding `big.pmtiles` (300 KiB), and two routes.
fn big_fake(name: &str) -> Fake {
    let dir = temp_dir(name);
    let path = dir.join("big.pmtiles");
    std::fs::write(&path, big_bytes()).unwrap();
    Fake {
        region: "r".into(),
        files: BTreeMap::from([("big.pmtiles".to_string(), path)]),
        routes: vec![route("Eins"), route("Zwei")],
        asked: Mutex::default(),
    }
}

fn mini_fake() -> Fake {
    let dir = mini_dir();
    let mut files = BTreeMap::new();
    for name in [
        "manifest.json",
        "basemap.pmtiles",
        "outdoor.pmtiles",
        "dem.pmtiles",
    ] {
        files.insert(name.to_string(), dir.join(name));
    }
    Fake {
        region: "mini".into(),
        files,
        routes: Vec::new(),
        asked: Mutex::default(),
    }
}

fn start(content: impl ShareContent + 'static, options: ShareOptions) -> ShareServer {
    ShareServer::start("127.0.0.1:0".parse().unwrap(), Arc::new(content), options).unwrap()
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(20)))
        .build()
        .into()
}

fn url(server: &ShareServer, path: &str) -> String {
    format!("http://{}{path}", server.local_addr())
}

/// Sends `GET <path>` exactly as given (no normalisation) and returns the status.
fn raw_status(addr: SocketAddr, method: &str, path: &str) -> u16 {
    let mut stream = TcpStream::connect(addr).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut response = Vec::new();
    let _ = stream.read_to_end(&mut response);
    let text = String::from_utf8_lossy(&response);
    let status = text.split(' ').nth(1).unwrap_or_default();
    status
        .parse()
        .unwrap_or_else(|_| panic!("no status in {text:?}"))
}

#[test]
fn serves_catalog_and_routes() {
    let fake = big_fake("catalog");
    let first = fake.routes[0].clone();
    let server = start(fake, ShareOptions::default());
    let agent = agent();

    let response = agent.get(url(&server, "/catalog.json")).call().unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/json"
    );
    let catalog = Catalog::parse(&response.into_body().read_to_string().unwrap(), "x").unwrap();
    assert_eq!(catalog.regions[0].id, "r");

    let response = agent.get(url(&server, "/routes.json")).call().unwrap();
    assert_eq!(response.status(), 200);
    let list: Vec<SharedRouteInfo> =
        serde_json::from_str(&response.into_body().read_to_string().unwrap()).unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0], first.info);

    let path = format!("/routes/{}.json", first.info.id);
    let response = agent.get(url(&server, &path)).call().unwrap();
    assert_eq!(response.status(), 200);
    let got: SharedRoute =
        serde_json::from_str(&response.into_body().read_to_string().unwrap()).unwrap();
    assert_eq!(got, first);

    let path = format!("/routes/{}.json", Uuid::now_v7());
    let response = agent.get(url(&server, &path)).call().unwrap();
    assert_eq!(response.status(), 404);
}

#[test]
fn only_manifest_files_are_served() {
    let fake = Arc::new(big_fake("files"));
    let server = ShareServer::start(
        "127.0.0.1:0".parse().unwrap(),
        Arc::clone(&fake) as Arc<dyn ShareContent>,
        ShareOptions::default(),
    )
    .unwrap();
    let agent = agent();

    let response = agent.get(url(&server, "/r/big.pmtiles")).call().unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/octet-stream"
    );
    assert_eq!(
        response.headers().get("content-length").unwrap(),
        &BIG_LEN.to_string()
    );
    let body = response
        .into_body()
        .with_config()
        .limit(1 << 20)
        .read_to_vec()
        .unwrap();
    assert_eq!(body, big_bytes());

    // Well-formed, but not in the manifest.
    for path in ["/r/other", "/x/big.pmtiles", "/routes/nope.json"] {
        assert_eq!(raw_status(server.local_addr(), "GET", path), 404, "{path}");
    }
    fake.asked.lock().unwrap().clear();

    // Hostile or malformed: refused before the content is asked.
    for path in [
        "/r/../x",
        "/r/%2e%2e/x",
        "/r/%2E%2E/big.pmtiles",
        "/r/..",
        "/r/.",
        "/r/..big.pmtiles",
        "/r/big.pmtiles/",
        "//r/big.pmtiles",
        "/R/big.pmtiles",
        "/r/big.pmtiles?x=1",
        "/routes/../catalog.json",
        "/routes/../../etc.json",
        "/catalog.json?x=1",
        "/",
        "*",
        "http://x/r/big.pmtiles",
    ] {
        assert_eq!(raw_status(server.local_addr(), "GET", path), 404, "{path}");
    }
    assert!(fake.asked.lock().unwrap().is_empty(), "{:?}", fake.asked);
}

#[test]
fn post_is_rejected() {
    let server = start(big_fake("post"), ShareOptions::default());
    let response = agent()
        .post(url(&server, "/catalog.json"))
        .send("{}")
        .unwrap();
    assert_eq!(response.status(), 405);
    assert_eq!(
        raw_status(server.local_addr(), "PUT", "/r/big.pmtiles"),
        405
    );
    assert_eq!(raw_status(server.local_addr(), "DELETE", "/x"), 405);
}

#[test]
fn range_resumes() {
    let server = start(big_fake("range"), ShareOptions::default());
    let agent = agent();
    let get = |range: &str| {
        agent
            .get(url(&server, "/r/big.pmtiles"))
            .header("Range", range)
            .call()
            .unwrap()
    };

    let response = get("bytes=1000-");
    assert_eq!(response.status(), 206);
    assert_eq!(
        response.headers().get("content-range").unwrap(),
        "bytes 1000-307199/307200"
    );
    assert_eq!(
        response.headers().get("content-length").unwrap(),
        &(BIG_LEN - 1000).to_string()
    );
    let body = response
        .into_body()
        .with_config()
        .limit(1 << 20)
        .read_to_vec()
        .unwrap();
    assert_eq!(body, big_bytes()[1000..]);

    assert_eq!(get("bytes=307200-").status(), 416);
    assert_eq!(get("bytes=999999-").status(), 416);

    for range in [
        "bytes=0-99",
        "bytes=-100",
        "bytes=1-2,5-",
        "items=5-",
        "bytes=x-",
    ] {
        let response = get(range);
        assert_eq!(response.status(), 200, "{range}");
        let body = response
            .into_body()
            .with_config()
            .limit(1 << 20)
            .read_to_vec()
            .unwrap();
        assert_eq!(body.len(), BIG_LEN, "{range}");
    }
}

#[test]
fn resume_through_region_store() {
    let server = start(mini_fake(), ShareOptions::default());
    let source = url(&server, "/");
    let store = RegionStore::open(&temp_dir("store")).unwrap();
    let fetcher = DefaultFetcher::new();

    let cancel = AtomicBool::new(false);
    let mut cancelled_at = 0;
    let first = store.install(
        &source,
        "mini",
        &fetcher,
        &mut |progress| {
            if progress.done > 0 && progress.done < progress.total {
                cancelled_at = progress.done;
                cancel.store(true, Ordering::SeqCst);
            }
        },
        &cancel,
    );
    assert!(matches!(first, Err(MapsError::Cancelled)), "{first:?}");
    assert!(cancelled_at > 0);

    cancel.store(false, Ordering::SeqCst);
    let mut resumed_from = None;
    let region = store
        .install(
            &source,
            "mini",
            &fetcher,
            &mut |progress| {
                resumed_from.get_or_insert(progress.done);
            },
            &cancel,
        )
        .unwrap();

    // The second run started with the bytes of the first one on disk.
    assert_eq!(resumed_from, Some(cancelled_at));
    assert_eq!(region.manifest.id, "mini");
    assert!(store.is_complete("mini"));
}

#[test]
fn stop_ends_running_download() {
    let throttled = ShareOptions {
        throttle_bytes_per_s: Some(128 * 1024),
    };
    let server = start(big_fake("stop"), throttled);
    let addr = server.local_addr();
    let response = agent().get(url(&server, "/r/big.pmtiles")).call().unwrap();
    assert_eq!(response.status(), 200);
    let mut reader = response.into_body().into_reader();

    let mut head = vec![0u8; 64 * 1024];
    reader.read_exact(&mut head).unwrap();
    assert_eq!(head, big_bytes()[..head.len()]);

    server.stop();

    let mut rest = Vec::new();
    let result = reader.read_to_end(&mut rest);
    let total = head.len() + rest.len();
    assert!(
        result.is_err() || total < BIG_LEN - 64 * 1024,
        "read {total} bytes after stop: {result:?}"
    );

    assert!(
        agent()
            .get(format!("http://{addr}/catalog.json"))
            .call()
            .is_err()
    );
}

struct Listener;

impl AppListener for Listener {
    fn on_position(&self, _: &Fix) {}
    fn on_recording(&self, _: &RecordingStatus) {}
    fn on_source_status(&self, _: &SourceStatus) {}
    fn on_error(&self, _: &str) {}
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        std::fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
    }
}

#[test]
fn serves_app_content() {
    let dir = temp_dir("app");
    let app = App::open(&dir.join("outdoor.sqlite"), Arc::new(Listener)).unwrap();
    let package = dir.join("package");
    copy_dir(&mini_dir(), &package);
    app.install_local_region(&package).unwrap();
    let point = |lat, lon| LatLon { lat, lon };
    app.plan_route(&PlanRequest {
        waypoints: vec![point(47.572, 11.545), point(47.578, 11.555)],
        legs: vec![LegSpec {
            profile: Profile::Manual,
            options: RouteOptions::default(),
        }],
        stage_ends: Vec::new(),
    })
    .unwrap();
    let saved = app.save_planned_route("Runde").unwrap();

    let server = start(app, ShareOptions::default());
    let agent = agent();

    let body = agent
        .get(url(&server, "/catalog.json"))
        .call()
        .unwrap()
        .into_body()
        .read_to_string()
        .unwrap();
    let catalog = Catalog::parse(&body, "x").unwrap();
    assert_eq!(catalog.regions.len(), 1);
    assert_eq!(catalog.regions[0].id, "mini");

    let response = agent
        .get(url(&server, "/mini/manifest.json"))
        .call()
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/json"
    );
    assert_eq!(
        raw_status(server.local_addr(), "GET", "/mini/../outdoor.sqlite"),
        404
    );
    assert_eq!(
        raw_status(server.local_addr(), "GET", "/mini/%2e%2e/x"),
        404
    );

    let response = agent.get(url(&server, "/routes.json")).call().unwrap();
    let list: Vec<SharedRouteInfo> =
        serde_json::from_str(&response.into_body().read_to_string().unwrap()).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, saved.id);

    let path = format!("/routes/{}.json", saved.id);
    let response = agent.get(url(&server, &path)).call().unwrap();
    assert_eq!(response.status(), 200);
    let route: SharedRoute =
        serde_json::from_str(&response.into_body().read_to_string().unwrap()).unwrap();
    assert_eq!(route.info.name, "Runde");
    assert!(route.plan.get("format").is_some());
}
