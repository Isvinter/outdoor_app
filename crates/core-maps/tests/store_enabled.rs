#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::fs;
use std::path::Path;

use common::{fixtures, temp_dir};
use core_maps::{Layer, MapsError, RegionStore, catalog_location};

/// Installs a copy of the `mini` fixture (installing moves the package).
fn install_mini(store: &RegionStore, name: &str) {
    let package = temp_dir(&format!("{name}-pkg")).join("mini");
    copy_dir(&fixtures().join("mini"), &package);
    store.install_local(&package).unwrap();
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
    }
}

fn store(name: &str) -> (RegionStore, std::path::PathBuf) {
    let dir = temp_dir(name);
    let store = RegionStore::open(&dir).unwrap();
    install_mini(&store, name);
    (store, dir)
}

#[test]
fn disabled_region_serves_no_tiles() {
    let (store, _) = store("en-tiles");
    assert!(store.tile(Layer::Basemap, 0, 0, 0).unwrap().is_some());
    store.set_enabled("mini", false).unwrap();
    assert!(store.tile(Layer::Basemap, 0, 0, 0).unwrap().is_none());
    store.set_enabled("mini", true).unwrap();
    assert!(store.tile(Layer::Basemap, 0, 0, 0).unwrap().is_some());
}

#[test]
fn disabled_region_has_no_layer_and_elevation() {
    let (store, _) = store("en-layer");
    assert!(store.has_layer(Layer::Dem));
    assert!(store.elevation(47.575, 11.55).unwrap().is_some());
    store.set_enabled("mini", false).unwrap();
    assert!(!store.has_layer(Layer::Dem));
    assert_eq!(store.elevation(47.575, 11.55).unwrap(), None);
}

#[test]
fn disabled_state_survives_reopen() {
    let (store, dir) = store("en-reopen");
    assert!(store.installed()[0].enabled);
    store.set_enabled("mini", false).unwrap();
    drop(store);
    let store = RegionStore::open(&dir).unwrap();
    assert!(!store.installed()[0].enabled);
}

#[test]
fn broken_disabled_file_counts_as_empty() {
    let (store, dir) = store("en-broken");
    drop(store);
    fs::write(dir.join(".disabled.json"), "not json").unwrap();
    let store = RegionStore::open(&dir).unwrap();
    assert!(store.installed()[0].enabled);
}

#[test]
fn update_keeps_disabled() {
    let (store, _) = store("en-update");
    store.set_enabled("mini", false).unwrap();
    install_mini(&store, "en-update2");
    assert!(!store.installed()[0].enabled);
}

#[test]
fn delete_clears_disabled() {
    let (store, _) = store("en-delete");
    store.set_enabled("mini", false).unwrap();
    store.delete("mini").unwrap();
    install_mini(&store, "en-delete2");
    assert!(store.installed()[0].enabled);
}

#[test]
fn set_enabled_unknown_region_fails() {
    let (store, _) = store("en-unknown");
    assert!(matches!(
        store.set_enabled("nope", false),
        Err(MapsError::NotInstalled(_))
    ));
}

#[test]
fn package_file_only_lists_manifest_files() {
    let (store, dir) = store("en-files");
    let (path, len) = store.package_file("mini", "manifest.json").unwrap();
    assert_eq!(path, dir.join("mini").join("manifest.json"));
    assert_eq!(len, fs::metadata(&path).unwrap().len());
    assert!(store.package_file("mini", "basemap.pmtiles").is_some());
    for name in ["../manifest.json", "x.pmtiles", ".disabled.json", ""] {
        assert!(store.package_file("mini", name).is_none(), "{name}");
    }
    assert!(store.package_file("other", "manifest.json").is_none());
    // Disabled regions are still shared.
    store.set_enabled("mini", false).unwrap();
    assert!(store.package_file("mini", "basemap.pmtiles").is_some());
}

#[test]
fn incomplete_region_is_not_complete() {
    let (store, dir) = store("en-incomplete");
    assert!(store.is_complete("mini"));
    fs::remove_file(dir.join("mini").join("outdoor.pmtiles")).unwrap();
    assert!(!store.is_complete("mini"));
    assert!(store.package_file("mini", "outdoor.pmtiles").is_none());
    assert!(!store.is_complete("unknown"));
}

#[test]
fn catalog_location_joins() {
    assert_eq!(
        catalog_location("http://h:8765/", "routes.json"),
        "http://h:8765/routes.json"
    );
    assert_eq!(
        catalog_location("http://h:8765/catalog.json", "routes/x.json"),
        "http://h:8765/routes/x.json"
    );
}
