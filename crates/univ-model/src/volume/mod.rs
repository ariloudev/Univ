//! A bounded volume of a query (query-a-bounded-volume): a box, a sphere or a cone in the ICRS,
//! galactic or supergalactic frame, in a unit of length, and the region of ICRS space it
//! encloses, against which positions and geometries are tested.
//!
//! - A box is axis-aligned in its own frame, from `min` to `max`.
//! - A sphere is its `centre` and `radius`.
//! - A cone's apex is the frame's origin, the Solar System barycentre. It is given by the
//!   longitude and latitude of its axis in its frame, its full opening angle, at most 180°, and
//!   the distances from the apex it spans, `near` to `far`: the part of the spherical shell
//!   from `near` to `far` within half the opening angle of the axis.

mod meets;
#[cfg(test)]
mod meets_tests;
mod refine;
mod region;
#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

use crate::position::{Frame, Rotation, Vec3, between, unit_vector};
use crate::units::{Dimension, Unit};

pub use refine::{REFINEMENT_BUDGET, TOLERANCE};
pub use region::Region;

/// The widest a cone opens, in degrees: beyond it a cone is no longer convex.
pub const MAX_OPENING_DEGREES: f64 = 180.0;

/// A volume as a query gives it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Volume {
    /// `icrs`, `galactic` or `supergalactic`; the axes are Cartesian about the barycentre.
    pub frame: Frame,
    /// A unit of length, such as `parsec` or `megaparsec`.
    pub unit: Unit,
    #[serde(flatten)]
    pub form: VolumeForm,
}

/// The form of a volume, by its member `form`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case", tag = "form")]
pub enum VolumeForm {
    /// Axis-aligned in the volume's frame.
    Box {
        #[cfg_attr(feature = "openapi", schema(value_type = Vec<f64>, min_items = 3, max_items = 3))]
        min: Vec3,
        #[cfg_attr(feature = "openapi", schema(value_type = Vec<f64>, min_items = 3, max_items = 3))]
        max: Vec3,
    },
    Sphere {
        #[cfg_attr(feature = "openapi", schema(value_type = Vec<f64>, min_items = 3, max_items = 3))]
        centre: Vec3,
        radius: f64,
    },
    /// Apex at the barycentre; `lon`, `lat` and `opening_angle` in degrees.
    Cone {
        lon: f64,
        lat: f64,
        opening_angle: f64,
        near: f64,
        far: f64,
    },
}

/// A member of a volume that cannot stand, named relative to the volume, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VolumeError {
    pub member: &'static str,
    pub reason: String,
}

impl VolumeError {
    fn new(member: &'static str, reason: impl Into<String>) -> Self {
        Self {
            member,
            reason: reason.into(),
        }
    }
}

fn finite(v: &Vec3) -> bool {
    v.iter().all(|c| c.is_finite())
}

/// What a volume's frame must be.
pub const FRAME_RULE: &str = "is icrs, galactic or supergalactic";
/// What a volume's unit must be.
pub const UNIT_RULE: &str = "is a unit of length, such as parsec or megaparsec";

/// The refusal of a frame a volume cannot be given in: one that is not ICRS, galactic or
/// supergalactic.
#[must_use]
pub fn check_frame(frame: Frame) -> Option<VolumeError> {
    (!matches!(frame, Frame::Icrs | Frame::Galactic | Frame::Supergalactic))
        .then(|| VolumeError::new("frame", FRAME_RULE))
}

/// The refusal of a unit a volume cannot be given in: one that is not a length.
#[must_use]
pub fn check_unit(unit: Unit) -> Option<VolumeError> {
    (unit.dimension() != Dimension::Length).then(|| VolumeError::new("unit", UNIT_RULE))
}

impl VolumeForm {
    /// Every member of the form that cannot stand: a size that is not positive, a box whose
    /// `max` is not above its `min` on every axis, or a cone wider than
    /// [`MAX_OPENING_DEGREES`].
    #[must_use]
    pub fn check(&self) -> Vec<VolumeError> {
        let mut errors = Vec::new();
        match self {
            VolumeForm::Box { min, max } => {
                if !finite(min) {
                    errors.push(VolumeError::new("min", "has three finite coordinates"));
                }
                if !finite(max) {
                    errors.push(VolumeError::new("max", "has three finite coordinates"));
                } else if finite(min) && (0..3).any(|i| max[i] <= min[i]) {
                    errors.push(VolumeError::new(
                        "max",
                        "is above min on every axis: a box has a positive size",
                    ));
                }
            }
            VolumeForm::Sphere { centre, radius } => {
                if !finite(centre) {
                    errors.push(VolumeError::new("centre", "has three finite coordinates"));
                }
                if !(radius.is_finite() && *radius > 0.0) {
                    errors.push(VolumeError::new("radius", "is positive and finite"));
                }
            }
            VolumeForm::Cone {
                lon,
                lat,
                opening_angle,
                near,
                far,
            } => {
                if !lon.is_finite() {
                    errors.push(VolumeError::new("lon", "is a finite angle in degrees"));
                }
                if !(lat.is_finite() && (-90.0..=90.0).contains(lat)) {
                    errors.push(VolumeError::new("lat", "is from -90 to 90 degrees"));
                }
                if !(opening_angle.is_finite()
                    && *opening_angle > 0.0
                    && *opening_angle <= MAX_OPENING_DEGREES)
                {
                    errors.push(VolumeError::new(
                        "opening_angle",
                        "is the cone's full width, above 0 and at most 180 degrees",
                    ));
                }
                if !(near.is_finite() && *near >= 0.0) {
                    errors.push(VolumeError::new("near", "is 0 or more and finite"));
                }
                if !(far.is_finite() && *far > 0.0) {
                    errors.push(VolumeError::new("far", "is positive and finite"));
                } else if near.is_finite() && far <= near {
                    errors.push(VolumeError::new("far", "is beyond near"));
                }
            }
        }
        errors
    }
}

impl Volume {
    /// Every member that cannot stand: a frame other than ICRS, galactic or supergalactic, a
    /// unit that is not a length, or a member of its form (see [`VolumeForm::check`]).
    #[must_use]
    pub fn check(&self) -> Vec<VolumeError> {
        check_frame(self.frame)
            .into_iter()
            .chain(check_unit(self.unit))
            .chain(self.form.check())
            .collect()
    }

    /// The region the volume encloses, in the ICRS and in `unit`; none for a volume that
    /// [`Volume::check`] refuses.
    #[must_use]
    pub fn region_in(&self, unit: Unit) -> Option<Region> {
        if !self.check().is_empty() {
            return None;
        }
        let factor = self.unit.factor_to(unit).ok()?;
        let to_icrs = between(self.frame, Frame::Icrs).rotation()?;
        Some(self.region(&to_icrs).scaled(factor))
    }

    /// The region in the ICRS, in the volume's own unit.
    fn region(&self, to_icrs: &Rotation) -> Region {
        match &self.form {
            VolumeForm::Box { min, max } => Region::Box {
                to_frame: to_icrs.transpose(),
                centre: std::array::from_fn(|i| f64::midpoint(min[i], max[i])),
                half: std::array::from_fn(|i| (max[i] - min[i]) / 2.0),
            },
            VolumeForm::Sphere { centre, radius } => Region::Sphere {
                centre: to_icrs.apply(*centre),
                radius: *radius,
            },
            VolumeForm::Cone {
                lon,
                lat,
                opening_angle,
                near,
                far,
            } => Region::Sector {
                axis: to_icrs.apply(unit_vector(*lon, *lat)),
                half_angle: (opening_angle / 2.0).to_radians(),
                near: *near,
                far: *far,
            },
        }
    }
}
