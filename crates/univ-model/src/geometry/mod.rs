//! The closed set of geometry kinds and their checks; [`bounds`] gives each its bounding box.

mod bounds;

pub use bounds::Aabb;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::ids::ObjectId;
use crate::position::{Frame, Rotation, Vec3, between, dot};
use crate::units::{Dimension, Unit};
use crate::value::positive;

/// A geometry in a named frame, its Cartesian coordinates in one length unit. Checked when built
/// and when read.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawShape")]
pub struct Shape {
    pub frame: Frame,
    pub unit: Unit,
    pub geometry: Geometry,
}

/// A shape as read, before its checks.
#[derive(Deserialize)]
struct RawShape {
    frame: Frame,
    unit: Unit,
    geometry: Geometry,
}

/// The shape that places an object, structure or layer item. Meshes are not among them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Geometry {
    Point {
        at: Vec3,
    },
    /// A line of sight from `origin` along the unit vector `direction`, out to `reach`, the edge
    /// of the volume it is drawn in.
    Ray {
        origin: Vec3,
        direction: Vec3,
        reach: f64,
    },
    /// A unit `direction` whose distance is known only to lie between `near` and `far`.
    Range {
        direction: Vec3,
        near: f64,
        far: f64,
    },
    Primitive {
        primitive: Primitive,
    },
    /// A polyline through two or more points.
    Path {
        points: Vec<Vec3>,
    },
    Members {
        members: Vec<Member>,
    },
    /// A region of the sky as `HEALPix` cells (a MOC), between two distances.
    SkyRegion {
        order: u8,
        cells: Vec<u64>,
        near: f64,
        far: f64,
    },
    /// A grid of `shape` cells of edge `cell` from the corner `origin`, axis-aligned in its frame.
    VolumeGrid {
        grid: String,
        origin: Vec3,
        cell: f64,
        shape: [u32; 3],
    },
    /// Some cells of a volume grid: a region of `grid` outlined on its cells, such as a void
    /// where a density field falls below a threshold. The grid is laid out as for
    /// [`Geometry::VolumeGrid`]; each cell is given by its indices along X, Y and Z of the frame.
    GridRegion {
        grid: String,
        origin: Vec3,
        cell: f64,
        shape: [u32; 3],
        cells: Vec<[u32; 3]>,
    },
}

/// A solid of a simple form.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "form")]
pub enum Primitive {
    Sphere {
        centre: Vec3,
        radius: f64,
    },
    /// Semi-axes along the columns of `orientation`, a rotation from the solid's axes to the frame.
    Ellipsoid {
        centre: Vec3,
        semi_axes: Vec3,
        orientation: Rotation,
    },
    /// Half-extents along the columns of `orientation`.
    OrientedBox {
        centre: Vec3,
        half_extents: Vec3,
        orientation: Rotation,
    },
}

/// A member of a structure: an object, or a tracer with its own position and kind. An object
/// member carries the position it was resolved to at ingest, in the shape's frame and unit, so a
/// member list is bounded without looking its objects up.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "member")]
pub enum Member {
    Object { id: ObjectId, at: Vec3 },
    Tracer { at: Vec3, kind: String },
}

/// A geometry that cannot stand.
#[derive(Debug, Error, PartialEq)]
pub enum GeometryError {
    #[error("{0:?} is not a unit of length")]
    NotALength(Unit),
    #[error("a {0} needs {1}")]
    Malformed(&'static str, &'static str),
    #[error("a shape lies in a frame that rotates into the others, not {0:?}")]
    NotARotatedFrame(Frame),
}

fn finite(v: &Vec3) -> bool {
    v.iter().all(|c| c.is_finite())
}

fn unit_length(v: &Vec3) -> bool {
    finite(v) && (dot(*v, *v) - 1.0).abs() < 1e-9
}

/// `0 ≤ near ≤ far < ∞`.
fn interval(near: f64, far: f64) -> bool {
    near >= 0.0 && near <= far && far.is_finite()
}

impl Shape {
    /// A shape, checked: a unit of length and a well-formed geometry.
    ///
    /// # Errors
    ///
    /// Fails on a unit that is not a length or a geometry that cannot stand.
    pub fn new(frame: Frame, unit: Unit, geometry: Geometry) -> Result<Self, GeometryError> {
        if unit.dimension() != Dimension::Length {
            return Err(GeometryError::NotALength(unit));
        }
        // FK4's E-terms bend a box, and an unstated equinox carries a systematic error a shape
        // cannot hold: the pipeline moves such coordinates to the ICRS first.
        if between(frame, Frame::Icrs).rotation().is_none()
            || frame.systematic_uncertainty().is_some()
        {
            return Err(GeometryError::NotARotatedFrame(frame));
        }
        geometry.check()?;
        Ok(Self {
            frame,
            unit,
            geometry,
        })
    }

    /// The bounding box in `frame`, in this shape's unit; a member list is bounded by the
    /// positions its members carry. There is no box in FK4 B1950, which is not a rotation of the
    /// other frames.
    #[must_use]
    pub fn bounds_in(&self, frame: Frame) -> Option<Aabb> {
        let rotation = between(self.frame, frame).rotation()?;
        self.geometry.bounds(&rotation)
    }
}

impl TryFrom<RawShape> for Shape {
    type Error = GeometryError;
    fn try_from(raw: RawShape) -> Result<Self, GeometryError> {
        Self::new(raw.frame, raw.unit, raw.geometry)
    }
}

impl Geometry {
    /// Checks that the geometry can stand, whatever its frame and unit.
    ///
    /// # Errors
    ///
    /// Fails, naming the kind and what it needs, on a geometry that cannot stand.
    pub fn check(&self) -> Result<(), GeometryError> {
        let (ok, kind, need) = match self {
            Self::Point { at } => (finite(at), "point", "finite coordinates"),
            Self::Ray {
                origin,
                direction,
                reach,
            } => (
                finite(origin) && unit_length(direction) && positive(*reach),
                "ray",
                "a finite origin, a unit direction and a positive reach",
            ),
            Self::Range {
                direction,
                near,
                far,
            } => (
                unit_length(direction) && interval(*near, *far),
                "range",
                "a unit direction and 0 ≤ near ≤ far < ∞",
            ),
            Self::Primitive { primitive } => primitive.check(),
            Self::Path { points } => (
                points.len() >= 2 && points.iter().all(finite),
                "path",
                "two or more finite points",
            ),
            Self::Members { members } => (
                !members.is_empty()
                    && members.iter().all(|m| match m {
                        Member::Object { at, .. } | Member::Tracer { at, .. } => finite(at),
                    }),
                "member list",
                "a member, and finite member positions",
            ),
            Self::SkyRegion {
                order,
                cells,
                near,
                far,
            } => (
                *order <= 29
                    && !cells.is_empty()
                    && cells.iter().all(|c| *c < 12 << (2 * u32::from(*order)))
                    && interval(*near, *far),
                "sky region",
                "HEALPix order 0 to 29, cells of that order and 0 ≤ near ≤ far < ∞",
            ),
            Self::VolumeGrid {
                origin,
                cell,
                shape,
                ..
            } => (
                finite(origin) && positive(*cell) && !shape.contains(&0),
                "volume grid",
                "a finite origin, a positive cell and shape",
            ),
            Self::GridRegion {
                origin,
                cell,
                shape,
                cells,
                ..
            } => (
                finite(origin)
                    && positive(*cell)
                    && !shape.contains(&0)
                    && !cells.is_empty()
                    && cells.iter().all(|c| (0..3).all(|k| c[k] < shape[k])),
                "grid region",
                "a finite origin, a positive cell and shape, and one or more cells inside it",
            ),
        };
        if ok {
            Ok(())
        } else {
            Err(GeometryError::Malformed(kind, need))
        }
    }
}

impl Primitive {
    fn check(&self) -> (bool, &'static str, &'static str) {
        match self {
            Self::Sphere { centre, radius } => (
                finite(centre) && positive(*radius),
                "sphere",
                "a finite centre and a positive radius",
            ),
            Self::Ellipsoid {
                centre,
                semi_axes: axes,
                orientation,
            }
            | Self::OrientedBox {
                centre,
                half_extents: axes,
                orientation,
            } => (
                finite(centre) && axes.iter().all(|a| positive(*a)) && orientation.is_orthonormal(),
                "ellipsoid or oriented box",
                "a finite centre, positive axes and an orthonormal orientation",
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_geometries_and_units_are_refused() {
        let point = Geometry::Point { at: [0.0; 3] };
        let refused = Shape::new(Frame::Icrs, Unit::Degree, point.clone());
        assert_eq!(refused, Err(GeometryError::NotALength(Unit::Degree)));
        assert!(Shape::new(Frame::Icrs, Unit::MegaparsecOverH, point).is_err());
        let x = [1.0, 0.0, 0.0];
        let stretched = Rotation([[2.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
        for bad in [
            Geometry::Point {
                at: [f64::NAN, 0.0, 0.0],
            },
            Geometry::Path {
                points: vec![[0.0; 3]],
            },
            Geometry::Members { members: vec![] },
            Geometry::Range {
                direction: x,
                near: 5.0,
                far: 1.0,
            },
            Geometry::Range {
                direction: x,
                near: 0.0,
                far: f64::INFINITY,
            },
            Geometry::Range {
                direction: [2.0, 0.0, 0.0],
                near: 0.0,
                far: 1.0,
            },
            Geometry::Ray {
                origin: [0.0; 3],
                direction: x,
                reach: f64::NAN,
            },
            Geometry::SkyRegion {
                order: 30,
                cells: vec![1],
                near: 0.0,
                far: 1.0,
            },
            Geometry::SkyRegion {
                order: 0,
                cells: vec![12],
                near: 0.0,
                far: 1.0,
            },
            Geometry::Primitive {
                primitive: Primitive::Sphere {
                    centre: [0.0; 3],
                    radius: -5.0,
                },
            },
            Geometry::GridRegion {
                grid: "g".into(),
                origin: [0.0; 3],
                cell: 1.0,
                shape: [2, 2, 2],
                cells: vec![[0, 2, 0]],
            },
            Geometry::GridRegion {
                grid: "g".into(),
                origin: [0.0; 3],
                cell: 1.0,
                shape: [2, 2, 2],
                cells: vec![],
            },
            Geometry::Primitive {
                primitive: Primitive::Ellipsoid {
                    centre: [0.0; 3],
                    semi_axes: [1.0; 3],
                    orientation: stretched,
                },
            },
        ] {
            assert!(
                Shape::new(Frame::Icrs, Unit::Megaparsec, bad.clone()).is_err(),
                "{bad:?}"
            );
        }
    }
}
