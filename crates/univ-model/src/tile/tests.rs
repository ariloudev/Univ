//! The tile encoding round trip and its refusals. Every point here is SIMULATED.

use super::*;

fn id(n: u64) -> ObjectId {
    ObjectId::new(n).unwrap()
}

fn node() -> NodeBox {
    NodeBox::new([-512.0, -512.0, 0.0], 512.0).unwrap()
}

/// SIMULATED: points spread over the node, its corners and upper faces among them.
fn points() -> Vec<TilePoint> {
    let kinds = [
        KindOfValue::Measured,
        KindOfValue::Estimated,
        KindOfValue::Inferred,
        KindOfValue::Reconstructed,
    ];
    let mut points = vec![
        TilePoint {
            position: [-512.0, -512.0, 0.0],
            id: id(1),
            source: 0,
            kind: KindOfValue::Measured,
            value: None,
            object: None,
        },
        TilePoint {
            position: [0.0, 0.0, 512.0],
            id: id((1 << 53) - 1),
            source: u16::MAX,
            kind: KindOfValue::Reconstructed,
            value: Some(PointValue::Colour(PointColour {
                index: -327.67,
                bands: ColourBands::BV,
            })),
            object: None,
        },
    ];
    for k in 0..200_u32 {
        let t = f64::from(k) / 200.0;
        points.push(TilePoint {
            position: [
                -512.0 + 511.9 * t,
                -512.0 + 512.0 * (t * 7.3).fract(),
                512.0 * t * t,
            ],
            id: id(u64::from(k) + 2),
            source: u16::try_from(k % 5).unwrap(),
            kind: kinds[k as usize % 4],
            value: (k % 3 != 0).then(|| {
                PointValue::Colour(PointColour {
                    index: f64::from(k) / 40.0 - 1.234,
                    bands: COLOUR_BANDS[k as usize % 2],
                })
            }),
            object: None,
        });
    }
    points
}

#[test]
fn the_most_points_a_tile_holds_fit_in_one_mebibyte() {
    // A version 4 point is the widest, so it bounds a tile of either version written.
    assert_eq!(MAX_POINTS, 49_929);
    assert_eq!(max_points(POINT_BYTES), 52_426);
    assert!(tile_bytes(V4_POINT_BYTES, MAX_POINTS) <= MAX_TILE_BYTES);
    assert!(tile_bytes(V4_POINT_BYTES, MAX_POINTS + 1) > MAX_TILE_BYTES);
}

#[test]
fn a_tile_round_trips_within_half_a_step_of_each_position() {
    let node = node();
    let points = points();
    let bytes = encode(&node, &points).unwrap();
    assert_eq!(bytes.len(), tile_bytes(POINT_BYTES, points.len()));
    let tile = decode(&bytes).unwrap();
    assert_eq!(tile.version, BASE_VERSION);
    assert_eq!(tile.node, node);
    assert_eq!(tile.points.len(), points.len());
    let step = node.step();
    assert!(
        (step - 512.0 / 65_536.0).abs() < 1e-15,
        "1/65,536 of the edge"
    );
    for (p, d) in points.iter().zip(&tile.points) {
        assert_eq!((d.id, d.source, d.kind), (p.id, p.source, p.kind));
        for i in 0..3 {
            let off = (d.position[i] - p.position[i]).abs();
            assert!(off <= step / 2.0 * (1.0 + 1e-9), "{p:?} decoded to {d:?}");
        }
    }
}

#[test]
fn the_byte_layout_is_as_documented() {
    let node = NodeBox::new([0.0, 0.0, 0.0], 65_536.0).unwrap();
    let p = TilePoint {
        position: [1.0, 2.5, 65_536.0],
        id: id(0x0102_0304_0506),
        source: 0x0A0B,
        kind: KindOfValue::Inferred,
        value: Some(PointValue::Colour(PointColour {
            index: 1.856,
            bands: ColourBands::BV,
        })),
        object: None,
    };
    let bytes = encode(&node, &[p]).unwrap();
    assert_eq!(&bytes[0..4], b"UVTL");
    assert_eq!(&bytes[4..8], &[3, 0, 0, 0]);
    assert_eq!(&bytes[8..16], &[1, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(&bytes[40..48], &65_536.0_f64.to_le_bytes());
    assert_eq!(
        &bytes[48..54],
        &[1, 0, 2, 0, 0xFF, 0xFF],
        "the upper face in the last step"
    );
    assert_eq!(&bytes[54..62], &[6, 5, 4, 3, 2, 1, 0, 0]);
    assert_eq!(&bytes[62..64], &[0x0B, 0x0A]);
    assert_eq!(bytes[64], 2);
    assert_eq!(
        &bytes[65..67],
        &186_i16.to_le_bytes(),
        "1.856 to the nearest hundredth"
    );
    assert_eq!(bytes[67], 2, "B-V");
    assert_eq!(bytes.len(), 68);
}

#[test]
fn a_colour_index_round_trips_to_the_nearest_hundredth_with_its_bands_or_none() {
    let decoded = decode(&encode(&node(), &points()).unwrap()).unwrap();
    for (p, d) in points().iter().zip(&decoded.points) {
        match (
            p.value.and_then(PointValue::colour),
            d.value.and_then(PointValue::colour),
        ) {
            (None, None) => {}
            (Some(c), Some(e)) => {
                assert!(
                    (c.index - e.index).abs() <= 0.005 + 1e-12,
                    "{} {}",
                    c.index,
                    e.index
                );
                assert_eq!(c.bands, e.bands);
            }
            other => panic!("{other:?}"),
        }
    }
    let beyond = TilePoint {
        value: Some(PointValue::Colour(PointColour {
            index: 400.0,
            bands: ColourBands::BpRp,
        })),
        ..points()[0]
    };
    assert!(matches!(
        encode(&node(), &[beyond]),
        Err(TileError::ColourRange { .. })
    ));
}

#[test]
fn a_decoder_refuses_a_colour_its_bands_contradict_and_reads_version_1_without_colours() {
    let good = encode(&node(), &points()).unwrap();
    let n = points().len();
    let (colours, bands) = (
        HEADER_BYTES + V1_POINT_BYTES * n,
        HEADER_BYTES + (V1_POINT_BYTES + 2) * n,
    );
    let mut unknown = good.clone();
    unknown[bands + 1] = 4;
    assert_eq!(
        decode(&unknown),
        Err(TileError::ValueCode { point: 1, code: 4 })
    );
    // Point 0 has no colour: a code without a colour index is refused.
    let mut code_alone = good.clone();
    code_alone[bands] = 1;
    assert_eq!(
        decode(&code_alone),
        Err(TileError::ValueCode { point: 0, code: 1 })
    );
    let mut colour_alone = good.clone();
    colour_alone[bands + 1] = 0;
    assert_eq!(
        decode(&colour_alone),
        Err(TileError::ValueCode { point: 1, code: 0 })
    );
    // The same tile as version 1: the version field 1 and no colour columns.
    let mut v1 = good[..colours].to_vec();
    v1[4..6].copy_from_slice(&1u16.to_le_bytes());
    let decoded = decode(&v1).unwrap();
    assert_eq!(decoded.version, 1);
    assert!(decoded.points.iter().all(|p| p.value.is_none()));
    assert_eq!(decoded.points.len(), n);
}

#[test]
fn an_encoder_refuses_too_many_points_or_one_outside_its_node() {
    let node = node();
    let p = points()[0];
    // A version 3 tile holds 52,426 points of 20 bytes, a version 4 tile 49,929 of 21.
    for (version, bytes) in [(BASE_VERSION, POINT_BYTES), (VERSION, V4_POINT_BYTES)] {
        let most = max_points(bytes);
        assert_eq!(
            encode_as(version, &node, &vec![p; most + 1]),
            Err(TileError::TooManyPoints {
                points: most + 1,
                point_bytes: bytes,
            })
        );
        assert!(encode_as(version, &node, &vec![p; most]).is_ok());
    }
    assert_eq!(
        encode_as(2, &node, &[p]),
        Err(TileError::UnwrittenVersion(2))
    );
    let outside = TilePoint {
        position: [0.1, 0.0, 0.0],
        ..p
    };
    assert!(matches!(
        encode(&node, &[outside]),
        Err(TileError::OutsideNode { .. })
    ));
    let nan = TilePoint {
        position: [f64::NAN, 0.0, 0.0],
        ..p
    };
    assert!(encode(&node, &[nan]).is_err());
    assert!(NodeBox::new([0.0; 3], 0.0).is_err());
    assert!(NodeBox::new([f64::INFINITY, 0.0, 0.0], 1.0).is_err());
}

#[test]
fn a_decoder_refuses_bad_input_with_an_error() {
    let good = encode(&node(), &points()).unwrap();
    for cut in [0, 3, 5, 20, 47, 48, good.len() - 1] {
        assert!(
            matches!(decode(&good[..cut]), Err(TileError::Truncated { .. })),
            "cut at {cut}"
        );
    }
    let mut longer = good.clone();
    longer.push(0);
    assert!(matches!(
        decode(&longer),
        Err(TileError::TrailingBytes { .. })
    ));

    let mut version = good.clone();
    version[4] = 5;
    let error = decode(&version).unwrap_err();
    assert_eq!(error, TileError::UnknownVersion(5));
    assert!(error.to_string().contains("version 5"), "{error}");

    let mut magic = good.clone();
    magic[0] = b'X';
    assert!(matches!(decode(&magic), Err(TileError::Magic(_))));
    let mut reserved = good.clone();
    reserved[13] = 1;
    assert_eq!(decode(&reserved), Err(TileError::Reserved));

    let mut count = good.clone();
    count[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        decode(&count),
        Err(TileError::TooManyPoints { .. })
    ));
    assert_eq!(
        decode(&vec![0; MAX_TILE_BYTES + 1]),
        Err(TileError::Magic([0; 4]))
    );

    let mut edge = good.clone();
    edge[40..48].copy_from_slice(&(-1.0_f64).to_le_bytes());
    assert!(matches!(decode(&edge), Err(TileError::BadNode { .. })));

    let n = points().len();
    let mut zero_id = good.clone();
    zero_id[HEADER_BYTES + 6 * n..HEADER_BYTES + 6 * n + 8].fill(0);
    assert_eq!(
        decode(&zero_id),
        Err(TileError::ObjectId { point: 0, id: 0 })
    );
    let mut kind = good.clone();
    kind[HEADER_BYTES + 16 * n] = 9;
    assert_eq!(decode(&kind), Err(TileError::Kind { point: 0, code: 9 }));
}

#[test]
fn a_decoder_never_panics_on_arbitrary_bytes() {
    let good = encode(&node(), &points()[..3]).unwrap();
    // Every single-byte change and every prefix of a small tile, and a run of made-up bytes.
    for i in 0..good.len() {
        for b in [0, 1, 0x7F, 0xFF] {
            let mut bytes = good.clone();
            bytes[i] = b;
            let _ = decode(&bytes);
        }
        let _ = decode(&good[..i]);
    }
    let mut x: u32 = 12_345;
    let noise: Vec<u8> = (0..4_096)
        .map(|_| {
            x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            x.to_le_bytes()[2]
        })
        .collect();
    for len in 0..noise.len() {
        let _ = decode(&noise[..len]);
    }
}

#[test]
fn a_child_cube_halves_its_parent_and_takes_its_octant() {
    let node = node();
    for octant in 0..8 {
        let child = node.child(octant);
        assert_eq!(child.edge(), 256.0);
        let centre = child.min().map(|c| c + 128.0);
        assert_eq!(node.octant_of(centre), octant);
        assert!(node.contains(child.max()));
    }
    assert_eq!(
        node.octant_of([-256.0, -256.0, 256.0]),
        7,
        "the middle plane goes up"
    );
    assert_eq!(node.octant_of([-512.0, -511.0, 0.0]), 0);
}

#[test]
fn an_oversized_tile_of_an_unknown_version_is_refused_naming_its_version() {
    let mut bytes = vec![0; 1_048_577];
    bytes[..4].copy_from_slice(&MAGIC);
    bytes[4..6].copy_from_slice(&5u16.to_le_bytes());
    let error = decode(&bytes).unwrap_err();
    assert_eq!(error, TileError::UnknownVersion(5));
    assert!(error.to_string().contains("version 5"), "{error}");
    bytes[..4].copy_from_slice(b"XXXX");
    assert_eq!(decode(&bytes), Err(TileError::Magic(*b"XXXX")));
    bytes[..4].copy_from_slice(&MAGIC);
    bytes[4..6].copy_from_slice(&VERSION.to_le_bytes());
    assert_eq!(decode(&bytes), Err(TileError::Oversized(1_048_577)));
}

/// A tile of the newest version of one point at the far corner of the node with corner `min`
/// and edge `edge`.
fn one_point_tile(min: f64, edge: f64) -> Vec<u8> {
    let mut bytes = vec![0; tile_bytes(V4_POINT_BYTES, 1)];
    bytes[..4].copy_from_slice(&MAGIC);
    bytes[4..6].copy_from_slice(&VERSION.to_le_bytes());
    bytes[8..12].copy_from_slice(&1u32.to_le_bytes());
    for at in [16, 24, 32] {
        bytes[at..at + 8].copy_from_slice(&min.to_le_bytes());
    }
    bytes[40..48].copy_from_slice(&edge.to_le_bytes());
    bytes[48..54].fill(0xFF);
    bytes[54..62].copy_from_slice(&1u64.to_le_bytes());
    bytes[65..67].copy_from_slice(&NO_VALUE.to_le_bytes());
    bytes
}

#[test]
fn a_node_whose_far_corner_overflows_or_whose_step_is_not_normal_is_refused() {
    let huge = 1.7e308;
    let subnormal = f64::from_bits(1);
    for (min, edge) in [
        (huge, huge),
        (-huge, 2.0 * huge),
        (0.0, subnormal),
        (0.0, f64::MIN_POSITIVE),
        (0.0, f64::MIN_POSITIVE * 65_535.0),
    ] {
        assert!(
            matches!(NodeBox::new([min; 3], edge), Err(TileError::BadNode { .. })),
            "{min}, {edge}"
        );
        assert!(
            matches!(
                decode(&one_point_tile(min, edge)),
                Err(TileError::BadNode { .. })
            ),
            "decoded {min}, {edge}"
        );
    }
    // The smallest edge whose step is normal, and the largest whose far corner is finite.
    let smallest = f64::MIN_POSITIVE * 65_536.0;
    assert_eq!(
        NodeBox::new([0.0; 3], smallest).unwrap().step(),
        f64::MIN_POSITIVE
    );
    assert!(decode(&one_point_tile(0.0, smallest)).is_ok());
    let node = NodeBox::new([-huge / 2.0; 3], huge).unwrap();
    assert!(node.max().iter().all(|c| c.is_finite()));
    let decoded = decode(&one_point_tile(-huge / 2.0, huge)).unwrap();
    assert!(decoded.points[0].position.iter().all(|c| c.is_finite()));
}

#[test]
fn each_colour_bands_code_reads_back_as_its_bands_and_a_version_1_count_is_refused_by_its_own_size()
{
    for (i, bands) in (1u8..).zip(COLOUR_BANDS) {
        assert_eq!(
            bands.code(),
            i,
            "{bands:?}: its index in COLOUR_BANDS plus one"
        );
        assert_eq!(ColourBands::of_code(i), Some(bands));
    }
    assert_eq!(ColourBands::of_code(0), None);
    // A version 1 tile one point beyond version 1's own limit, 61,678 points of 17 bytes.
    let n = max_points(V1_POINT_BYTES) + 1;
    let mut bytes = vec![0; HEADER_BYTES];
    bytes[..4].copy_from_slice(&MAGIC);
    bytes[4..6].copy_from_slice(&1u16.to_le_bytes());
    bytes[8..12].copy_from_slice(&u32::try_from(n).unwrap().to_le_bytes());
    let error = decode(&bytes).unwrap_err();
    assert_eq!(
        error,
        TileError::TooManyPoints {
            points: 61_679,
            point_bytes: 17
        }
    );
    assert!(
        error.to_string().contains("at most 61678 points"),
        "{error}"
    );
}
