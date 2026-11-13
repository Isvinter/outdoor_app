use std::fmt::Write as _;

use core_track::TrackPoint;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::{GpxDocument, GpxError};

const HEADER: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
    "<gpx version=\"1.1\" creator=\"outdoor_app\"",
    " xmlns=\"http://www.topografix.com/GPX/1/1\"",
    " xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"",
    " xsi:schemaLocation=\"http://www.topografix.com/GPX/1/1 http://www.topografix.com/GPX/1/1/gpx.xsd\">\n",
);

/// Serialises a document as GPX 1.1.
///
/// Coordinates are written with 7 decimals (about 1 cm), elevation with one.
/// Receiver speed and accuracy have no place in plain GPX 1.1 and are not written.
pub fn write_gpx(doc: &GpxDocument) -> Result<String, GpxError> {
    let mut out = String::from(HEADER);
    if let Some(name) = &doc.name {
        let _ = writeln!(out, "  <metadata><name>{}</name></metadata>", escape(name));
    }
    for track in &doc.tracks {
        out.push_str("  <trk>\n");
        if let Some(name) = &track.name {
            let _ = writeln!(out, "    <name>{}</name>", escape(name));
        }
        if let Some(activity) = track.activity {
            let _ = writeln!(out, "    <type>{}</type>", activity.as_str());
        }
        for segment in &track.segments {
            out.push_str("    <trkseg>\n");
            for point in &segment.points {
                write_point(&mut out, point)?;
            }
            out.push_str("    </trkseg>\n");
        }
        out.push_str("  </trk>\n");
    }
    out.push_str("</gpx>\n");
    Ok(out)
}

fn write_point(out: &mut String, p: &TrackPoint) -> Result<(), GpxError> {
    if !p.lat.is_finite() || !p.lon.is_finite() {
        return Err(GpxError::InvalidCoordinate);
    }
    let _ = write!(
        out,
        "      <trkpt lat=\"{:.7}\" lon=\"{:.7}\"",
        p.lat, p.lon
    );
    let ele = p.ele.filter(|e| e.is_finite());
    if ele.is_none() && p.time_ms.is_none() {
        out.push_str("/>\n");
        return Ok(());
    }
    out.push('>');
    if let Some(ele) = ele {
        let _ = write!(out, "<ele>{ele:.1}</ele>");
    }
    if let Some(ms) = p.time_ms {
        let time = OffsetDateTime::from_unix_timestamp_nanos(i128::from(ms) * 1_000_000)
            .ok()
            .and_then(|t| t.format(&Rfc3339).ok())
            .ok_or(GpxError::TimeOutOfRange)?;
        let _ = write!(out, "<time>{time}</time>");
    }
    out.push_str("</trkpt>\n");
    Ok(())
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}
