//! Frames and positions: ICRS with an epoch, and rotations to galactic (Hipparcos, ESA 1997) and
//! supergalactic (Lahav et al. 2000) coordinates.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A Cartesian vector; its frame and unit are held by whatever holds it.
pub type Vec3 = [f64; 3];

/// A rotation between frames, applied as `matrix · vector`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rotation(pub [[f64; 3]; 3]);

/// A celestial reference frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Frame {
    /// The International Celestial Reference System, equatorial.
    Icrs,
    /// Galactic coordinates as the Hipparcos catalogue defines them from the ICRS (ESA 1997, vol. 1,
    /// §1.5.3).
    Galactic,
    /// Supergalactic coordinates with the pole and origin of Lahav et al. 2000 (MNRAS 312, 166).
    Supergalactic,
}

/// The ICRS → galactic rotation of the Hipparcos catalogue (ESA 1997, vol. 1, §1.5.3):
/// the transpose of its matrix `A_G`. The rows are the galactic x (toward the centre), y (toward
/// l = 90°) and z (toward the north galactic pole) axes in ICRS.
pub const ICRS_TO_GALACTIC: Rotation = Rotation([
    [-0.054_875_560_4, -0.873_437_090_2, -0.483_835_015_5],
    [0.494_109_427_9, -0.444_829_630_0, 0.746_982_244_5],
    [-0.867_666_149_0, -0.198_076_373_4, 0.455_983_776_2],
]);

/// The supergalactic north pole in galactic coordinates (Lahav et al. 2000, MNRAS 312, 166), degrees.
const SUPERGALACTIC_POLE: (f64, f64) = (47.37, 6.32);
/// The supergalactic origin, SGL = SGB = 0, in galactic coordinates, degrees.
const SUPERGALACTIC_ORIGIN: (f64, f64) = (137.37, 0.0);

/// An epoch as a Julian year, such as 2000.0 for J2000.0 or 2016.0 for Gaia DR3.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Epoch(pub f64);

impl Epoch {
    pub const J2000: Self = Self(2000.0);
}

/// A direction on the sky in ICRS at the epoch of its source, in degrees.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkyPosition {
    pub ra: f64,
    pub dec: f64,
    pub epoch: Epoch,
}

/// A sky position that cannot stand.
#[derive(Debug, Error, PartialEq)]
pub enum PositionError {
    #[error("right ascension {0}° lies outside [0, 360)")]
    RightAscension(f64),
    #[error("declination {0}° lies outside [-90, 90]")]
    Declination(f64),
}

impl SkyPosition {
    /// A checked ICRS position.
    ///
    /// # Errors
    ///
    /// Fails when the right ascension or declination lies outside its range.
    pub fn new(ra: f64, dec: f64, epoch: Epoch) -> Result<Self, PositionError> {
        if !(0.0..360.0).contains(&ra) {
            return Err(PositionError::RightAscension(ra));
        }
        if !(-90.0..=90.0).contains(&dec) {
            return Err(PositionError::Declination(dec));
        }
        Ok(Self { ra, dec, epoch })
    }

    /// The unit vector toward this position in ICRS.
    #[must_use]
    pub fn unit_vector(&self) -> Vec3 {
        unit_vector(self.ra, self.dec)
    }
}

/// The unit vector toward longitude `lon` and latitude `lat`, in degrees, of any frame.
#[must_use]
pub fn unit_vector(lon: f64, lat: f64) -> Vec3 {
    let (lon, lat) = (lon.to_radians(), lat.to_radians());
    [lat.cos() * lon.cos(), lat.cos() * lon.sin(), lat.sin()]
}

/// The longitude in [0, 360) and latitude of a vector, in degrees.
#[must_use]
pub fn lon_lat(v: Vec3) -> (f64, f64) {
    let lon = v[1].atan2(v[0]).to_degrees().rem_euclid(360.0);
    let lat = v[2].atan2(v[0].hypot(v[1])).to_degrees();
    (lon, lat)
}

impl Rotation {
    pub const IDENTITY: Self = Self([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);

    /// `self · v`.
    #[must_use]
    pub fn apply(&self, v: Vec3) -> Vec3 {
        let m = &self.0;
        [dot(m[0], v), dot(m[1], v), dot(m[2], v)]
    }

    /// The inverse rotation.
    #[must_use]
    pub fn transpose(&self) -> Self {
        let m = &self.0;
        Self([
            [m[0][0], m[1][0], m[2][0]],
            [m[0][1], m[1][1], m[2][1]],
            [m[0][2], m[1][2], m[2][2]],
        ])
    }

    /// `self · other`: `other` applied first.
    #[must_use]
    pub fn then_after(&self, other: &Self) -> Self {
        let t = other.transpose();
        let row = |r: [f64; 3]| [dot(r, t.0[0]), dot(r, t.0[1]), dot(r, t.0[2])];
        Self([row(self.0[0]), row(self.0[1]), row(self.0[2])])
    }
}

/// The galactic → supergalactic rotation, built from the pole and origin of Lahav et al. 2000.
#[must_use]
pub fn galactic_to_supergalactic() -> Rotation {
    let z = unit_vector(SUPERGALACTIC_POLE.0, SUPERGALACTIC_POLE.1);
    let x = unit_vector(SUPERGALACTIC_ORIGIN.0, SUPERGALACTIC_ORIGIN.1);
    Rotation([x, cross(z, x), z])
}

/// The rotation that takes an ICRS vector into `frame`.
#[must_use]
pub fn from_icrs(frame: Frame) -> Rotation {
    match frame {
        Frame::Icrs => Rotation::IDENTITY,
        Frame::Galactic => ICRS_TO_GALACTIC,
        Frame::Supergalactic => galactic_to_supergalactic().then_after(&ICRS_TO_GALACTIC),
    }
}

/// The rotation that takes a vector in `from` into `to`.
#[must_use]
pub fn between(from: Frame, to: Frame) -> Rotation {
    if from == to {
        return Rotation::IDENTITY;
    }
    from_icrs(to).then_after(&from_icrs(from).transpose())
}

/// The dot product.
#[must_use]
pub fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// The cross product.
#[must_use]
pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[cfg(test)]
mod tests {
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
    fn the_hipparcos_rotation_is_orthonormal() {
        let product = ICRS_TO_GALACTIC.then_after(&ICRS_TO_GALACTIC.transpose());
        for (i, row) in product.0.iter().enumerate() {
            for (j, value) in row.iter().enumerate() {
                let expected = if i == j { 1.0 } else { 0.0 };
                assert!((value - expected).abs() < 1e-9, "({i}, {j}) = {value}");
            }
        }
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
        // The published matrix has 10 digits, so its transpose inverts it to about 1e-10.
        assert_near(lon_lat(back), (ra, dec), 1e-7);
    }

    #[test]
    fn positions_outside_their_range_are_refused() {
        assert!(SkyPosition::new(360.0, 0.0, Epoch::J2000).is_err());
        assert!(SkyPosition::new(10.0, 90.5, Epoch::J2000).is_err());
        let gaia = SkyPosition::new(10.0, -5.0, Epoch(2016.0)).unwrap();
        assert_near(lon_lat(gaia.unit_vector()), (10.0, -5.0), 1e-12);
    }
}
