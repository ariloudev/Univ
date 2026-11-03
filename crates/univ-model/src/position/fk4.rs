//! FK4 at equinox B1950 to FK5 at J2000, as ERFA's `fk45z` does it: the E-terms of aberration
//! removed, then the FK4 to FK5 matrix of Standish 1982 (A&A 115, 20), in the form of Aoki et
//! al. 1983 (A&A 128, 263) and Seidelmann 1992 (Explanatory Supplement, §3.591), with zero
//! proper motion in FK5 assumed. Extragalactic positions have none, so the assumption is exact
//! for them.

use super::{Rotation, Vec3, dot, scaled, zip};

/// The E-terms of aberration in FK4 at B1950, in radians (Seidelmann 1992, eq. 3.591-2; ERFA
/// `fk45z` vector `a`). Their rate, `adot`, is multiplied by the time from B1950, which is zero
/// for a B1950 position.
pub(super) const E_TERMS: Vec3 = [-1.625_57e-6, -0.319_19e-6, -0.138_43e-6];

/// The position-to-position block of the 6 × 6 FK4 to FK5 matrix (Standish 1982; ERFA `fk425`
/// matrix `em[i][0]`): the rotation from the B1950 equator and equinox to J2000.
pub(super) const POSITION: [[f64; 3]; 3] = [
    [0.999_925_678_2, -0.011_182_061_1, -0.004_857_947_7],
    [0.011_182_061_0, 0.999_937_478_4, -0.000_027_176_5],
    [0.004_857_947_9, -0.000_027_147_4, 0.999_988_199_7],
];

/// The position-to-velocity block of the same matrix (ERFA `fk425` `em[i][1]` rows of the
/// lower half), in arcseconds per century: the fictitious proper motion that FK4's rotation
/// against an inertial frame gives a source fixed in FK5.
const VELOCITY: [[f64; 3]; 3] = [
    [-0.000_551, -0.238_565, 0.435_739],
    [0.238_514, -0.002_667, -0.008_541],
    [-0.435_623, 0.012_254, 0.002_117],
];

/// Arcseconds per century in one radian per year (ERFA `PMF`).
const PMF: f64 = 100.0 * 206_264.806_247_096_36;

/// The Julian epoch of B1950.0, 1949.99979: the Besselian epoch through its modified Julian
/// date, 15019.81352 + 50 × 365.242198781 (ERFA `epb2jd`), to a Julian year (ERFA `epj`).
fn b1950_as_julian_epoch() -> f64 {
    let mjd = 15_019.813_52 + 50.0 * 365.242_198_781;
    2000.0 + (mjd - 51_544.5) / 365.25
}

/// The FK4 (without E-terms) to FK5 J2000 matrix for a B1950 position of zero FK5 proper
/// motion: the position block plus the velocity block times the time from J2000 to B1950, as
/// `fk45z` applies it. It is a rotation to about 1e-6, not exactly.
pub(super) fn fk4_to_fk5() -> Rotation {
    let w = (b1950_as_julian_epoch() - 2000.0) / PMF;
    Rotation(std::array::from_fn(|i| {
        zip(POSITION[i], VELOCITY[i], |p, v| p + w * v)
    }))
}

/// The inverse of a 3 × 3 matrix, by its adjugate. Only ever called on the near-rotation of
/// [`fk4_to_fk5`], whose determinant is 1 to about 1e-6.
pub(super) fn inverse(m: &Rotation) -> Rotation {
    let [a, b, c] = m.0;
    let cofactors = [super::cross(b, c), super::cross(c, a), super::cross(a, b)];
    let determinant = dot(a, cofactors[0]);
    // The rows of the adjugate are the columns of the cofactor matrix.
    Rotation(std::array::from_fn(|i| {
        std::array::from_fn(|j| cofactors[j][i] / determinant)
    }))
}

/// The unit vector `u` scaled to length one.
pub(super) fn normalized(u: Vec3) -> Vec3 {
    scaled(u, 1.0 / dot(u, u).sqrt())
}

/// A catalogue direction with its E-terms removed: `u − A + (u · A) u` (ERFA `fk45z`).
pub(super) fn remove_e_terms(u: Vec3) -> Vec3 {
    let along = dot(u, E_TERMS);
    std::array::from_fn(|i| u[i] - E_TERMS[i] + along * u[i])
}

/// The catalogue direction whose E-terms removed give the direction `p`: `p + A` normalized,
/// refined twice, which leaves an error below 1e-15 rad as A is 1.7e-6 rad.
pub(super) fn add_e_terms(p: Vec3) -> Vec3 {
    let p = normalized(p);
    let mut r = normalized(zip(p, E_TERMS, |a, b| a + b));
    for _ in 0..2 {
        let length = dot(remove_e_terms(r), remove_e_terms(r)).sqrt();
        r = normalized(zip(scaled(p, length), E_TERMS, |a, b| a + b));
    }
    r
}

/// The largest angle, in degrees, between a direction read in B1950 (FK4) and the same numbers
/// read in J2000: the angle of the precession rotation between the two equators and equinoxes,
/// `acos((trace − 1) / 2)` of [`POSITION`], 0.6985°, which general precession over the 50 years
/// also gives (50.29″ a year × 50 = 2515″ = 0.6985°), plus the E-terms' 0.343″. A point 90°
/// from the rotation axis moves by the whole angle; others by less.
#[must_use]
pub fn equinox_ambiguity() -> f64 {
    let trace = POSITION[0][0] + POSITION[1][1] + POSITION[2][2];
    let precession = ((trace - 1.0) / 2.0).acos();
    let e_terms = dot(E_TERMS, E_TERMS).sqrt();
    (precession + e_terms).to_degrees()
}

#[cfg(test)]
mod tests {
    use super::super::{Frame, between, dot, lon_lat, scaled, separation_degrees, unit_vector};

    fn assert_near(actual: (f64, f64), expected: (f64, f64), tolerance: f64) {
        let dlon = (actual.0 - expected.0 + 180.0).rem_euclid(360.0) - 180.0;
        assert!(
            dlon.abs() < tolerance && (actual.1 - expected.1).abs() < tolerance,
            "{actual:?} is not within {tolerance}° of {expected:?}"
        );
    }

    /// B1950 positions moved by pyerfa 2.0.1.5 `erfa.fk45z(ra, dec, 1950.0)` (FK5 J2000), and on
    /// to the ICRS by astropy 8.0.2's FK5 → ICRS: 3C 273 at its B1950 position from the lunar
    /// occultations of Hazard et al. 1971 (12h26m33.246s, +02°19′43.29″), and two points of the
    /// grid. astropy's own FK4 → ICRS path differs from fk45z by 0.09 to 1.4 mas on these.
    /// A longitude and latitude in degrees.
    type LonLat = (f64, f64);

    /// Each point in FK4 B1950, FK5 J2000 and the ICRS.
    const FK45Z: [(LonLat, LonLat, LonLat); 3] = [
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
            let apart = separation_degrees(b1950, j2000);
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
}
