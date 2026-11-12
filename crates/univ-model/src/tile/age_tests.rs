//! Version 3's point value: a colour index, or the base-10 logarithm of an age in years, coded
//! apart, in the same 20 bytes a version 2 point takes; and version 4's kind of object after
//! them. Every point here is SIMULATED.

use super::*;

fn node() -> NodeBox {
    NodeBox::new([0.0, 0.0, 0.0], 1_000.0).unwrap()
}

fn point(n: u64, value: Option<PointValue>) -> TilePoint {
    TilePoint {
        position: [f64::from(u32::try_from(n).unwrap()), 10.0, 20.0],
        id: ObjectId::new(n).unwrap(),
        source: 0,
        kind: KindOfValue::Estimated,
        value,
        object: None,
    }
}

fn colour(index: f64, bands: ColourBands) -> PointValue {
    PointValue::Colour(PointColour { index, bands })
}

/// SIMULATED: a point of each value code: none, BP−RP, B−V and a log age.
fn points() -> Vec<TilePoint> {
    vec![
        point(1, None),
        point(2, Some(colour(0.82, ColourBands::BpRp))),
        point(3, Some(colour(-0.12, ColourBands::BV))),
        point(4, Some(PointValue::LogAge(7.325_891_02))),
    ]
}

#[test]
fn version_3_round_trips_every_value_code_in_20_bytes_a_point() {
    let bytes = encode(&node(), &points()).unwrap();
    assert_eq!(BASE_VERSION, 3);
    assert_eq!(u16::from_le_bytes([bytes[4], bytes[5]]), 3);
    assert_eq!(bytes.len(), HEADER_BYTES + 20 * 4);
    assert_eq!(point_bytes(3), Some(20));
    let tile = decode(&bytes).unwrap();
    let p: Vec<Option<PointValue>> = tile.points.iter().map(|p| p.value).collect();
    assert_eq!(p[0], None);
    let bp_rp = p[1].and_then(PointValue::colour).unwrap();
    assert_eq!(bp_rp.bands, ColourBands::BpRp);
    assert!((bp_rp.index - 0.82).abs() < 0.005);
    assert_eq!(
        p[2].and_then(PointValue::colour).unwrap().bands,
        ColourBands::BV
    );
    // An age decodes within 0.005 of the logarithm it was built from, well within the 0.01 the
    // specification asks.
    let log_age = p[3].and_then(PointValue::log_age).unwrap();
    assert!((log_age - 7.325_891_02).abs() <= 0.005);
    let n = 4;
    let codes = HEADER_BYTES + (V1_POINT_BYTES + 2) * n;
    assert_eq!(bytes[codes..codes + n], [0, 1, 2, LOG_AGE_CODE]);
}

#[test]
fn the_age_code_is_refused_in_a_version_2_tile() {
    let mut bytes = encode(&node(), &points()).unwrap();
    bytes[4..6].copy_from_slice(&2u16.to_le_bytes());
    assert_eq!(
        decode(&bytes),
        Err(TileError::ValueCode {
            point: 3,
            code: LOG_AGE_CODE
        })
    );
    // Without the age, the same points read as a version 2 tile.
    let mut without = encode(&node(), &points()[..3]).unwrap();
    without[4..6].copy_from_slice(&2u16.to_le_bytes());
    assert_eq!(decode(&without).unwrap().points.len(), 3);
}

#[test]
fn an_encoder_refuses_an_age_beyond_a_tile() {
    let beyond = point(6, Some(PointValue::LogAge(400.0)));
    assert!(matches!(
        encode(&node(), &[beyond]),
        Err(TileError::AgeRange { .. })
    ));
}

#[test]
fn the_version_3_layout_names_the_value_and_its_code_and_the_age_code() {
    let v3 = format::layout(3).unwrap();
    let names: Vec<&str> = v3.points.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names[names.len() - 2..], ["value", "value_code"]);
    assert_eq!(v3.point_bytes, 20);
    assert_eq!(v3.log_age_code, Some(LOG_AGE_CODE));
    let v2 = format::layout(2).unwrap();
    assert_eq!(v2.log_age_code, None);
    assert!(v2.points.iter().any(|f| f.name == "colour"));
}

#[test]
fn version_4_adds_each_points_kind_of_object_after_version_3s_columns() {
    // build-octree-tiles: the kind of object follows every column of a version 3 point; a body
    // carries code 0, which no kind of object takes.
    let kinds = [
        Some(ObjectKind::LuminousHotStar),
        None,
        Some(ObjectKind::OpenCluster),
        Some(ObjectKind::StarFormingRegion),
    ];
    let points: Vec<TilePoint> = points()
        .into_iter()
        .zip(kinds)
        .map(|(p, object)| TilePoint { object, ..p })
        .collect();
    let v3 = encode(&node(), &points).unwrap();
    let v4 = encode_as(VERSION, &node(), &points).unwrap();
    let n = points.len();
    assert_eq!(VERSION, 4);
    assert_eq!(u16::from_le_bytes([v4[4], v4[5]]), 4);
    assert_eq!(v4.len(), HEADER_BYTES + 21 * n);
    assert_eq!(point_bytes(4), Some(21));
    // Every byte after the version field is version 3's, then the kind of object column.
    assert_eq!(v4[6..HEADER_BYTES + POINT_BYTES * n], v3[6..]);
    let codes = &v4[HEADER_BYTES + POINT_BYTES * n..];
    assert_eq!(codes, [7, 0, 2, 6]);
    let decoded = decode(&v4).unwrap();
    let objects: Vec<_> = decoded.points.iter().map(|p| p.object).collect();
    assert_eq!(objects, kinds);
    let values: Vec<_> = decoded.points.iter().map(|p| p.value).collect();
    let v3_values: Vec<_> = decode(&v3)
        .unwrap()
        .points
        .iter()
        .map(|p| p.value)
        .collect();
    assert_eq!(values, v3_values);
    // A version 3 tile carries no kind of object.
    assert!(
        decode(&v3)
            .unwrap()
            .points
            .iter()
            .all(|p| p.object.is_none())
    );
}

#[test]
fn every_kind_of_object_round_trips_and_a_code_beyond_them_is_refused() {
    for (i, kind) in OBJECT_KINDS.iter().enumerate() {
        let code = object_kind_code(Some(*kind));
        assert_eq!(usize::from(code), i + 1);
        assert_eq!(object_kind_of_code(code), Ok(Some(*kind)));
    }
    assert_eq!(object_kind_code(None), 0);
    assert_eq!(object_kind_code(Some(ObjectKind::Body)), 0);
    assert_eq!(object_kind_of_code(0), Ok(None));
    let mut bytes = encode_as(VERSION, &node(), &points()).unwrap();
    let last = bytes.len() - 1;
    bytes[last] = 8;
    assert_eq!(
        decode(&bytes),
        Err(TileError::ObjectKind { point: 3, code: 8 })
    );
}
