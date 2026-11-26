#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use core_gpx::{GpxDocument, GpxError, GpxWaypoint, parse_gpx, write_gpx};
use core_track::{Activity, Segment, Track, TrackPoint};

const FIXTURE: &str = include_str!("../../../fixtures/gpx/synthetic_two_segments.gpx");

fn wrap(body: &str) -> String {
    format!(
        r#"<?xml version="1.0"?><gpx version="1.1" xmlns="http://www.topografix.com/GPX/1/1">{body}</gpx>"#
    )
}

fn first_point(body: &str) -> TrackPoint {
    parse_gpx(&wrap(body)).unwrap().tracks[0].segments[0].points[0]
}

#[test]
fn parses_names_type_segments_and_points() {
    let doc = parse_gpx(FIXTURE).unwrap();
    assert_eq!(doc.name.as_deref(), Some("Synthetic fixture"));
    assert_eq!(doc.tracks.len(), 1);
    let trk = &doc.tracks[0];
    assert_eq!(trk.name.as_deref(), Some("Kranzberg & Ferchensee"));
    assert_eq!(trk.activity, Some(Activity::Hike));
    assert_eq!(
        trk.segments
            .iter()
            .map(|s| s.points.len())
            .collect::<Vec<_>>(),
        [3, 2]
    );
    let p = trk.segments[0].points[1];
    assert_eq!((p.lat, p.lon, p.ele), (47.4401, 11.25, Some(921.5)));
    assert_eq!(p.time_ms, Some(1_719_820_810_000));
}

#[test]
fn parses_fractional_seconds_and_utc_offsets() {
    let p = first_point(
        r#"<trk><trkseg><trkpt lat="1" lon="2"><time>2024-07-01T10:00:00.250+02:00</time></trkpt></trkseg></trk>"#,
    );
    assert_eq!(p.time_ms, Some(1_719_820_800_250));
}

#[test]
fn accepts_time_without_zone_as_utc() {
    let p = first_point(
        r#"<trk><trkseg><trkpt lat="1" lon="2"><time>2024-07-01T08:00:00</time></trkpt></trkseg></trk>"#,
    );
    assert_eq!(p.time_ms, Some(1_719_820_800_000));
}

#[test]
fn accepts_points_without_elevation_and_time_and_self_closing_points() {
    let doc = parse_gpx(&wrap(
        r#"<trk><trkseg><trkpt lat="47.5" lon="11.5"/><trkpt lon="11.6" lat="47.6"></trkpt></trkseg></trk>"#,
    ))
    .unwrap();
    let pts = &doc.tracks[0].segments[0].points;
    assert_eq!(pts.len(), 2);
    assert_eq!(pts[0], TrackPoint::new(47.5, 11.5));
    assert_eq!(pts[1], TrackPoint::new(47.6, 11.6));
}

#[test]
fn ignores_vendor_extensions_but_reads_speed() {
    let p = first_point(
        r#"<trk><extensions><color>red</color></extensions><trkseg>
           <trkpt lat="1" lon="2"><ele>10</ele>
             <extensions><gpxtpx:TrackPointExtension xmlns:gpxtpx="http://www.garmin.com/xmlschemas/TrackPointExtension/v2">
               <gpxtpx:hr>140</gpxtpx:hr><gpxtpx:speed>3.5</gpxtpx:speed>
             </gpxtpx:TrackPointExtension></extensions>
           </trkpt></trkseg></trk>"#,
    );
    assert_eq!(p.ele, Some(10.0));
    assert_eq!(p.speed_mps, Some(3.5));
}

#[test]
fn ignores_waypoints_and_routes() {
    let doc = parse_gpx(&wrap(
        r#"<wpt lat="1" lon="2"><name>Hut</name><ele>5</ele></wpt>
           <rte><name>R</name><rtept lat="1" lon="2"/></rte>
           <trk><name>T</name><trkseg><trkpt lat="3" lon="4"/></trkseg></trk>"#,
    ))
    .unwrap();
    assert_eq!(doc.name, None);
    assert_eq!(doc.tracks.len(), 1);
    assert_eq!(doc.tracks[0].name.as_deref(), Some("T"));
    assert_eq!(
        doc.tracks[0].segments[0].points,
        [TrackPoint::new(3.0, 4.0)]
    );
}

#[test]
fn maps_common_activity_names_and_leaves_unknown_ones_empty() {
    for (ty, expected) in [
        ("cycling", Some(Activity::Bike)),
        ("MTB", Some(Activity::Mtb)),
        ("Hiking", Some(Activity::Hike)),
        ("ski_tour", Some(Activity::SkiTour)),
        ("kayaking", None),
    ] {
        let doc = parse_gpx(&wrap(&format!("<trk><type>{ty}</type></trk>"))).unwrap();
        assert_eq!(doc.tracks[0].activity, expected, "{ty}");
    }
}

#[test]
fn accepts_cdata_and_byte_order_mark() {
    let xml = format!(
        "\u{feff}{}",
        wrap("<trk><name><![CDATA[A <b> & C]]></name></trk>")
    );
    assert_eq!(
        parse_gpx(&xml).unwrap().tracks[0].name.as_deref(),
        Some("A <b> & C")
    );
}

#[test]
fn rejects_point_without_coordinate() {
    let err = parse_gpx(&wrap(r#"<trk><trkseg><trkpt lat="1"/></trkseg></trk>"#)).unwrap_err();
    assert!(
        matches!(
            err,
            GpxError::MissingAttribute {
                attribute: "lon",
                line: 1
            }
        ),
        "{err:?}"
    );
}

#[test]
fn rejects_unparsable_numbers_and_times_with_line_number() {
    let err = parse_gpx(&wrap(
        "<trk><trkseg>\n<trkpt lat=\"abc\" lon=\"2\"/></trkseg></trk>",
    ))
    .unwrap_err();
    assert!(
        matches!(&err, GpxError::InvalidNumber { value, line: 2 } if value == "abc"),
        "{err:?}"
    );
    let err = parse_gpx(&wrap(
        "<trk><trkseg><trkpt lat=\"1\" lon=\"2\">\n\n<time>yesterday</time></trkpt></trkseg></trk>",
    ))
    .unwrap_err();
    assert!(
        matches!(&err, GpxError::InvalidTime { value, line: 3 } if value == "yesterday"),
        "{err:?}"
    );
}

#[test]
fn rejects_documents_that_are_not_gpx() {
    assert!(matches!(
        parse_gpx("<kml><trk/></kml>").unwrap_err(),
        GpxError::NotGpx
    ));
    assert!(matches!(parse_gpx("").unwrap_err(), GpxError::NotGpx));
    assert!(matches!(
        parse_gpx("just text").unwrap_err(),
        GpxError::NotGpx
    ));
}

#[test]
fn rejects_malformed_xml() {
    let err = parse_gpx(r#"<gpx><trk><trkseg><trkpt lat="1" lon="2"></trk></gpx>"#).unwrap_err();
    assert!(matches!(err, GpxError::Xml { .. }), "{err:?}");
    let err = parse_gpx("<gpx><trk>").unwrap_err();
    assert!(matches!(err, GpxError::Xml { .. }), "{err:?}");
}

#[test]
fn written_document_parses_back_to_the_same_content() {
    let doc = parse_gpx(FIXTURE).unwrap();
    let xml = write_gpx(&doc).unwrap();
    assert_eq!(parse_gpx(&xml).unwrap(), doc);
}

#[test]
fn writes_gpx_1_1_header_escaped_text_and_millisecond_times() {
    let mut p = TrackPoint::new(47.123456789, -11.5);
    p.ele = Some(1234.56);
    p.time_ms = Some(1_719_820_800_250);
    let doc = GpxDocument {
        name: None,
        waypoints: Vec::new(),
        tracks: vec![Track {
            name: Some("A < B & \"C\"".into()),
            activity: Some(Activity::SkiTour),
            segments: vec![Segment {
                points: vec![p, TrackPoint::new(0.0, 0.0)],
                ..Default::default()
            }],
        }],
    };
    let xml = write_gpx(&doc).unwrap();
    assert!(xml.starts_with(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<gpx version=\"1.1\" creator=\"ZeroGrid\""
    ));
    assert!(xml.contains("xmlns=\"http://www.topografix.com/GPX/1/1\""));
    assert!(xml.contains("<name>A &lt; B &amp; \"C\"</name>"));
    assert!(xml.contains("<type>ski_tour</type>"));
    assert!(xml.contains(
        "<trkpt lat=\"47.1234568\" lon=\"-11.5000000\"><ele>1234.6</ele><time>2024-07-01T08:00:00.25Z</time></trkpt>"
    ));
    assert!(xml.contains("<trkpt lat=\"0.0000000\" lon=\"0.0000000\"/>"));
}

#[test]
fn refuses_to_write_non_finite_coordinates() {
    let doc = GpxDocument {
        name: None,
        waypoints: Vec::new(),
        tracks: vec![Track {
            name: None,
            activity: None,
            segments: vec![Segment {
                points: vec![TrackPoint::new(f64::NAN, 1.0)],
                ..Default::default()
            }],
        }],
    };
    assert!(matches!(
        write_gpx(&doc).unwrap_err(),
        GpxError::InvalidCoordinate
    ));
}

#[test]
fn waypoints_are_written_before_tracks() {
    let doc = GpxDocument {
        name: None,
        waypoints: vec![GpxWaypoint {
            lat: 47.5,
            lon: 11.25,
            ele: Some(812.34),
            name: Some("Etappe 1 Ende & mehr".into()),
        }],
        tracks: vec![Track {
            name: None,
            activity: None,
            segments: vec![Segment {
                points: vec![TrackPoint {
                    lat: 47.5,
                    lon: 11.25,
                    ele: None,
                    time_ms: None,
                    h_accuracy_m: None,
                    speed_mps: None,
                }],
                ..Default::default()
            }],
        }],
    };

    let xml = write_gpx(&doc).unwrap();

    let wpt = xml.find("<wpt").unwrap();
    assert!(wpt < xml.find("<trk>").unwrap());
    assert!(xml.contains(
        "<wpt lat=\"47.5000000\" lon=\"11.2500000\"><ele>812.3</ele><name>Etappe 1 Ende &amp; mehr</name></wpt>"
    ));
    // Waypoints are not read back; the track is.
    assert_eq!(parse_gpx(&xml).unwrap().tracks.len(), 1);
}
