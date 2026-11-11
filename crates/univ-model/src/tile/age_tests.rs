//! Version 3's point value: a colour index, or the base-10 logarithm of an age in years, coded
//! apart, in the same 20 bytes a version 2 point takes. Every point here is SIMULATED.

use super::*;

fn node() -> NodeBox {
    NodeBox::new([0.0, 0.0, 0.0], 1_000.0).unwrap()
}

fn point(n: u64, colour: Option<PointColour>, log_age: Option<f64>) -> TilePoint {
    TilePoint {
        position: [f64::from(u32::try_from(n).unwrap()), 10.0, 20.0],
        id: ObjectId::new(n).unwrap(),
        source: 0,
        kind: KindOfValue::Estimated,
        colour,
        log_age,
    }
}

/// SIMULATED: a point of each value code: none, BP−RP, B−V and a log age.
fn points() -> Vec<TilePoint> {
    vec![
        point(1, None, None),
        point(
            2,
            Some(PointColour {
                index: 0.82,
                bands: ColourBands::BpRp,
            }),
            None,
        ),
        point(
            3,
            Some(PointColour {
                index: -0.12,
                bands: ColourBands::BV,
            }),
            None,
        ),
        point(4, None, Some(7.325_891_02)),
    ]
}

#[test]
fn version_3_round_trips_every_value_code_in_20_bytes_a_point() {
    let bytes = encode(&node(), &points()).unwrap();
    assert_eq!(VERSION, 3);
    assert_eq!(u16::from_le_bytes([bytes[4], bytes[5]]), 3);
    assert_eq!(bytes.len(), HEADER_BYTES + 20 * 4);
    assert_eq!(point_bytes(3), Some(20));
    let tile = decode(&bytes).unwrap();
    let p = &tile.points;
    assert_eq!((p[0].colour, p[0].log_age), (None, None));
    assert_eq!(p[1].colour.unwrap().bands, ColourBands::BpRp);
    assert!((p[1].colour.unwrap().index - 0.82).abs() < 0.005);
    assert_eq!(p[2].colour.unwrap().bands, ColourBands::BV);
    assert_eq!(p[3].colour, None);
    // An age decodes within 0.005 of the logarithm it was built from, well within the 0.01 the
    // specification asks.
    assert!((p[3].log_age.unwrap() - 7.325_891_02).abs() <= 0.005);
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
        Err(TileError::Colour {
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
fn an_encoder_refuses_a_point_with_both_values_or_an_age_beyond_a_tile() {
    let both = point(
        5,
        Some(PointColour {
            index: 0.5,
            bands: ColourBands::BpRp,
        }),
        Some(8.0),
    );
    assert!(matches!(
        encode(&node(), &[both]),
        Err(TileError::TwoValues { .. })
    ));
    let beyond = point(6, None, Some(400.0));
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
