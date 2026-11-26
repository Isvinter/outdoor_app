use core_track::{Activity, Segment, Track, TrackPoint};
use quick_xml::Reader;
use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::{BytesStart, Event};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::{GpxDocument, GpxError};

/// Parses a GPX document. Accepts GPX 1.0 and 1.1; namespaces prefixes are ignored.
pub fn parse_gpx(xml: &str) -> Result<GpxDocument, GpxError> {
    let xml = xml.strip_prefix('\u{feff}').unwrap_or(xml);
    let mut parser = Parser {
        xml,
        reader: Reader::from_str(xml),
        ..Parser::default_state()
    };
    parser.run()?;
    Ok(parser.doc)
}

struct Parser<'a> {
    xml: &'a str,
    reader: Reader<&'a [u8]>,
    /// Local names of the currently open elements.
    path: Vec<String>,
    /// Text content of the innermost open element.
    text: String,
    doc: GpxDocument,
    track: Option<Track>,
    segment: Option<Segment>,
    point: Option<TrackPoint>,
}

impl<'a> Parser<'a> {
    fn default_state() -> Self {
        Self {
            xml: "",
            reader: Reader::from_str(""),
            path: Vec::new(),
            text: String::new(),
            doc: GpxDocument::default(),
            track: None,
            segment: None,
            point: None,
        }
    }

    fn line(&self) -> usize {
        let pos = usize::try_from(self.reader.buffer_position())
            .unwrap_or(usize::MAX)
            .min(self.xml.len());
        1 + self.xml.as_bytes()[..pos]
            .iter()
            .filter(|b| **b == b'\n')
            .count()
    }

    fn xml_error(&self, message: impl ToString) -> GpxError {
        GpxError::Xml {
            line: self.line(),
            message: message.to_string(),
        }
    }

    fn run(&mut self) -> Result<(), GpxError> {
        let mut seen_root = false;
        loop {
            let event = self.reader.read_event().map_err(|e| self.xml_error(e))?;
            match event {
                Event::Start(e) => {
                    seen_root = self.check_root(seen_root, &e)?;
                    self.open(&e)?;
                }
                Event::Empty(e) => {
                    seen_root = self.check_root(seen_root, &e)?;
                    self.open(&e)?;
                    self.close()?;
                }
                Event::End(_) => self.close()?,
                Event::Text(t) => self.text.push_str(&t.xml10_content()),
                Event::CData(t) => self.text.push_str(&t.xml10_content()),
                Event::GeneralRef(r) => {
                    if let Some(c) = r.resolve_char_ref().map_err(|e| self.xml_error(e))? {
                        self.text.push(c);
                    } else {
                        let name = r.xml10_content();
                        let resolved = resolve_predefined_entity(&name)
                            .ok_or_else(|| self.xml_error(format!("unknown entity `&{name};`")))?;
                        self.text.push_str(resolved);
                    }
                }
                Event::Eof => break,
                Event::Comment(_) | Event::Decl(_) | Event::PI(_) | Event::DocType(_) => {}
            }
        }
        if !seen_root {
            return Err(GpxError::NotGpx);
        }
        if !self.path.is_empty() {
            return Err(self.xml_error("unexpected end of document"));
        }
        Ok(())
    }

    fn check_root(&self, seen_root: bool, e: &BytesStart<'_>) -> Result<bool, GpxError> {
        if seen_root || e.local_name().as_ref() == "gpx" {
            Ok(true)
        } else {
            Err(GpxError::NotGpx)
        }
    }

    fn open(&mut self, e: &BytesStart<'_>) -> Result<(), GpxError> {
        let name = e.local_name().as_ref().to_owned();
        self.text.clear();
        match (self.path.last().map(String::as_str), name.as_str()) {
            (Some("gpx"), "trk") => self.track = Some(Track::default()),
            (Some("trk"), "trkseg") => self.segment = Some(Segment::default()),
            (Some("trkseg"), "trkpt") => {
                let lat = self.coordinate(e, "lat")?;
                let lon = self.coordinate(e, "lon")?;
                self.point = Some(TrackPoint::new(lat, lon));
            }
            (Some("extensions"), "section") if self.segment.is_some() => {
                self.section(e)?;
            }
            _ => {}
        }
        self.path.push(name);
        Ok(())
    }

    fn close(&mut self) -> Result<(), GpxError> {
        let Some(name) = self.path.pop() else {
            return Err(self.xml_error("unexpected closing tag"));
        };
        let text = std::mem::take(&mut self.text);
        let text = text.trim();
        let parent = self.path.last().map(String::as_str);
        match (parent, name.as_str()) {
            (Some("metadata"), "name") if self.path.len() == 2 => self.doc.name = non_empty(text),
            (Some("trk"), "name") => {
                if let Some(t) = &mut self.track {
                    t.name = non_empty(text);
                }
            }
            (Some("trk"), "type") => {
                if let Some(t) = &mut self.track {
                    t.activity = activity_from_type(text);
                }
            }
            (Some("trkpt"), "ele") if self.point.is_some() && !text.is_empty() => {
                let ele = self.number(text)?;
                if let Some(p) = &mut self.point {
                    p.ele = Some(ele);
                }
            }
            (Some("trkpt"), "time") if self.point.is_some() && !text.is_empty() => {
                let time_ms = self.time(text)?;
                if let Some(p) = &mut self.point {
                    p.time_ms = Some(time_ms);
                }
            }
            // GPX 1.0 `<speed>` or a vendor extension such as `gpxtpx:speed`
            (_, "speed") if self.point.is_some() && !text.is_empty() => {
                let speed = self.number(text)?;
                if let Some(p) = &mut self.point {
                    p.speed_mps = Some(speed as f32);
                }
            }
            (Some("trkseg"), "trkpt") => {
                if let (Some(p), Some(s)) = (self.point.take(), &mut self.segment) {
                    s.points.push(p);
                }
            }
            (Some("trk"), "trkseg") => {
                if let (Some(s), Some(t)) = (self.segment.take(), &mut self.track) {
                    t.segments.push(s);
                }
            }
            (Some("gpx"), "trk") => self.doc.tracks.extend(self.track.take()),
            _ => {}
        }
        Ok(())
    }

    /// `<zg:section activity=".." reason=".."/>` inside a segment's extensions.
    fn section(&mut self, e: &BytesStart<'_>) -> Result<(), GpxError> {
        for attr in e.attributes() {
            let attr = attr.map_err(|err| self.xml_error(err))?;
            let value = attr
                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .map_err(|err| self.xml_error(err))?;
            let Some(segment) = &mut self.segment else {
                continue;
            };
            match attr.key.local_name().as_ref() {
                "activity" => segment.activity = value.trim().parse().ok(),
                "reason" => segment.reason = value.trim().parse().unwrap_or_default(),
                _ => {}
            }
        }
        Ok(())
    }

    fn coordinate(&self, e: &BytesStart<'_>, attribute: &'static str) -> Result<f64, GpxError> {
        for attr in e.attributes() {
            let attr = attr.map_err(|err| self.xml_error(err))?;
            if attr.key.local_name().as_ref() == attribute {
                return self.number(attr.value.trim());
            }
        }
        Err(GpxError::MissingAttribute {
            attribute,
            line: self.line(),
        })
    }

    fn number(&self, text: &str) -> Result<f64, GpxError> {
        text.parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| GpxError::InvalidNumber {
                value: text.to_owned(),
                line: self.line(),
            })
    }

    fn time(&self, text: &str) -> Result<i64, GpxError> {
        let parsed = OffsetDateTime::parse(text, &Rfc3339)
            // Some devices omit the zone designator; GPX times are UTC by definition.
            .or_else(|_| OffsetDateTime::parse(&format!("{text}Z"), &Rfc3339))
            .map_err(|_| GpxError::InvalidTime {
                value: text.to_owned(),
                line: self.line(),
            })?;
        i64::try_from(parsed.unix_timestamp_nanos() / 1_000_000).map_err(|_| {
            GpxError::InvalidTime {
                value: text.to_owned(),
                line: self.line(),
            }
        })
    }
}

fn non_empty(text: &str) -> Option<String> {
    (!text.is_empty()).then(|| text.to_owned())
}

/// Maps the free-text GPX `<type>` to an activity. Unknown values yield `None`.
fn activity_from_type(text: &str) -> Option<Activity> {
    let key: String = text
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    match key.as_str() {
        "bike" | "biking" | "cycling" | "ride" | "roadbiking" | "racebike" | "touringbicycle" => {
            Some(Activity::Bike)
        }
        "mtb" | "mountainbiking" | "mountainbike" | "emountainbike" => Some(Activity::Mtb),
        "hike" | "hiking" | "walk" | "walking" | "mountaineering" => Some(Activity::Hike),
        "skitour" | "skitouring" | "backcountryski" | "backcountryskiing" => {
            Some(Activity::SkiTour)
        }
        // What this app writes for a track without an activity.
        "other" => Some(Activity::Other),
        _ => None,
    }
}
