#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use core_storage::{Storage, StorageError, TrackState};
use core_track::{Activity, Segment, StatsParams, Track, TrackPoint, compute_stats};
use uuid::Uuid;

fn point(i: i64) -> TrackPoint {
    let mut p = TrackPoint::new(47.0 + i as f64 * 1e-4, 11.0);
    p.ele = Some(700.0 + i as f64);
    p.time_ms = Some(1_700_000_000_000 + i * 1000);
    p.h_accuracy_m = Some(4.5);
    p.speed_mps = Some(3.25);
    p
}

fn temp_db(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("core-storage-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("test.sqlite")
}

#[test]
fn new_database_is_migrated_to_the_current_schema() {
    let db = Storage::open_in_memory().unwrap();
    assert_eq!(
        db.schema_version().unwrap(),
        Storage::CURRENT_SCHEMA_VERSION
    );
    assert!(db.list_tracks().unwrap().is_empty());
}

#[test]
fn recorded_points_come_back_in_order_and_unchanged() {
    let mut db = Storage::open_in_memory().unwrap();
    let track = db
        .create_track(
            Some("Morning ride"),
            Activity::Bike,
            TrackState::Recording,
            1000,
        )
        .unwrap();
    let first = db.begin_segment(track, 1000).unwrap();
    for i in 0..3 {
        db.append_point(first, &point(i)).unwrap();
    }
    let second = db.begin_segment(track, 5000).unwrap();
    db.append_points(second, &[point(10), point(11)]).unwrap();

    let loaded = db.load_track(track).unwrap();
    assert_eq!(loaded.name.as_deref(), Some("Morning ride"));
    assert_eq!(loaded.activity, Some(Activity::Bike));
    assert_eq!(loaded.segments.len(), 2);
    assert_eq!(loaded.segments[0].points, [point(0), point(1), point(2)]);
    assert_eq!(loaded.segments[1].points, [point(10), point(11)]);
}

#[test]
fn track_record_reflects_state_changes_and_timestamps() {
    let mut db = Storage::open_in_memory().unwrap();
    let id = db
        .create_track(None, Activity::Hike, TrackState::Recording, 1000)
        .unwrap();
    let rec = db.track(id).unwrap();
    assert_eq!(
        (
            rec.state,
            rec.created_at_ms,
            rec.updated_at_ms,
            rec.ended_at_ms
        ),
        (TrackState::Recording, 1000, 1000, None)
    );

    db.set_track_state(id, TrackState::Paused, 2000).unwrap();
    assert_eq!(db.track(id).unwrap().state, TrackState::Paused);

    db.set_track_state(id, TrackState::Finished, 3000).unwrap();
    let rec = db.track(id).unwrap();
    assert_eq!(
        (rec.state, rec.updated_at_ms, rec.ended_at_ms),
        (TrackState::Finished, 3000, Some(3000))
    );
    assert!(rec.rev > 0);
}

#[test]
fn unfinished_recording_is_found_after_reopening_the_database() {
    let path = temp_db("reopen");
    let id = {
        let mut db = Storage::open(&path).unwrap();
        assert!(db.open_recording().unwrap().is_none());
        let id = db
            .create_track(None, Activity::Mtb, TrackState::Recording, 1000)
            .unwrap();
        let seg = db.begin_segment(id, 1000).unwrap();
        db.append_point(seg, &point(0)).unwrap();
        db.append_point(seg, &point(1)).unwrap();
        id
        // dropped without finishing: the app went away mid-recording
    };
    let mut db = Storage::open(&path).unwrap();
    let open = db.open_recording().unwrap().unwrap();
    assert_eq!(
        (open.id, open.state, open.activity),
        (id, TrackState::Recording, Activity::Mtb)
    );
    assert_eq!(db.load_track(id).unwrap().segments[0].points.len(), 2);

    db.set_track_state(id, TrackState::Finished, 9000).unwrap();
    assert!(db.open_recording().unwrap().is_none());
}

#[test]
fn stats_and_name_are_stored_with_the_track() {
    let mut db = Storage::open_in_memory().unwrap();
    let track = Track {
        name: Some("Imported".into()),
        activity: None,
        segments: vec![Segment {
            points: (0..50).map(point).collect(),
        }],
    };
    let stats = compute_stats(&track, &StatsParams::for_activity(Activity::Hike));
    let id = db
        .import_track(&track, Activity::Hike, &stats, 7000)
        .unwrap();

    let rec = db.track(id).unwrap();
    assert_eq!(rec.state, TrackState::Finished);
    assert_eq!(rec.stats.as_ref(), Some(&stats));
    assert_eq!(rec.started_at_ms, Some(1_700_000_000_000));
    assert_eq!(rec.ended_at_ms, Some(1_700_000_049_000));
    assert_eq!(db.load_track(id).unwrap().segments, track.segments);

    db.rename_track(id, "Renamed", 8000).unwrap();
    let rec = db.track(id).unwrap();
    assert_eq!(
        (rec.name.as_deref(), rec.updated_at_ms),
        (Some("Renamed"), 8000)
    );
}

#[test]
fn tracks_are_listed_newest_first() {
    let mut db = Storage::open_in_memory().unwrap();
    let old = db
        .create_track(Some("old"), Activity::Bike, TrackState::Finished, 1000)
        .unwrap();
    let new = db
        .create_track(Some("new"), Activity::Bike, TrackState::Finished, 2000)
        .unwrap();
    let ids: Vec<Uuid> = db.list_tracks().unwrap().iter().map(|t| t.id).collect();
    assert_eq!(ids, [new, old]);
}

#[test]
fn deleted_track_disappears_but_is_kept_as_tombstone() {
    let mut db = Storage::open_in_memory().unwrap();
    let id = db
        .create_track(None, Activity::Bike, TrackState::Finished, 1000)
        .unwrap();
    db.delete_track(id, 2000).unwrap();
    assert!(db.list_tracks().unwrap().is_empty());
    assert!(matches!(db.track(id), Err(StorageError::NotFound(missing)) if missing == id));
    assert!(matches!(db.load_track(id), Err(StorageError::NotFound(_))));
    assert_eq!(db.deleted_tracks().unwrap(), [(id, 2000)]);
}

#[test]
fn operations_on_unknown_ids_report_not_found() {
    let mut db = Storage::open_in_memory().unwrap();
    let unknown = Uuid::now_v7();
    assert!(matches!(
        db.set_track_state(unknown, TrackState::Paused, 1),
        Err(StorageError::NotFound(_))
    ));
    assert!(matches!(
        db.delete_track(unknown, 1),
        Err(StorageError::NotFound(_))
    ));
    assert!(matches!(
        db.begin_segment(unknown, 1),
        Err(StorageError::NotFound(_))
    ));
    assert!(matches!(
        db.append_point(unknown, &point(0)),
        Err(StorageError::NotFound(_))
    ));
}

#[test]
fn settings_can_be_stored_overwritten_and_survive_reopening() {
    let path = temp_db("settings");
    {
        let mut db = Storage::open(&path).unwrap();
        assert_eq!(db.setting("ui.language").unwrap(), None);
        db.set_setting("ui.language", "en", 1).unwrap();
        db.set_setting("ui.language", "de", 2).unwrap();
    }
    let db = Storage::open(&path).unwrap();
    assert_eq!(db.setting("ui.language").unwrap().as_deref(), Some("de"));
}

#[test]
fn database_from_a_newer_app_version_is_refused() {
    let path = temp_db("newer");
    drop(Storage::open(&path).unwrap());
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.pragma_update(None, "user_version", Storage::CURRENT_SCHEMA_VERSION + 1)
        .unwrap();
    drop(conn);
    assert!(matches!(
        Storage::open(&path),
        Err(StorageError::SchemaTooNew { found, supported })
            if found == Storage::CURRENT_SCHEMA_VERSION + 1 && supported == Storage::CURRENT_SCHEMA_VERSION
    ));
}

#[test]
fn routes_are_saved_listed_renamed_and_deleted() {
    let mut storage = Storage::open(&temp_db("routes")).unwrap();
    assert!(storage.list_routes().unwrap().is_empty());

    let first = storage
        .save_route(
            "Neureuth",
            "hike",
            3630.0,
            Some(520.0),
            "{\"plan\":1}",
            1_000,
        )
        .unwrap();
    let second = storage
        .save_route(
            "Achensee-Runde",
            "mtb",
            42_000.0,
            None,
            "{\"plan\":2}",
            2_000,
        )
        .unwrap();

    let routes = storage.list_routes().unwrap();
    assert_eq!(
        routes.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
        ["Achensee-Runde", "Neureuth"]
    );
    assert_eq!(routes[1].id, first);
    assert_eq!(
        (
            routes[1].profile.as_str(),
            routes[1].distance_m,
            routes[1].ascent_m
        ),
        ("hike", 3630.0, Some(520.0))
    );
    assert_eq!(routes[0].ascent_m, None);
    assert_eq!(routes[0].created_at_ms, 2_000);
    assert_eq!(storage.route_plan(first).unwrap(), "{\"plan\":1}");

    storage
        .rename_route(first, "Neureuth über Sommerweg", 3_000)
        .unwrap();
    assert_eq!(
        storage.list_routes().unwrap()[1].name,
        "Neureuth über Sommerweg"
    );
    assert_eq!(storage.list_routes().unwrap()[1].updated_at_ms, 3_000);

    storage.delete_route(second, 4_000).unwrap();
    assert_eq!(storage.list_routes().unwrap().len(), 1);
    assert!(matches!(
        storage.route_plan(second),
        Err(StorageError::NotFound(_))
    ));
    assert!(matches!(
        storage.delete_route(second, 5_000),
        Err(StorageError::NotFound(_))
    ));
    assert!(matches!(
        storage.rename_route(Uuid::now_v7(), "x", 5_000),
        Err(StorageError::NotFound(_))
    ));
}

#[test]
fn routes_survive_reopening_and_old_databases_gain_the_table() {
    let path = temp_db("routes-reopen");
    let id = {
        let mut storage = Storage::open(&path).unwrap();
        storage
            .save_route("Tour", "manual", 1.0, None, "{}", 1)
            .unwrap()
    };
    let storage = Storage::open(&path).unwrap();
    assert_eq!(storage.list_routes().unwrap()[0].id, id);
    assert_eq!(storage.schema_version().unwrap(), 2);
}

#[test]
fn replace_route_bumps_rev() {
    let mut storage = Storage::open(&temp_db("routes-replace")).unwrap();
    let id = storage
        .save_route("Tour", "hike", 1000.0, Some(10.0), "{\"v\":1}", 1_000)
        .unwrap();
    assert_eq!(storage.list_routes().unwrap()[0].rev, 1);

    storage
        .replace_route(id, "Tour 2", "hike+mtb", 2000.0, None, "{\"v\":2}", 2_000)
        .unwrap();

    let routes = storage.list_routes().unwrap();
    assert_eq!(routes.len(), 1);
    let route = &routes[0];
    assert_eq!(
        (
            route.id,
            route.name.as_str(),
            route.profile.as_str(),
            route.distance_m,
            route.ascent_m
        ),
        (id, "Tour 2", "hike+mtb", 2000.0, None)
    );
    assert_eq!(
        (route.created_at_ms, route.updated_at_ms, route.rev),
        (1_000, 2_000, 2)
    );
    assert_eq!(storage.route_plan(id).unwrap(), "{\"v\":2}");
}

#[test]
fn replace_deleted_route_is_not_found() {
    let mut storage = Storage::open(&temp_db("routes-replace-deleted")).unwrap();
    let id = storage
        .save_route("Tour", "hike", 1.0, None, "{}", 1)
        .unwrap();
    storage.delete_route(id, 2).unwrap();
    assert!(matches!(
        storage.replace_route(id, "x", "hike", 1.0, None, "{}", 3),
        Err(StorageError::NotFound(_))
    ));
    assert!(matches!(
        storage.replace_route(Uuid::now_v7(), "x", "hike", 1.0, None, "{}", 3),
        Err(StorageError::NotFound(_))
    ));
}
