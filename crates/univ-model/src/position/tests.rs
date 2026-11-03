use super::*;

/// Reference values from astropy 8.0.2, whose galactic frame is defined from FK5 J2000 rather
/// than ICRS, which moves positions by up to about 0.1″; hence a tolerance of 1e-4°.
const ASTROPY: f64 = 1e-4;

fn assert_near(actual: (f64, f64), expected: (f64, f64), tolerance: f64) {
    let dlon = (actual.0 - expected.0 + 180.0).rem_euclid(360.0) - 180.0;
    assert!(
        dlon.abs() < tolerance && (actual.1 - expected.1).abs() < tolerance,
        "{actual:?} is not within {tolerance}° of {expected:?}"
    );
}

fn to(frame: Frame, ra: f64, dec: f64) -> (f64, f64) {
    lon_lat(from_icrs(frame).apply(unit_vector(ra, dec)))
}

#[test]
fn the_galactic_rotation_matches_the_hipparcos_matrix_and_is_orthonormal() {
    let derived = from_icrs(Frame::Galactic).rotation().unwrap();
    for (i, row) in derived.0.iter().enumerate() {
        for (j, value) in row.iter().enumerate() {
            let published = ICRS_TO_GALACTIC.0[i][j];
            assert!(
                (value - published).abs() < 1e-9,
                "({i}, {j}): {value} vs {published}"
            );
        }
    }
    assert!(derived.is_orthonormal());
    let product = derived.then_after(&derived.transpose());
    assert!(
        product
            .0
            .iter()
            .flatten()
            .zip(Rotation::IDENTITY.0.iter().flatten())
            .all(|(a, b)| (a - b).abs() < 1e-15)
    );
}

#[test]
fn the_galactic_centre_and_pole_land_where_hipparcos_puts_them() {
    // ESA 1997 §1.5.3: the north galactic pole at α = 192.85948°, δ = +27.12825°.
    let (_, b) = to(Frame::Galactic, 192.859_48, 27.128_25);
    assert!((b - 90.0).abs() < 1e-6, "pole at b = {b}");
    assert_near(
        to(Frame::Galactic, 266.404_988_286_5, -28.936_177_761_8),
        (0.0, 0.0),
        ASTROPY,
    );
}

#[test]
fn the_supergalactic_pole_and_origin_follow_lahav_2000() {
    let to_sg = galactic_to_supergalactic();
    let (_, sgb) = lon_lat(to_sg.apply(unit_vector(47.37, 6.32)));
    assert!((sgb - 90.0).abs() < 1e-9);
    let origin = lon_lat(to_sg.apply(unit_vector(137.37, 0.0)));
    assert_near(origin, (0.0, 0.0), 1e-9);
}

#[test]
fn m31_matches_astropy_in_every_frame() {
    let (ra, dec) = (10.684_708, 41.268_75);
    assert_near(
        to(Frame::Galactic, ra, dec),
        (121.174_328_795, -21.573_308_789),
        ASTROPY,
    );
    assert_near(
        to(Frame::Supergalactic, ra, dec),
        (336.192_971_751, 12.552_108_342),
        ASTROPY,
    );
    let back = between(Frame::Supergalactic, Frame::Icrs)
        .apply(from_icrs(Frame::Supergalactic).apply(unit_vector(ra, dec)));
    assert_near(lon_lat(back), (ra, dec), 1e-9);
}

#[test]
fn fk5_positions_move_to_the_icrs_as_astropy_moves_them() {
    // astropy 8.0.2: FK5(equinox="J2000") → ICRS, which applies ERFA's fk5hip rotation.
    let astropy = [
        [1.0, 1.110_223_371_627e-7, 4.411_803_426_626e-8],
        [-1.110_223_329_055e-7, 1.0, -9.647_792_746_537e-8],
        [-4.411_804_498_097e-8, 9.647_792_250_666e-8, 1.0],
    ];
    let ours = between(Frame::Fk5J2000, Frame::Icrs).rotation().unwrap();
    for (row, expected) in ours.0.iter().zip(astropy) {
        for (value, expected) in row.iter().zip(expected) {
            assert!((value - expected).abs() < 2e-14, "{value} vs {expected}");
        }
    }
    // A 6dFGS galaxy as VizieR gives it in FK5 J2000.
    let (ra, dec) = lon_lat(ours.apply(unit_vector(54.430_541_7, -22.908_194_4)));
    assert_near((ra, dec), (54.430_537_566_576, -22.908_191_374_027), 1e-11);
}

#[test]
fn a_longitude_just_below_zero_comes_back_as_zero_not_360() {
    assert_eq!(lon_lat([1.0, -1e-17, 0.0]).0, 0.0);
}

#[test]
fn positions_outside_their_range_are_refused() {
    assert!(SkyPosition::new(360.0, 0.0, Epoch::J2000).is_err());
    assert!(SkyPosition::new(10.0, 90.5, Epoch::J2000).is_err());
    let gaia = SkyPosition::new(10.0, -5.0, Epoch(2016.0)).unwrap();
    assert_near(lon_lat(gaia.unit_vector()), (10.0, -5.0), 1e-12);
}

/// B1950 positions moved by pyerfa 2.0.1.5 `erfa.fk45z(ra, dec, 1950.0)` (FK5 J2000), and on
/// to the ICRS by astropy 8.0.2's FK5 → ICRS: 3C 273 at its B1950 position from the lunar
/// occultations of Hazard et al. 1971 (12h26m33.246s, +02°19′43.29″), and two points of the
/// grid. astropy's own FK4 → ICRS path differs from fk45z by 0.09 to 1.4 mas on these.
const FK45Z: [((f64, f64), (f64, f64), (f64, f64)); 3] = [
    (
        (186.638_525, 2.328_691_666_666_666_4),
        (187.277_896_377_367_17, 2.052_348_156_826_204),
        (187.277_890_224_226_5, 2.052_349_963_969_006_4),
    ),
    (
        (0.0, 0.0),
        (0.640_690_976_989_889, 0.278_409_441_673_693_36),
        (0.640_684_588_882_531_9, 0.278_406_975_865_196_8),
    ),
    (
        (180.0, 80.0),
        (180.631_975_801_196, 79.721_691_471_704_94),
        (180.632_000_074_980_08, 79.721_693_938_357),
    ),
];

#[test]
fn fk4_b1950_positions_move_as_erfa_fk45z_moves_them() {
    for ((ra, dec), fk5, icrs) in FK45Z {
        let v = unit_vector(ra, dec);
        // 1e-9° is 3.6 µas.
        let to_fk5 = between(Frame::Fk4B1950, Frame::Fk5J2000).apply(v);
        assert_near(lon_lat(to_fk5), fk5, 1e-9);
        assert_near(
            lon_lat(between(Frame::Fk4B1950, Frame::Icrs).apply(v)),
            icrs,
            1e-9,
        );
        let back = between(Frame::Icrs, Frame::Fk4B1950).apply(unit_vector(icrs.0, icrs.1));
        assert_near(lon_lat(back), (ra, dec), 1e-11);
    }
    // A Cartesian position keeps its length; the transformation is not a rotation.
    let far = scaled(unit_vector(186.638_525, 2.328_691_666_666_666_4), 749.0);
    let moved = between(Frame::Fk4B1950, Frame::Galactic).apply(far);
    assert!((dot(moved, moved).sqrt() - 749.0).abs() < 1e-9);
    assert!(between(Frame::Fk4B1950, Frame::Icrs).rotation().is_none());
    assert!(
        between(Frame::Fk5J2000, Frame::Galactic)
            .rotation()
            .is_some()
    );
}

#[test]
fn an_unstated_equinox_is_placed_as_j2000_with_the_b1950_to_j2000_angle_as_its_error() {
    let (ra, dec) = (150.0, 20.0);
    let unstated = between(Frame::EquatorialEquinoxUnstated, Frame::Icrs);
    let j2000 = between(Frame::Fk5J2000, Frame::Icrs);
    assert_near(
        lon_lat(unstated.apply(unit_vector(ra, dec))),
        lon_lat(j2000.apply(unit_vector(ra, dec))),
        1e-12,
    );
    // The rotation angle of the Standish 1982 matrix plus 0.343″ of E-terms. General
    // precession over the 50 years gives nearly the same angle: 5029.0966″ a century × 0.5.
    let error = Frame::EquatorialEquinoxUnstated
        .systematic_uncertainty()
        .unwrap();
    assert!((error - 0.6986).abs() < 1e-3, "{error}°");
    let general_precession = 5_029.096_6 / 2.0 / 3_600.0;
    assert!((error - general_precession).abs() < 2e-3);
    // No position read as B1950 and as J2000 lies further apart than that.
    for (ra, dec) in [
        (0.0, 0.0),
        (90.0, 66.56),
        (270.0, -66.56),
        (180.0, 0.0),
        (45.0, 30.0),
    ] {
        let b1950 = between(Frame::Fk4B1950, Frame::Icrs).apply(unit_vector(ra, dec));
        let j2000 = j2000.apply(unit_vector(ra, dec));
        let apart = dot(b1950, j2000).clamp(-1.0, 1.0).acos().to_degrees();
        assert!(apart <= error, "({ra}, {dec}) moves {apart}°");
    }
    assert_eq!(Frame::Icrs.systematic_uncertainty(), None);
    assert_eq!(Frame::Fk4B1950.systematic_uncertainty(), None);
}

#[test]
fn the_new_frames_read_by_their_names() {
    let read: Vec<Frame> =
        serde_norway::from_str("[fk4_b1950, equatorial_equinox_unstated, fk5_j2000]").unwrap();
    assert_eq!(
        read,
        [
            Frame::Fk4B1950,
            Frame::EquatorialEquinoxUnstated,
            Frame::Fk5J2000
        ]
    );
}
