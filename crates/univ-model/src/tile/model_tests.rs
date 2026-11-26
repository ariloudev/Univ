//! Version 5, the Galaxy model's tiles (build-the-galaxy-model; decode-the-tile-format).

use super::*;

fn node() -> NodeBox {
    NodeBox::new([-1000.0, -2000.0, -500.0], 4000.0).unwrap()
}

fn points() -> Vec<ModelPoint> {
    vec![
        ModelPoint {
            position: [0.0, 0.0, 0.0],
            population: 1,
            covered: false,
        },
        ModelPoint {
            position: [2999.0, 1999.0, 3499.0],
            population: 5,
            covered: true,
        },
        ModelPoint {
            position: [-1000.0, -2000.0, -500.0],
            population: 6,
            covered: false,
        },
    ]
}

#[test]
fn a_model_point_takes_its_position_population_and_covered_flag_in_8_bytes() {
    let bytes = encode_model(&node(), &points()).unwrap();
    assert_eq!(u16::from_le_bytes([bytes[4], bytes[5]]), MODEL_VERSION);
    assert_eq!(MODEL_VERSION, 5);
    assert_eq!(bytes.len(), HEADER_BYTES + 8 * 3);
    assert_eq!(point_bytes(5), Some(MODEL_POINT_BYTES));
    // The populations, then the flags, after the positions.
    assert_eq!(bytes[HEADER_BYTES + 18..HEADER_BYTES + 21], [1, 5, 6]);
    assert_eq!(bytes[HEADER_BYTES + 21..], [0, COVERED, 0]);
    let tile = decode_model(&bytes).unwrap();
    assert_eq!(tile.node, node());
    for (p, d) in points().iter().zip(&tile.points) {
        assert_eq!(d.population, p.population);
        assert_eq!(d.covered, p.covered);
        for i in 0..3 {
            assert!(
                (d.position[i] - p.position[i]).abs() <= tile.node.step(),
                "{d:?}"
            );
        }
    }
}

#[test]
fn a_model_point_without_a_population_outside_its_node_or_with_an_unknown_flag_is_refused() {
    let mut none = points();
    none[1].population = 0;
    assert_eq!(
        encode_model(&node(), &none),
        Err(TileError::Population { point: 1 })
    );
    let mut outside = points();
    outside[2].position = [5000.0, 0.0, 0.0];
    assert!(matches!(
        encode_model(&node(), &outside),
        Err(TileError::ModelOutsideNode { point: 2, .. })
    ));
    let mut bytes = encode_model(&node(), &points()).unwrap();
    let last = bytes.len() - 1;
    bytes[last] = 0b10;
    assert_eq!(
        decode_model(&bytes),
        Err(TileError::Flags {
            point: 2,
            flags: 0b10
        })
    );
}

#[test]
fn the_object_decoder_refuses_a_model_tile_and_the_model_decoder_an_object_tile() {
    let model = encode_model(&node(), &points()).unwrap();
    assert_eq!(decode(&model), Err(TileError::ModelVersion(MODEL_VERSION)));
    let object = encode(&node(), &[]).unwrap();
    assert_eq!(
        decode_model(&object),
        Err(TileError::ModelVersion(BASE_VERSION))
    );
    assert!(VERSIONS.contains(&MODEL_VERSION));
    assert!(!has_object_kind(MODEL_VERSION) && !has_colour(MODEL_VERSION));
}
