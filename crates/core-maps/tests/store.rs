#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::atomic::{AtomicBool, Ordering};

use common::{FileServer, catalog_copy, fixtures, sha256_hex, temp_dir};
use core_maps::{Catalog, DefaultFetcher, Layer, MapsError, Progress, RegionManifest, RegionStore};

const OUTDOOR_TILE_SHA: &str = "7dc549f0820e10fa9f83b1568a43ea4c4d2d633fad83917f287875e8effc8b80";
const REGION_SIZE: u64 = 842_866;

fn source() -> String {
    fixtures().to_string_lossy().into_owned()
}

fn install(store: &RegionStore, source: &str) -> Result<(), MapsError> {
    store
        .install(
            source,
            "mini",
            &DefaultFetcher::new(),
            &mut |_| {},
            &AtomicBool::new(false),
        )
        .map(|_| ())
}

#[test]
fn catalog_is_read_from_a_directory() {
    let store = RegionStore::open(&temp_dir("catalog")).unwrap();
    let catalog = store.catalog(&source(), &DefaultFetcher::new()).unwrap();
    assert_eq!(catalog.regions.len(), 1);
    assert_eq!(catalog.regions[0].id, "mini");
    assert_eq!(catalog.regions[0].size, REGION_SIZE);
}

#[test]
fn catalog_source_may_name_the_catalog_file_or_use_a_file_url() {
    let store = RegionStore::open(&temp_dir("catalog-forms")).unwrap();
    for source in [
        format!("{}/catalog.json", source()),
        format!("file://{}/", source()),
    ] {
        let catalog = store.catalog(&source, &DefaultFetcher::new()).unwrap();
        assert_eq!(catalog.regions[0].id, "mini", "{source}");
    }
}

#[test]
fn missing_catalog_is_a_download_error() {
    let store = RegionStore::open(&temp_dir("no-catalog")).unwrap();
    let missing = temp_dir("no-catalog-source");
    let result = store.catalog(&missing.to_string_lossy(), &DefaultFetcher::new());
    assert!(
        matches!(result, Err(MapsError::Download { .. })),
        "{result:?}"
    );
}

#[test]
fn installed_region_serves_tiles() {
    let store = RegionStore::open(&temp_dir("install")).unwrap();
    assert!(
        store
            .tile(Layer::Outdoor, 13, 4358, 2862)
            .unwrap()
            .is_none()
    );

    install(&store, &source()).unwrap();

    let tile = store.tile(Layer::Outdoor, 13, 4358, 2862).unwrap().unwrap();
    assert_eq!(sha256_hex(&tile.data), OUTDOOR_TILE_SHA);
    assert_eq!(tile.mime, "application/x-protobuf");
    assert_eq!(
        store.tile(Layer::Dem, 2, 2, 1).unwrap().unwrap().mime,
        "image/webp"
    );
    assert!(store.tile(Layer::Basemap, 0, 0, 0).unwrap().is_some());
    // Somewhere else on the planet.
    assert!(store.tile(Layer::Outdoor, 13, 100, 100).unwrap().is_none());
}

#[test]
fn installed_region_is_listed_with_its_size() {
    let store = RegionStore::open(&temp_dir("listed")).unwrap();
    assert!(store.installed().is_empty());
    install(&store, &source()).unwrap();
    let installed = store.installed();
    assert_eq!(installed.len(), 1);
    assert_eq!(installed[0].manifest.id, "mini");
    assert_eq!(installed[0].manifest.version, "202611130000");
    assert_eq!(installed[0].bytes, REGION_SIZE);
}

#[test]
fn installed_regions_survive_reopening() {
    let dir = temp_dir("reopen");
    install(&RegionStore::open(&dir).unwrap(), &source()).unwrap();

    let store = RegionStore::open(&dir).unwrap();
    assert_eq!(store.installed().len(), 1);
    assert!(
        store
            .tile(Layer::Outdoor, 13, 4358, 2862)
            .unwrap()
            .is_some()
    );
}

#[test]
fn deleting_a_region_removes_tiles_and_files() {
    let dir = temp_dir("delete");
    let store = RegionStore::open(&dir).unwrap();
    install(&store, &source()).unwrap();

    store.delete("mini").unwrap();

    assert!(store.installed().is_empty());
    assert!(
        store
            .tile(Layer::Outdoor, 13, 4358, 2862)
            .unwrap()
            .is_none()
    );
    assert!(!dir.join("mini").exists());
    assert!(matches!(
        store.delete("mini"),
        Err(MapsError::NotInstalled(_))
    ));
}

#[test]
fn installing_again_replaces_the_region() {
    let store = RegionStore::open(&temp_dir("again")).unwrap();
    install(&store, &source()).unwrap();
    install(&store, &source()).unwrap();
    assert_eq!(store.installed().len(), 1);
    assert!(
        store
            .tile(Layer::Outdoor, 13, 4358, 2862)
            .unwrap()
            .is_some()
    );
}

#[test]
fn unknown_region_is_reported() {
    let store = RegionStore::open(&temp_dir("unknown")).unwrap();
    let result = store.install(
        &source(),
        "atlantis",
        &DefaultFetcher::new(),
        &mut |_| {},
        &AtomicBool::new(false),
    );
    assert!(matches!(result, Err(MapsError::UnknownRegion(id)) if id == "atlantis"));
}

#[test]
fn damaged_download_is_rejected_and_nothing_is_installed() {
    let catalog = catalog_copy("damaged-source");
    let victim = catalog.join("mini/outdoor.pmtiles");
    let mut bytes = std::fs::read(&victim).unwrap();
    let middle = bytes.len() / 2;
    bytes[middle] ^= 0xff;
    std::fs::write(&victim, bytes).unwrap();

    let dir = temp_dir("damaged");
    let store = RegionStore::open(&dir).unwrap();
    let result = install(&store, &catalog.to_string_lossy());

    assert!(
        matches!(result, Err(MapsError::Checksum { ref file, .. }) if file == "outdoor.pmtiles"),
        "{result:?}"
    );
    assert!(store.installed().is_empty());
    assert!(!dir.join("mini").exists());

    // Once the source is repaired the download succeeds.
    std::fs::copy(fixtures().join("mini/outdoor.pmtiles"), &victim).unwrap();
    install(&store, &catalog.to_string_lossy()).unwrap();
    assert_eq!(store.installed().len(), 1);
}

#[test]
fn download_over_http_reports_progress() {
    let server = FileServer::start(fixtures());
    let store = RegionStore::open(&temp_dir("http")).unwrap();
    let mut seen: Vec<Progress> = Vec::new();

    store
        .install(
            &server.url,
            "mini",
            &DefaultFetcher::new(),
            &mut |p| seen.push(p),
            &AtomicBool::new(false),
        )
        .unwrap();

    assert!(seen.len() >= 3, "{} progress reports", seen.len());
    assert!(
        seen.windows(2).all(|w| w[0].done <= w[1].done),
        "progress never goes backwards"
    );
    assert!(
        seen.iter()
            .all(|p| p.total == REGION_SIZE && p.region == "mini")
    );
    assert_eq!(seen.last().map(|p| p.done), Some(REGION_SIZE));
    let tile = store.tile(Layer::Outdoor, 13, 4358, 2862).unwrap().unwrap();
    assert_eq!(sha256_hex(&tile.data), OUTDOOR_TILE_SHA);
}

#[test]
fn cancelled_download_resumes_where_it_stopped() {
    let server = FileServer::start(fixtures());
    let dir = temp_dir("resume");
    let store = RegionStore::open(&dir).unwrap();
    let cancel = AtomicBool::new(false);

    let first = store.install(
        &server.url,
        "mini",
        &DefaultFetcher::new(),
        &mut |p| {
            if p.done >= 100_000 {
                cancel.store(true, Ordering::SeqCst);
            }
        },
        &cancel,
    );
    assert!(matches!(first, Err(MapsError::Cancelled)), "{first:?}");
    assert!(store.installed().is_empty());
    assert!(!dir.join("mini").exists());
    server.requests.lock().unwrap().clear();

    let mut start = None;
    store
        .install(
            &server.url,
            "mini",
            &DefaultFetcher::new(),
            &mut |p| {
                start.get_or_insert(p.done);
            },
            &AtomicBool::new(false),
        )
        .unwrap();

    assert!(
        start.unwrap() >= 100_000,
        "second run starts with the bytes already on disk"
    );
    let requests = server.requests.lock().unwrap().clone();
    let resumed: Vec<_> = requests
        .iter()
        .filter(|(_, range)| range.is_some_and(|start| start > 0))
        .collect();
    assert_eq!(resumed.len(), 1, "{requests:?}");
    assert_eq!(resumed[0].0, "/mini/basemap.pmtiles");
    let tile = store.tile(Layer::Outdoor, 13, 4358, 2862).unwrap().unwrap();
    assert_eq!(sha256_hex(&tile.data), OUTDOOR_TILE_SHA);
}

#[test]
fn region_cannot_be_downloaded_twice_at_once() {
    let server = FileServer::start(fixtures());
    let store = RegionStore::open(&temp_dir("busy")).unwrap();
    let mut nested = None;
    store
        .install(
            &server.url,
            "mini",
            &DefaultFetcher::new(),
            &mut |_| {
                if nested.is_none() {
                    nested = Some(install(&store, &server.url));
                }
            },
            &AtomicBool::new(false),
        )
        .unwrap();
    assert!(
        matches!(nested, Some(Err(MapsError::Busy(_)))),
        "{nested:?}"
    );
}

// --- descriptions -----------------------------------------------------------

fn manifest_json() -> String {
    std::fs::read_to_string(fixtures().join("mini/manifest.json")).unwrap()
}

#[test]
fn manifest_of_fixture_is_parsed() {
    let manifest = RegionManifest::parse(&manifest_json(), "test").unwrap();
    assert_eq!(manifest.id, "mini");
    assert_eq!(manifest.files.len(), 3);
    assert_eq!(manifest.size, REGION_SIZE);
    assert_eq!(manifest.bbox, [11.54, 47.57, 11.56, 47.58]);
}

#[test]
fn manifest_must_not_lead_outside_its_directory() {
    for (from, to) in [
        ("\"outdoor.pmtiles\"", "\"../outdoor.pmtiles\""),
        ("\"outdoor.pmtiles\"", "\"/etc/outdoor.pmtiles\""),
        ("\"id\": \"mini\"", "\"id\": \"../mini\""),
        ("\"id\": \"mini\"", "\"id\": \"\""),
        ("\"format\": 1", "\"format\": 2"),
    ] {
        let json = manifest_json().replacen(from, to, 1);
        assert_ne!(json, manifest_json(), "{from} not found");
        let result = RegionManifest::parse(&json, "test");
        assert!(
            matches!(result, Err(MapsError::InvalidManifest { .. })),
            "{to}: {result:?}"
        );
    }
}

#[test]
fn manifest_with_a_broken_checksum_is_rejected() {
    let manifest = RegionManifest::parse(&manifest_json(), "test").unwrap();
    let json = manifest_json().replacen(&manifest.files[0].sha256, "abc", 1);
    assert!(matches!(
        RegionManifest::parse(&json, "test"),
        Err(MapsError::InvalidManifest { .. })
    ));
}

#[test]
fn catalog_entries_must_stay_below_the_catalog() {
    let json = std::fs::read_to_string(fixtures().join("catalog.json")).unwrap();
    assert!(Catalog::parse(&json, "test").is_ok());
    let evil = json.replacen("\"path\": \"mini\"", "\"path\": \"../../mini\"", 1);
    assert_ne!(evil, json);
    assert!(matches!(
        Catalog::parse(&evil, "test"),
        Err(MapsError::InvalidManifest { .. })
    ));
    assert!(matches!(
        Catalog::parse("not json", "test"),
        Err(MapsError::InvalidManifest { .. })
    ));
}

#[test]
fn layer_names_round_trip() {
    assert_eq!("slope".parse::<Layer>().ok(), Some(Layer::Slope));
    assert_eq!(Layer::Slope.file_name(), "slope.pmtiles");
    for layer in Layer::ALL {
        assert_eq!(layer.as_str().parse::<Layer>().ok(), Some(layer));
    }
    assert!("secrets".parse::<Layer>().is_err());
}

#[test]
fn tile_paths_are_parsed() {
    use core_maps::TilePath;
    let path: TilePath = "/outdoor/13/4358/2862".parse().unwrap();
    assert_eq!(
        path,
        TilePath {
            layer: Layer::Outdoor,
            z: 13,
            x: 4358,
            y: 2862
        }
    );
    assert_eq!(
        "dem/2/2/1.webp".parse::<TilePath>().ok(),
        Some(TilePath {
            layer: Layer::Dem,
            z: 2,
            x: 2,
            y: 1
        })
    );
    for bad in [
        "",
        "/",
        "/outdoor/13/4358",
        "/outdoor/13/4358/2862/1",
        "/secrets/1/0/0",
        "/dem/x/0/0",
        "/dem/1/-1/0",
        "/dem/300/0/0",
    ] {
        assert!(bad.parse::<TilePath>().is_err(), "{bad:?}");
    }
}

#[test]
fn manifest_may_carry_routing_tiles_and_says_how_they_were_built() {
    let json = manifest_json().replacen(
        "\"files\": [",
        "\"routing\": { \"valhalla\": \"3.9.1\", \"max_mtb_scale\": 2 },\n  \"files\": [\n    { \"name\": \"routing.tar\", \"size\": 10, \"sha256\": \"0000000000000000000000000000000000000000000000000000000000000000\" },",
        1,
    );
    let manifest = RegionManifest::parse(&json, "test").unwrap();
    assert_eq!(manifest.files[0].name, "routing.tar");
    let routing = manifest.routing.unwrap();
    assert_eq!(
        (routing.valhalla.as_str(), routing.max_mtb_scale),
        ("3.9.1", 2)
    );
    assert_eq!(
        RegionManifest::parse(&manifest_json(), "test")
            .unwrap()
            .routing,
        None
    );

    let other = json.replacen("routing.tar", "secrets.tar", 1);
    assert!(matches!(
        RegionManifest::parse(&other, "test"),
        Err(MapsError::InvalidManifest { .. })
    ));
}

#[test]
fn routing_tiles_are_found_for_points_inside_one_region() {
    let dir = temp_dir("routing-tiles");
    let region = dir.join("tegernsee");
    std::fs::create_dir_all(&region).unwrap();
    std::fs::write(region.join("routing.tar"), b"tiles").unwrap();
    let manifest = serde_json::json!({
        "format": 1, "id": "tegernsee", "name": "Tegernsee", "bbox": [11.74, 47.70, 11.80, 47.74],
        "version": "1", "attribution": [], "size": 5,
        "routing": { "valhalla": "3.9.1", "max_mtb_scale": 2 },
        "files": [{ "name": "routing.tar", "size": 5, "sha256": "0".repeat(64) }],
    });
    std::fs::write(region.join("manifest.json"), manifest.to_string()).unwrap();
    let store = RegionStore::open(&dir).unwrap();

    let found = store
        .routing_tiles(&[(47.7128, 11.758), (47.7294, 11.7797)])
        .unwrap();
    assert_eq!(found.region, "tegernsee");
    assert_eq!(found.path, region.join("routing.tar"));
    assert_eq!(found.info.map(|info| info.max_mtb_scale), Some(2));
    assert!(
        store
            .routing_tiles(&[(47.7128, 11.758), (48.5, 12.5)])
            .is_none(),
        "one point outside"
    );
    assert!(store.routing_tiles(&[]).is_none());

    // A region without routing tiles does not offer any.
    let plain = RegionStore::open(&temp_dir("no-routing")).unwrap();
    install(&plain, &source()).unwrap();
    assert!(plain.routing_tiles(&[(47.575, 11.55)]).is_none());
}

#[test]
fn local_package_is_installed_and_moved() {
    let package = catalog_copy("local-pkg").join("mini");
    let store = RegionStore::open(&temp_dir("local")).unwrap();

    let region = store.install_local(&package).unwrap();

    assert_eq!(region.manifest.id, "mini");
    assert_eq!(region.bytes, REGION_SIZE);
    let tile = store.tile(Layer::Outdoor, 13, 4358, 2862).unwrap().unwrap();
    assert_eq!(sha256_hex(&tile.data), OUTDOOR_TILE_SHA);
    assert!(!package.exists(), "the package was moved, not copied");
}

#[test]
fn local_package_with_wrong_checksum_is_rejected() {
    use std::io::Write;
    let package = catalog_copy("local-bad-pkg").join("mini");
    std::fs::OpenOptions::new()
        .append(true)
        .open(package.join("basemap.pmtiles"))
        .unwrap()
        .write_all(b"x")
        .unwrap();
    let store = RegionStore::open(&temp_dir("local-bad")).unwrap();

    let result = store.install_local(&package);

    assert!(
        matches!(result, Err(MapsError::Checksum { .. })),
        "{result:?}"
    );
    assert!(store.installed().is_empty());
    assert!(package.join("basemap.pmtiles").is_file());
    assert!(package.join("manifest.json").is_file());
}

#[test]
fn local_package_replaces_installed_region() {
    let store = RegionStore::open(&temp_dir("local-replace")).unwrap();
    install(&store, &source()).unwrap();
    let package = catalog_copy("local-replace-pkg").join("mini");
    let manifest = std::fs::read_to_string(package.join("manifest.json")).unwrap();
    let old_version = store.installed()[0].manifest.version.clone();
    assert!(manifest.contains(&old_version));
    std::fs::write(
        package.join("manifest.json"),
        manifest.replace(&old_version, "209901010000"),
    )
    .unwrap();

    store.install_local(&package).unwrap();

    let installed = store.installed();
    assert_eq!(installed.len(), 1);
    assert_eq!(installed[0].manifest.version, "209901010000");
    assert!(store.tile(Layer::Basemap, 0, 0, 0).unwrap().is_some());
}

#[test]
fn local_install_ignores_leftovers_in_staging() {
    let dir = temp_dir("local-leftover");
    let staging = dir.join(".staging").join("mini");
    std::fs::create_dir_all(&staging).unwrap();
    std::fs::write(staging.join("junk.part"), b"junk").unwrap();
    let store = RegionStore::open(&dir).unwrap();

    store
        .install_local(&catalog_copy("local-leftover-pkg").join("mini"))
        .unwrap();

    assert!(!dir.join("mini").join("junk.part").exists());
    assert!(dir.join("mini").join("basemap.pmtiles").is_file());
}

#[test]
fn local_package_without_manifest_is_an_error() {
    let store = RegionStore::open(&temp_dir("local-empty")).unwrap();
    let result = store.install_local(&temp_dir("local-empty-pkg"));
    assert!(matches!(result, Err(MapsError::Io { .. })), "{result:?}");
}
