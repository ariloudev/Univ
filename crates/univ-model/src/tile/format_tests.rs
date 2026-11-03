//! The layout of the encoding against the encoder. Every point here is SIMULATED.

use super::format::{EncodingLayout, FieldType, Fixed, HeaderField, VERSIONS, layout};
use super::*;

/// SIMULATED: `n` points spread over a node, every kind and a few sources among them.
fn tile(n: u32) -> (NodeBox, Vec<u8>) {
    let node = NodeBox::new([-64.0, 8.0, -0.5], 128.0).unwrap();
    let points: Vec<TilePoint> = (1..=n)
        .map(|k| {
            let t = f64::from(k) / f64::from(n);
            TilePoint {
                position: [
                    -64.0 + 128.0 * t,
                    8.0 + 128.0 * t * t,
                    -0.5 + 128.0 * (1.0 - t),
                ],
                id: ObjectId::new(u64::from(k) * 7_919).unwrap(),
                source: u16::try_from(k % 3).unwrap(),
                kind: KINDS[usize::try_from(k % 4).unwrap()],
            }
        })
        .collect();
    let bytes = encode(&node, &points).unwrap();
    (node, bytes)
}

fn v1() -> EncodingLayout {
    layout(u32::from(VERSION)).unwrap()
}

fn field<'a>(layout: &'a EncodingLayout, name: &str) -> &'a HeaderField {
    layout.header.iter().find(|f| f.name == name).unwrap()
}

#[test]
fn every_listed_version_has_a_layout_and_no_other_does() {
    assert_eq!(VERSIONS, &[1]);
    for v in VERSIONS {
        assert_eq!(layout(*v).unwrap().version, *v);
    }
    assert!(layout(0).is_none());
    assert!(layout(2).is_none());
}

#[test]
fn the_fields_fill_the_header_and_each_point_without_gaps() {
    let l = v1();
    let mut at = 0;
    for f in &l.header {
        assert_eq!(
            f.offset, at,
            "{} starts where the field before ends",
            f.name
        );
        assert_eq!(f.bytes, f.field_type.bytes() * f.count);
        at += f.bytes;
    }
    assert_eq!(at, l.header_bytes);
    assert_eq!(l.header_bytes, HEADER_BYTES);
    let mut at = 0;
    for f in &l.points {
        assert_eq!(f.column_offset, at);
        at += f.bytes;
    }
    assert_eq!(at, l.point_bytes);
    assert_eq!(l.point_bytes, POINT_BYTES);
}

#[test]
fn the_layouts_sizes_add_up_to_the_encoders_tile() {
    let l = v1();
    let header: usize = l.header.iter().map(|f| f.bytes).sum();
    let point: usize = l.points.iter().map(|f| f.bytes).sum();
    for n in [0, 1, 37] {
        let (_, bytes) = tile(n);
        assert_eq!(bytes.len(), header + point * usize::try_from(n).unwrap());
    }
}

/// The little-endian number of `ty` in `bytes` at `at`.
fn number(bytes: &[u8], at: usize, ty: FieldType) -> u64 {
    let mut b = [0u8; 8];
    b[..ty.bytes()].copy_from_slice(&bytes[at..at + ty.bytes()]);
    u64::from_le_bytes(b)
}

#[test]
fn reading_a_tile_by_the_layout_gives_what_the_decoder_gives() {
    let l = v1();
    let (node, bytes) = tile(37);
    let decoded = decode(&bytes).unwrap();
    let n = decoded.points.len();
    for f in &l.header {
        match &f.value {
            Some(Fixed::Text(t)) => assert_eq!(&bytes[f.offset..f.offset + f.bytes], t.as_bytes()),
            Some(Fixed::Number(v)) => assert_eq!(number(&bytes, f.offset, f.field_type), *v),
            None => {}
        }
    }
    let count = field(&l, "count");
    assert_eq!(number(&bytes, count.offset, count.field_type), 37);
    let f64_at = |at| f64::from_bits(number(&bytes, at, FieldType::F64le));
    let min = field(&l, "node_min").offset;
    assert_eq!([0, 8, 16].map(|d| f64_at(min + d)), node.min());
    assert_eq!(f64_at(field(&l, "node_edge").offset), node.edge());
    let step = node.edge() / f64::from(l.steps_per_edge);
    for (k, p) in decoded.points.iter().enumerate() {
        let at = |name: &str, i: usize| {
            let f = l.points.iter().find(|f| f.name == name).unwrap();
            let start = l.header_bytes + f.column_offset * n + f.bytes * k;
            number(&bytes, start + f.field_type.bytes() * i, f.field_type)
        };
        for i in 0..3 {
            let q = u16::try_from(at("position", i)).unwrap();
            assert_eq!(q, p.quantized[i]);
            let centre = node.min()[i] + (f64::from(q) + 0.5) * step;
            assert_eq!(centre.to_bits(), p.position[i].to_bits());
        }
        assert_eq!(at("object_id", 0), p.id.get());
        assert_eq!(at("source", 0), u64::from(p.source));
        let kind = usize::try_from(at("kind", 0)).unwrap();
        assert_eq!(l.kinds[kind], p.kind);
    }
}

#[test]
fn the_layout_serializes_its_types_as_clients_name_them() {
    let json = serde_json::to_value(v1()).unwrap();
    assert_eq!(json["header"][0]["type"], "ascii");
    assert_eq!(json["header"][0]["value"], "UVTL");
    assert_eq!(json["header"][1]["value"], 1);
    assert_eq!(json["points"][0]["type"], "u16le");
    assert_eq!(json["points"][1]["type"], "u64le");
    assert_eq!(json["kinds"][3], "reconstructed");
    assert!(json["header"][3].get("value").is_none());
}
