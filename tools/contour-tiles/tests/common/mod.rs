//! A minimal Mapbox Vector Tile reader, enough to check what the encoder wrote.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Sint(i64),
    Uint(u64),
    Other,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Feature {
    pub geom_type: u64,
    /// Key name and value, in tag order.
    pub tags: Vec<(String, Value)>,
    /// Decoded lines in tile coordinates.
    pub lines: Vec<Vec<[i32; 2]>>,
}

impl Feature {
    pub fn int(&self, key: &str) -> i64 {
        match self.tags.iter().find(|(k, _)| k == key).map(|(_, v)| v) {
            Some(Value::Sint(v)) => *v,
            Some(Value::Uint(v)) => *v as i64,
            other => panic!("no integer {key}: {other:?}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    pub version: u64,
    pub name: String,
    pub extent: u64,
    pub features: Vec<Feature>,
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn varint(&mut self) -> u64 {
        let mut value = 0u64;
        let mut shift = 0;
        loop {
            let byte = self.data[self.pos];
            self.pos += 1;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return value;
            }
            shift += 7;
        }
    }
    fn bytes(&mut self) -> &[u8] {
        let len = self.varint() as usize;
        let start = self.pos;
        self.pos += len;
        &self.data[start..self.pos]
    }
    fn done(&self) -> bool {
        self.pos >= self.data.len()
    }
    /// Field number and wire type; skips nothing.
    fn key(&mut self) -> (u64, u64) {
        let key = self.varint();
        (key >> 3, key & 7)
    }
    fn skip(&mut self, wire: u64) {
        match wire {
            0 => {
                self.varint();
            }
            2 => {
                self.bytes();
            }
            5 => self.pos += 4,
            1 => self.pos += 8,
            _ => panic!("wire type {wire}"),
        }
    }
}

fn zigzag(v: u64) -> i64 {
    ((v >> 1) as i64) ^ -((v & 1) as i64)
}

fn packed(data: &[u8]) -> Vec<u64> {
    let mut r = Reader { data, pos: 0 };
    let mut out = Vec::new();
    while !r.done() {
        out.push(r.varint());
    }
    out
}

fn geometry(commands: &[u64]) -> Vec<Vec<[i32; 2]>> {
    let (mut x, mut y) = (0i32, 0i32);
    let mut lines: Vec<Vec<[i32; 2]>> = Vec::new();
    let mut i = 0;
    while i < commands.len() {
        let (id, count) = (commands[i] & 7, commands[i] >> 3);
        i += 1;
        for _ in 0..count {
            x += zigzag(commands[i]) as i32;
            y += zigzag(commands[i + 1]) as i32;
            i += 2;
            match id {
                1 => lines.push(vec![[x, y]]),
                2 => lines.last_mut().unwrap().push([x, y]),
                _ => panic!("command {id}"),
            }
        }
    }
    lines
}

/// All layers of a tile.
pub fn decode(tile: &[u8]) -> Vec<Layer> {
    let mut r = Reader { data: tile, pos: 0 };
    let mut layers = Vec::new();
    while !r.done() {
        let (field, wire) = r.key();
        if field != 3 {
            r.skip(wire);
            continue;
        }
        let mut l = Reader {
            data: r.bytes(),
            pos: 0,
        };
        let (mut version, mut name, mut extent) = (1, String::new(), 4096);
        let (mut keys, mut values, mut raw) = (Vec::new(), Vec::new(), Vec::new());
        while !l.done() {
            match l.key() {
                (15, 0) => version = l.varint(),
                (1, 2) => name = String::from_utf8(l.bytes().to_vec()).unwrap(),
                (5, 0) => extent = l.varint(),
                (3, 2) => keys.push(String::from_utf8(l.bytes().to_vec()).unwrap()),
                (4, 2) => {
                    let mut v = Reader {
                        data: l.bytes(),
                        pos: 0,
                    };
                    let mut value = Value::Other;
                    while !v.done() {
                        match v.key() {
                            (6, 0) => value = Value::Sint(zigzag(v.varint())),
                            (5, 0) => value = Value::Uint(v.varint()),
                            (_, wire) => v.skip(wire),
                        }
                    }
                    values.push(value);
                }
                (2, 2) => raw.push(l.bytes().to_vec()),
                (_, wire) => l.skip(wire),
            }
        }
        let features = raw
            .iter()
            .map(|data| {
                let mut f = Reader { data, pos: 0 };
                let (mut geom_type, mut tags, mut geom) = (0, Vec::new(), Vec::new());
                while !f.done() {
                    match f.key() {
                        (3, 0) => geom_type = f.varint(),
                        (2, 2) => tags = packed(f.bytes()),
                        (4, 2) => geom = packed(f.bytes()),
                        (_, wire) => f.skip(wire),
                    }
                }
                Feature {
                    geom_type,
                    tags: tags
                        .chunks(2)
                        .map(|kv| (keys[kv[0] as usize].clone(), values[kv[1] as usize].clone()))
                        .collect(),
                    lines: geometry(&geom),
                }
            })
            .collect();
        layers.push(Layer {
            version,
            name,
            extent,
            features,
        });
    }
    layers
}
