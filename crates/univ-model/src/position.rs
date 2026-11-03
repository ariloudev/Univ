//! Frames and positions: ICRS with an epoch, and rotations between it and FK5 J2000 (Mignard &
//! Froeschlé 2000), galactic (Hipparcos, ESA 1997) and supergalactic (Lahav et al. 2000)
//! coordinates.

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

/// The rotations between every pair of frames, indexed `[from][to]` in the order of [`Frame`],
/// computed once. A frame to itself is exactly the identity.
static BETWEEN: LazyLock<[[Rotation; 4]; 4]> = LazyLock::new(|| {
    let galactic = icrs_to_galactic();
    let from_icrs = [
        Rotation::IDENTITY,
        galactic,
        galactic_to_supergalactic().then_after(&galactic),
        fk5_to_icrs().transpose(),
    ];
    from_fn(|from| {
        from_fn(|to| {
            if from == to {
                Rotation::IDENTITY
            } else {
                from_icrs[to].then_after(&from_icrs[from].transpose())
            }
        })
    })
});

/// The rotation that takes an ICRS vector into `frame`.
#[must_use]
pub fn from_icrs(frame: Frame) -> Rotation {
    between(Frame::Icrs, frame)
}

/// The rotation that takes a vector in `from` into `to`.
#[must_use]
pub fn between(from: Frame, to: Frame) -> Rotation {
    BETWEEN[from as usize][to as usize]
}

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
    fn the_galactic_rotation_matches_the_hipparcos_matrix_and_is_orthonormal() {
        let derived = from_icrs(Frame::Galactic);
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
        let ours = between(Frame::Fk5J2000, Frame::Icrs);
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
}
