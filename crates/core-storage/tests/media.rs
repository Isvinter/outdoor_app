#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use core_storage::{MediaKind, MediaSource, NewMedia, Storage, StorageError, TrackState};
use core_track::{Activity, SegmentReason, TrackPoint};
use uuid::Uuid;

fn temp_db(name: &str) -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("core-storage-media-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("test.sqlite")
}

fn track_with_segment(db: &mut Storage) -> (Uuid, Uuid) {
    let track = db
        .create_track(None, Activity::Bike, TrackState::Recording, 1000)
        .unwrap();
    let segment = db
        .begin_segment(track, Activity::Bike, SegmentReason::Start, 1000)
        .unwrap();
    db.append_point(segment, &TrackPoint::new(47.0, 11.0))
        .unwrap();
    (track, segment)
}

fn photo(track: Uuid, segment: Option<Uuid>, time_ms: Option<i64>) -> NewMedia<'static> {
    NewMedia {
        track_id: track,
        segment_id: segment,
        kind: MediaKind::Photo,
        source: MediaSource::Camera,
        time_ms,
        lat: Some(47.5),
        lon: Some(11.25),
        file: Some("media/a.jpg"),
        text: None,
    }
}

fn text_note(track: Uuid, time_ms: Option<i64>) -> NewMedia<'static> {
    NewMedia {
        track_id: track,
        segment_id: None,
        kind: MediaKind::Text,
        source: MediaSource::Typed,
        time_ms,
        lat: None,
        lon: None,
        file: None,
        text: Some("Gipfel"),
    }
}

#[test]
fn migration_5_opens_schema_4() {
    let path = temp_db("migration-5");
    let track = {
        let mut db = Storage::open(&path).unwrap();
        track_with_segment(&mut db).0
    };
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("DROP TABLE track_media;").unwrap();
    conn.pragma_update(None, "user_version", 4).unwrap();
    drop(conn);

    let db = Storage::open(&path).unwrap();
    assert_eq!(db.schema_version().unwrap(), 5);
    assert!(db.list_media(track).unwrap().is_empty());
}

#[test]
fn media_round_trip() {
    let mut db = Storage::open_in_memory().unwrap();
    let (track, segment) = track_with_segment(&mut db);
    let photo_id = db
        .add_media(&photo(track, Some(segment), Some(2000)), 5000)
        .unwrap();
    let note_id = db.add_media(&text_note(track, Some(3000)), 6000).unwrap();

    let media = db.list_media(track).unwrap();
    assert_eq!(media.len(), 2);
    let p = &media[0];
    assert_eq!(p.id, photo_id);
    assert_eq!(p.track_id, track);
    assert_eq!(p.segment_id, Some(segment));
    assert_eq!(p.kind, MediaKind::Photo);
    assert_eq!(p.source, MediaSource::Camera);
    assert_eq!(p.time_ms, Some(2000));
    assert_eq!((p.lat, p.lon), (Some(47.5), Some(11.25)));
    assert_eq!(p.file.as_deref(), Some("media/a.jpg"));
    assert_eq!(p.text, None);
    assert!(!p.transcribed);
    assert_eq!((p.created_at_ms, p.updated_at_ms, p.rev), (5000, 5000, 1));
    let n = &media[1];
    assert_eq!(n.id, note_id);
    assert_eq!(n.segment_id, None);
    assert_eq!(n.kind, MediaKind::Text);
    assert_eq!(n.source, MediaSource::Typed);
    assert_eq!(n.text.as_deref(), Some("Gipfel"));
    assert_eq!(db.media(note_id).unwrap(), *n);
}

#[test]
fn list_orders_by_time_untimed_last() {
    let mut db = Storage::open_in_memory().unwrap();
    let (track, _) = track_with_segment(&mut db);
    let untimed = db.add_media(&text_note(track, None), 1000).unwrap();
    let late = db.add_media(&text_note(track, Some(9000)), 2000).unwrap();
    let early = db.add_media(&text_note(track, Some(4000)), 3000).unwrap();
    let ids: Vec<Uuid> = db.list_media(track).unwrap().iter().map(|m| m.id).collect();
    assert_eq!(ids, [early, late, untimed]);
}

#[test]
fn segment_of_other_track_is_rejected() {
    let mut db = Storage::open_in_memory().unwrap();
    let (track, _) = track_with_segment(&mut db);
    let (_, other_segment) = track_with_segment(&mut db);
    let result = db.add_media(&photo(track, Some(other_segment), None), 1000);
    assert!(matches!(result, Err(StorageError::NotFound(_))));
    assert!(db.list_media(track).unwrap().is_empty());
}

#[test]
fn media_of_unknown_track_is_rejected() {
    let mut db = Storage::open_in_memory().unwrap();
    let result = db.add_media(&text_note(Uuid::now_v7(), None), 1000);
    assert!(matches!(result, Err(StorageError::NotFound(_))));
}

#[test]
fn set_media_text_bumps_rev() {
    let mut db = Storage::open_in_memory().unwrap();
    let (track, _) = track_with_segment(&mut db);
    let id = db.add_media(&text_note(track, None), 1000).unwrap();
    db.set_media_text(id, Some("Hütte"), true, 2000).unwrap();
    let m = db.media(id).unwrap();
    assert_eq!(m.text.as_deref(), Some("Hütte"));
    assert!(m.transcribed);
    assert_eq!((m.updated_at_ms, m.rev), (2000, 2));
    assert!(matches!(
        db.set_media_text(Uuid::now_v7(), None, false, 3000),
        Err(StorageError::NotFound(_))
    ));
}

#[test]
fn deleted_media_is_not_listed() {
    let mut db = Storage::open_in_memory().unwrap();
    let (track, _) = track_with_segment(&mut db);
    let id = db.add_media(&text_note(track, None), 1000).unwrap();
    db.delete_media(id, 2000).unwrap();
    assert!(db.list_media(track).unwrap().is_empty());
    assert!(matches!(db.media(id), Err(StorageError::NotFound(_))));
    assert!(matches!(
        db.delete_media(id, 3000),
        Err(StorageError::NotFound(_))
    ));
}

#[test]
fn deleting_track_deletes_its_media() {
    let path = temp_db("delete-track");
    let mut db = Storage::open(&path).unwrap();
    let (track, segment) = track_with_segment(&mut db);
    let id = db
        .add_media(&photo(track, Some(segment), None), 1000)
        .unwrap();
    db.delete_track(track, 2000).unwrap();
    assert!(matches!(db.media(id), Err(StorageError::NotFound(_))));
    let conn = rusqlite::Connection::open(&path).unwrap();
    let (deleted_at, rev): (Option<i64>, i64) = conn
        .query_row(
            "SELECT deleted_at, rev FROM track_media WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!((deleted_at, rev), (Some(2000), 2));
}

#[test]
fn segment_ids_follow_seq() {
    let mut db = Storage::open_in_memory().unwrap();
    let track = db
        .create_track(None, Activity::Bike, TrackState::Recording, 1000)
        .unwrap();
    let ids: Vec<Uuid> = (0..3)
        .map(|_| {
            db.begin_segment(track, Activity::Bike, SegmentReason::Start, 1000)
                .unwrap()
        })
        .collect();
    assert_eq!(db.segment_ids(track).unwrap(), ids);
}

#[test]
fn media_survives_section_activity_change() {
    let mut db = Storage::open_in_memory().unwrap();
    let (track, segment) = track_with_segment(&mut db);
    let id = db
        .add_media(&photo(track, Some(segment), None), 1000)
        .unwrap();
    db.set_segment_activity(track, 0, Activity::Hike, 2000)
        .unwrap();
    let media = db.list_media(track).unwrap();
    assert_eq!(media.len(), 1);
    assert_eq!(media[0].id, id);
    assert_eq!(media[0].segment_id, Some(segment));
}
