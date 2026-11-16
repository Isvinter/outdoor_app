#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use core_storage::{RouteSync, Storage, SyncedRoute};
use uuid::Uuid;

fn synced(name: &str, source_rev: i64) -> SyncedRoute<'_> {
    SyncedRoute {
        name,
        profile: "hike",
        distance_m: 1000.0,
        ascent_m: Some(5.0),
        plan_json: "{\"v\":1}",
        source_rev,
    }
}

#[test]
fn migration_3_adds_source_rev() {
    let mut storage = Storage::open_in_memory().unwrap();
    assert_eq!(storage.schema_version().unwrap(), 3);
    let id = storage
        .save_route("Tour", "hike", 1.0, None, "{}", 1)
        .unwrap();
    assert_eq!(storage.route_sync(id).unwrap().unwrap().source_rev, None);
}

#[test]
fn put_synced_inserts_with_given_id() {
    let mut storage = Storage::open_in_memory().unwrap();
    let id = Uuid::now_v7();
    assert_eq!(storage.route_sync(id).unwrap(), None);
    storage
        .put_synced_route(id, &synced("Tour", 4), 1_000)
        .unwrap();
    assert_eq!(
        storage.route_sync(id).unwrap(),
        Some(RouteSync {
            rev: 1,
            synced_rev: 1,
            source_rev: Some(4),
            deleted: false
        })
    );
    assert!(storage.list_routes().unwrap().iter().any(|r| r.id == id));
}

#[test]
fn put_synced_replaces_and_undeletes() {
    let mut storage = Storage::open_in_memory().unwrap();
    let id = storage
        .save_route("Tour", "hike", 1.0, None, "{}", 1_000)
        .unwrap();
    let created = storage.list_routes().unwrap()[0].created_at_ms;
    storage.delete_route(id, 2_000).unwrap();
    let old = storage.route_sync(id).unwrap().unwrap();
    assert!(old.deleted);

    storage
        .put_synced_route(id, &synced("Desktop", 7), 3_000)
        .unwrap();
    let now = storage.route_sync(id).unwrap().unwrap();
    assert!(!now.deleted);
    assert_eq!(now.rev, old.rev + 1);
    assert_eq!(now.synced_rev, now.rev);
    assert_eq!(now.source_rev, Some(7));
    let route = &storage.list_routes().unwrap()[0];
    assert_eq!(route.created_at_ms, created);
    assert_eq!(route.name, "Desktop");
    assert_eq!(storage.route_plan(id).unwrap(), "{\"v\":1}");
}

#[test]
fn local_edit_breaks_sync() {
    let mut storage = Storage::open_in_memory().unwrap();
    let id = Uuid::now_v7();
    storage
        .put_synced_route(id, &synced("Tour", 1), 1_000)
        .unwrap();
    storage.rename_route(id, "Other", 2_000).unwrap();
    let sync = storage.route_sync(id).unwrap().unwrap();
    assert_eq!(sync.rev, sync.synced_rev + 1);
}
