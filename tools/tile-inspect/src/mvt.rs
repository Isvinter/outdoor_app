//! A Mapbox Vector Tile (specification 2.1) reader: layer names, geometry
//! types and properties. Coordinates are not decoded.

use serde_json::{Map, Number, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Geometry {
    Point,
    Line,
    Polygon,
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Feature {
    pub geometry: Geometry,
    pub props: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    pub name: String,
    pub features: Vec<Feature>,
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Reader { data, pos: 0 }
    }

    fn done(&self) -> bool {
        self.pos >= self.data.len()
    }

    fn varint(&mut self) -> u64 {
        let mut value = 0u64;
        let mut shift = 0;
        while let Some(&byte) = self.data.get(self.pos) {
            self.pos += 1;
            if shift < 64 {
                value |= u64::from(byte & 0x7f) << shift;
            }
            shift += 7;
            if byte & 0x80 == 0 {
                break;
            }
        }
        value
    }

    fn take(&mut self, len: usize) -> &'a [u8] {
        let start = self.pos.min(self.data.len());
        let end = start.saturating_add(len).min(self.data.len());
        self.pos = end;
        &self.data[start..end]
    }

    fn bytes(&mut self) -> &'a [u8] {
        let len = self.varint() as usize;
        self.take(len)
    }

    fn key(&mut self) -> (u64, u64) {
        let key = self.varint();
        (key >> 3, key & 7)
    }

    fn skip(&mut self, wire: u64) {
        match wire {
            0 => {
                self.varint();
            }
            1 => {
                self.take(8);
            }
            2 => {
                self.bytes();
            }
            5 => {
                self.take(4);
            }
            // Unknown wire type: the rest cannot be parsed.
            _ => self.pos = self.data.len(),
        }
    }
}

fn zigzag(v: u64) -> i64 {
    (v >> 1) as i64 ^ -((v & 1) as i64)
}

fn float(value: f64) -> Value {
    Number::from_f64(value).map_or(Value::Null, Value::Number)
}

fn value(data: &[u8]) -> Value {
    let mut r = Reader::new(data);
    let mut out = Value::Null;
    while !r.done() {
        out = match r.key() {
            (1, 2) => Value::String(String::from_utf8_lossy(r.bytes()).into_owned()),
            (2, 5) => {
                let raw = r.take(4);
                float(
                    raw.try_into()
                        .map_or(f64::NAN, |b| f64::from(f32::from_le_bytes(b))),
                )
            }
            (3, 1) => {
                let raw = r.take(8);
                float(raw.try_into().map_or(f64::NAN, f64::from_le_bytes))
            }
            (4, 0) => Value::from(r.varint() as i64),
            (5, 0) => Value::from(r.varint()),
            (6, 0) => Value::from(zigzag(r.varint())),
            (7, 0) => Value::Bool(r.varint() != 0),
            (_, wire) => {
                r.skip(wire);
                continue;
            }
        };
    }
    out
}

fn packed(data: &[u8]) -> Vec<u64> {
    let mut r = Reader::new(data);
    let mut out = Vec::new();
    while !r.done() {
        out.push(r.varint());
    }
    out
}

fn layer(data: &[u8]) -> Layer {
    let mut r = Reader::new(data);
    let (mut name, mut keys, mut values, mut raw) =
        (String::new(), Vec::new(), Vec::new(), Vec::new());
    while !r.done() {
        match r.key() {
            (1, 2) => name = String::from_utf8_lossy(r.bytes()).into_owned(),
            (2, 2) => raw.push(r.bytes()),
            (3, 2) => keys.push(String::from_utf8_lossy(r.bytes()).into_owned()),
            (4, 2) => values.push(value(r.bytes())),
            (_, wire) => r.skip(wire),
        }
    }
    let features = raw
        .into_iter()
        .map(|data| {
            let mut f = Reader::new(data);
            let (mut tags, mut geometry) = (Vec::new(), Geometry::Unknown);
            while !f.done() {
                match f.key() {
                    (2, 2) => tags = packed(f.bytes()),
                    (3, 0) => {
                        geometry = match f.varint() {
                            1 => Geometry::Point,
                            2 => Geometry::Line,
                            3 => Geometry::Polygon,
                            _ => Geometry::Unknown,
                        }
                    }
                    (_, wire) => f.skip(wire),
                }
            }
            let props = tags
                .as_chunks::<2>()
                .0
                .iter()
                .filter_map(|[k, v]| {
                    let key = keys.get(*k as usize)?;
                    let value = values.get(*v as usize)?;
                    Some((key.clone(), value.clone()))
                })
                .collect();
            Feature { geometry, props }
        })
        .collect();
    Layer { name, features }
}

/// All layers of an uncompressed tile; damaged parts are skipped.
pub fn decode(tile: &[u8]) -> Vec<Layer> {
    let mut r = Reader::new(tile);
    let mut layers = Vec::new();
    while !r.done() {
        match r.key() {
            (3, 2) => layers.push(layer(r.bytes())),
            (_, wire) => r.skip(wire),
        }
    }
    layers
}
