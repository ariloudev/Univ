//! Frames and positions: ICRS with an epoch, and transformations between it and FK5 J2000
//! (Mignard & Froeschlé 2000), FK4 B1950 (Standish 1982, with E-terms), galactic (Hipparcos,
//! ESA 1997) and supergalactic (Lahav et al. 2000) coordinates, and equatorial coordinates whose
//! equinox is not stated.

mod fk4;

use std::array::from_fn;
use std::sync::LazyLock;

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
    /// FK5 at equinox and epoch J2000, in which `VizieR` gives the positions it computes. It
    /// differs from the ICRS by tens of milliarcseconds; see [`fk5_to_icrs`].
    #[serde(rename = "fk5_j2000")]
    Fk5J2000,
    /// FK4 at equinox B1950 with its E-terms of aberration, as older papers publish positions.
    /// It moves to FK5 J2000 as ERFA's `fk45z` moves it, assuming zero proper motion in FK5,
    /// and from there to the ICRS.
    #[serde(rename = "fk4_b1950")]
    Fk4B1950,
    /// Equatorial coordinates whose paper does not state the equinox. They are placed as FK5
    /// J2000, never silently: [`Frame::systematic_uncertainty`] gives the B1950 to J2000
    /// precession angle, about 0.7°, which a position in it must carry.
    EquatorialEquinoxUnstated,
}

/// The number of frames, the size of the table of transformations.
const FRAMES: usize = 6;

impl Frame {
    /// The systematic uncertainty, in degrees, that a direction published in this frame carries
    /// beyond its own errors: for an unstated equinox, the largest angle between reading it as
    /// B1950 and as J2000 (see [`equinox_ambiguity`]); none for every other frame.
    #[must_use]
    pub fn systematic_uncertainty(self) -> Option<f64> {
        (self == Self::EquatorialEquinoxUnstated).then(fk4::equinox_ambiguity)
    }
}

/// A transformation between frames: a rotation, except to or from FK4 B1950, whose E-terms of
/// aberration depend on the direction and are removed before the matrix or added after it.
/// It applies to directions and to Cartesian positions, whose length it keeps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    matrix: Rotation,
    remove_e_terms: bool,
    add_e_terms: bool,
}

impl Transform {
    const IDENTITY: Self = Self {
        matrix: Rotation::IDENTITY,
        remove_e_terms: false,
        add_e_terms: false,
    };

    /// The vector `v` in the target frame.
    #[must_use]
    pub fn apply(&self, v: Vec3) -> Vec3 {
        if !(self.remove_e_terms || self.add_e_terms) {
            return self.matrix.apply(v);
        }
        let length = dot(v, v).sqrt();
        if length == 0.0 {
            return v;
        }
        let mut u = scaled(v, 1.0 / length);
        if self.remove_e_terms {
            u = fk4::remove_e_terms(u);
        }
        u = fk4::normalized(self.matrix.apply(u));
        if self.add_e_terms {
            u = fk4::add_e_terms(u);
        }
        scaled(u, length)
    }

    /// The rotation, when the transformation is one: when neither frame is FK4 B1950.
    #[must_use]
    pub fn rotation(&self) -> Option<Rotation> {
        (!(self.remove_e_terms || self.add_e_terms)).then_some(self.matrix)
    }
}

/// The ICRS → galactic rotation as the Hipparcos catalogue prints it (ESA 1997, vol. 1, §1.5.3):
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
#[serde(try_from = "RawSkyPosition")]
pub struct SkyPosition {
    pub ra: f64,
    pub dec: f64,
    pub epoch: Epoch,
}

/// A sky position as read, before its checks.
#[derive(Deserialize)]
struct RawSkyPosition {
    ra: f64,
    dec: f64,
    epoch: Epoch,
}

impl TryFrom<RawSkyPosition> for SkyPosition {
    type Error = PositionError;
    fn try_from(raw: RawSkyPosition) -> Result<Self, PositionError> {
        Self::new(raw.ra, raw.dec, raw.epoch)
    }
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
    // `rem_euclid` rounds a tiny negative angle up to exactly 360.
    let lon = v[1].atan2(v[0]).to_degrees().rem_euclid(360.0);
    let lon = if lon >= 360.0 { 0.0 } else { lon };
    let lat = v[2].atan2(v[0].hypot(v[1])).to_degrees();
    (lon, lat)
}

impl Rotation {
    pub const IDENTITY: Self = Self([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);

    /// `self · v`.
    #[must_use]
    pub fn apply(&self, v: Vec3) -> Vec3 {
        self.0.map(|row| dot(row, v))
    }

    /// The inverse rotation.
    #[must_use]
    pub fn transpose(&self) -> Self {
        Self(from_fn(|i| from_fn(|j| self.0[j][i])))
    }

    /// Whether the rows are orthogonal unit vectors, to 1e-9.
    #[must_use]
    pub fn is_orthonormal(&self) -> bool {
        let product = self.then_after(&self.transpose());
        product
            .0
            .iter()
            .flatten()
            .zip(Self::IDENTITY.0.iter().flatten())
            .all(|(a, b)| (a - b).abs() < 1e-9)
    }

    /// `self · other`: `other` applied first.
    #[must_use]
    pub fn then_after(&self, other: &Self) -> Self {
        let columns = other.transpose();
        Self(self.0.map(|row| columns.apply(row)))
    }
}

/// The rotation from equatorial axes to the axes of a frame given by its north pole and the
/// longitude, in that frame, of the ascending node of its equator on the celestial equator.
fn from_pole_and_node(pole: Vec3, node_longitude: f64) -> Rotation {
    let node = unit_vector(lon_lat(pole).0 + 90.0, 0.0);
    let (sin, cos) = node_longitude.to_radians().sin_cos();
    let x = zip(node, cross(pole, node), |n, m| cos * n - sin * m);
    Rotation([x, cross(pole, x), pole])
}

/// The ICRS → galactic rotation, derived from the angles that define it in the Hipparcos
/// catalogue (ESA 1997, vol. 1, §1.5.3): the north galactic pole at α = 192.85948°,
/// δ = +27.12825°, and the ascending node at l = 32.93192°. It agrees with the published
/// [`ICRS_TO_GALACTIC`] to its 10 digits, and is orthonormal to rounding.
fn icrs_to_galactic() -> Rotation {
    from_pole_and_node(unit_vector(192.859_48, 27.128_25), 32.931_92)
}

/// The galactic → supergalactic rotation, built from the pole and origin of Lahav et al. 2000.
fn galactic_to_supergalactic() -> Rotation {
    let z = unit_vector(SUPERGALACTIC_POLE.0, SUPERGALACTIC_POLE.1);
    let x = unit_vector(SUPERGALACTIC_ORIGIN.0, SUPERGALACTIC_ORIGIN.1);
    Rotation([x, cross(z, x), z])
}

/// The FK5 J2000 → ICRS rotation: the orientation of the Hipparcos frame, and so the ICRS,
/// against FK5 found by Mignard & Froeschlé 2000 (A&A 354, 732), −19.9, −9.1 and +22.9 mas about
/// x, y and z, built as ERFA's `fk5hip` builds it and as astropy applies it. FK5's spin of about
/// 0.3 mas a year is left out: it moves a J2000 position by nothing.
fn fk5_to_icrs() -> Rotation {
    let mas = |m: f64| (m / 3_600_000.0).to_radians();
    let vector = [mas(-19.9), mas(-9.1), mas(22.9)];
    let angle = dot(vector, vector).sqrt();
    let [ax, ay, az] = vector.map(|c| c / angle);
    let (sin, cos) = angle.sin_cos();
    let vers = 1.0 - cos;
    Rotation([
        [
            ax * ax * vers + cos,
            ax * ay * vers + az * sin,
            ax * az * vers - ay * sin,
        ],
        [
            ay * ax * vers - az * sin,
            ay * ay * vers + cos,
            ay * az * vers + ax * sin,
        ],
        [
            az * ax * vers + ay * sin,
            az * ay * vers - ax * sin,
            az * az * vers + cos,
        ],
    ])
}

/// The transformations between every pair of frames, indexed `[from][to]` in the order of
/// [`Frame`], computed once. A frame to itself is exactly the identity.
static BETWEEN: LazyLock<[[Transform; FRAMES]; FRAMES]> = LazyLock::new(|| {
    let galactic = icrs_to_galactic();
    let fk5 = fk5_to_icrs();
    let fk4_matrix = fk5.then_after(&fk4::fk4_to_fk5());
    // Each frame's matrix into the ICRS and out of it. Only FK4's is not a rotation.
    let to_icrs = [
        Rotation::IDENTITY,
        galactic.transpose(),
        galactic_to_supergalactic()
            .then_after(&galactic)
            .transpose(),
        fk5,
        fk4_matrix,
        fk5,
    ];
    let fk4 = Frame::Fk4B1950 as usize;
    let mut from_icrs = to_icrs.map(|m| m.transpose());
    from_icrs[fk4] = fk4::inverse(&fk4_matrix);
    from_fn(|from| {
        from_fn(|to| {
            if from == to {
                Transform::IDENTITY
            } else {
                Transform {
                    matrix: from_icrs[to].then_after(&to_icrs[from]),
                    remove_e_terms: from == fk4,
                    add_e_terms: to == fk4,
                }
            }
        })
    })
});

/// The transformation that takes an ICRS vector into `frame`.
#[must_use]
pub fn from_icrs(frame: Frame) -> Transform {
    between(Frame::Icrs, frame)
}

/// The transformation that takes a vector in `from` into `to`.
#[must_use]
pub fn between(from: Frame, to: Frame) -> Transform {
    BETWEEN[from as usize][to as usize]
}

pub use fk4::equinox_ambiguity;

/// The dot product.
#[must_use]
pub fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// `f` applied to the matching components of `a` and `b`.
#[must_use]
pub fn zip(a: Vec3, b: Vec3, f: impl Fn(f64, f64) -> f64) -> Vec3 {
    from_fn(|i| f(a[i], b[i]))
}

/// `v` times `s`.
#[must_use]
pub fn scaled(v: Vec3, s: f64) -> Vec3 {
    v.map(|c| c * s)
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
mod tests;
