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
