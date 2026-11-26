#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{app_with_ramp, open, position, temp_dir};
use core_app::App;
use core_gpx::{GpxDocument, write_gpx};
use core_storage::TrackRecord;
use core_track::{Activity, Segment, Track, TrackPoint};

/// A hike 40 pixels east along the ramp (80 m up in the terrain) whose
/// receiver reports elevations around 500 m, jumping by 12 m every few points.
fn import_noisy_hike(app: &App) -> TrackRecord {
    let points = (0..=400)
        .map(|i| {
            let at = position(10.0 + f64::from(i) / 10.0, 32.0);
            TrackPoint {
                lat: at.lat,
                lon: at.lon,
                ele: Some(500.0 + if (i / 20) % 2 == 0 { 0.0 } else { 12.0 }),
                time_ms: Some(1_700_000_000_000 + i64::from(i) * 8000),
                h_accuracy_m: None,
                speed_mps: None,
            }
        })
        .collect();
    let track = Track {
        name: Some("Ramp".into()),
        activity: Some(Activity::Hike),
        segments: vec![Segment {
            points,
            ..Default::default()
        }],
    };
    let xml = write_gpx(&GpxDocument {
        name: None,
        waypoints: Vec::new(),
        tracks: vec![track],
    })
    .unwrap();
    app.import_gpx(&xml, None).unwrap().remove(0)
}

#[test]
fn track_detail_offers_statistics_with_terrain_elevation() {
    let app = app_with_ramp("track-dem");
    let record = import_noisy_hike(&app);
    let gps = record.stats.unwrap();
    assert!(
        gps.ascent_m > 100.0,
        "noise counts as ascent: {}",
        gps.ascent_m
    );

    let detail = app.track_detail(record.id).unwrap();

    let dem = detail.dem_stats.expect("terrain covers the track");
    assert!((dem.ascent_m - 80.0).abs() < 2.0, "ascent {}", dem.ascent_m);
    assert!(dem.descent_m < 1.0, "descent {}", dem.descent_m);
    assert!((dem.min_ele_m.unwrap() - 1020.0).abs() < 2.0);
    assert!((dem.max_ele_m.unwrap() - 1100.0).abs() < 2.0);
    // Everything but elevation is untouched.
    assert_eq!(dem.distance_m, gps.distance_m);
    assert_eq!(dem.moving_time_s, gps.moving_time_s);
    // The stored track keeps what the receiver measured.
    assert_eq!(detail.record.stats, Some(gps));
    assert!(app.export_gpx(record.id).unwrap().contains("<ele>512"));
}

#[test]
fn track_outside_of_the_terrain_model_has_no_terrain_statistics() {
    let app = open(&temp_dir("track-no-dem"));
    let record = import_noisy_hike(&app);
    assert_eq!(app.track_detail(record.id).unwrap().dem_stats, None);
}
