//! Mapbox Vector Tile encoding (specification 2.1) of contour lines: one
//! layer `contours`, line features with the integer properties `ele` and `level`.

/// Tile coordinates run from 0 to this value.
pub const EXTENT: u32 = 4096;
const LAYER: &str = "contours";

fn varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push((value & 0x7f) as u8 | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn zigzag(value: i64) -> u64 {
    ((value << 1) ^ (value >> 63)) as u64
}

fn key(out: &mut Vec<u8>, field: u64, wire: u64) {
    varint(out, field << 3 | wire);
}

fn bytes(out: &mut Vec<u8>, field: u64, data: &[u8]) {
    key(out, field, 2);
    varint(out, data.len() as u64);
    out.extend_from_slice(data);
}

fn uint(out: &mut Vec<u8>, field: u64, value: u64) {
    key(out, field, 0);
    varint(out, value);
}

/// Encodes lines `(ele, level, points)` given in tile coordinates; points may
/// lie outside 0..EXTENT (the buffer around the tile).
pub fn encode_tile(lines: &[(i32, u8, Vec<[i32; 2]>)]) -> Vec<u8> {
    // Values: elevations as sint, levels as uint, each stored once.
    let mut values: Vec<(bool, i64)> = Vec::new();
    let mut value_index = |value: (bool, i64)| match values.iter().position(|v| *v == value) {
        Some(index) => index as u64,
        None => {
            values.push(value);
            values.len() as u64 - 1
        }
    };
    let mut features: Vec<Vec<u8>> = Vec::new();
    for (ele, level, points) in lines {
        if points.len() < 2 {
            continue;
        }
        let tags = [
            0,
            value_index((true, i64::from(*ele))),
            1,
            value_index((false, i64::from(*level))),
        ];
        let mut geometry = Vec::new();
        let mut cursor = [0i32, 0i32];
        let mut command = |out: &mut Vec<u8>, point: [i32; 2]| {
            varint(out, zigzag(i64::from(point[0] - cursor[0])));
            varint(out, zigzag(i64::from(point[1] - cursor[1])));
            cursor = point;
        };
        varint(&mut geometry, 1 << 3 | 1); // MoveTo, one point
        command(&mut geometry, points[0]);
        varint(&mut geometry, ((points.len() - 1) as u64) << 3 | 2); // LineTo
        for point in &points[1..] {
            command(&mut geometry, *point);
        }
        let mut packed_tags = Vec::new();
        for tag in tags {
            varint(&mut packed_tags, tag);
        }
        let mut feature = Vec::new();
        bytes(&mut feature, 2, &packed_tags);
        uint(&mut feature, 3, 2); // LINESTRING
        bytes(&mut feature, 4, &geometry);
        features.push(feature);
    }
    let mut layer = Vec::new();
    uint(&mut layer, 15, 2);
    bytes(&mut layer, 1, LAYER.as_bytes());
    for feature in &features {
        bytes(&mut layer, 2, feature);
    }
    bytes(&mut layer, 3, b"ele");
    bytes(&mut layer, 3, b"level");
    for (signed, value) in values {
        let mut encoded = Vec::new();
        if signed {
            uint(&mut encoded, 6, zigzag(value));
        } else {
            uint(&mut encoded, 5, value as u64);
        }
        bytes(&mut layer, 4, &encoded);
    }
    uint(&mut layer, 5, u64::from(EXTENT));
    let mut tile = Vec::new();
    bytes(&mut tile, 3, &layer);
    tile
}
