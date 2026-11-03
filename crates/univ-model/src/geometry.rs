//! The closed set of geometry kinds, each with an axis-aligned bounding box for the cell index.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::ids::ObjectId;
use crate::position::{Frame, Rotation, Vec3, between};
use crate::units::{Dimension, Unit};

/// A geometry in a named frame, its Cartesian coordinates in one length unit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Shape {
    pub frame: Frame,
    pub unit: Unit,
    pub geometry: Geometry,
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
    /// A direction whose distance is known only to lie between `near` and `far`.
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

/// A member of a structure: an object, or a tracer with its own position and kind.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "member")]
pub enum Member {
    Object { id: ObjectId },
    Tracer { at: Vec3, kind: String },
}

/// An axis-aligned box.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

/// A geometry that cannot stand.
#[derive(Debug, Error, PartialEq)]
pub enum GeometryError {
    #[error("{0:?} is not a unit of length")]
    NotALength(Unit),
    #[error("a {0} needs {1}")]
    Malformed(&'static str, &'static str),
}

impl Aabb {
    fn around(points: impl IntoIterator<Item = Vec3>) -> Option<Self> {
        points.into_iter().fold(None, |acc, p| {
            Some(match acc {
                None => Self { min: p, max: p },
                Some(b) => Self {
                    min: p.zip_with(b.min, f64::min),
                    max: p.zip_with(b.max, f64::max),
                },
            })
        })
    }

    fn centred(centre: Vec3, half: Vec3) -> Self {
        Self {
            min: [0, 1, 2].map(|i| centre[i] - half[i]),
            max: [0, 1, 2].map(|i| centre[i] + half[i]),
        }
    }
}

/// `zip_with` for the three components of a vector.
trait ZipWith {
    fn zip_with(self, other: Vec3, f: fn(f64, f64) -> f64) -> Vec3;
}

impl ZipWith for Vec3 {
    fn zip_with(self, other: Vec3, f: fn(f64, f64) -> f64) -> Vec3 {
        [
            f(self[0], other[0]),
            f(self[1], other[1]),
            f(self[2], other[2]),
        ]
    }
}

/// A finite number above zero.
fn positive(x: f64) -> bool {
    x.is_finite() && x > 0.0
}

fn scaled(v: Vec3, s: f64) -> Vec3 {
    v.map(|c| c * s)
}

fn sum(a: Vec3, b: Vec3) -> Vec3 {
    a.zip_with(b, |x, y| x + y)
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
        geometry.check()?;
        Ok(Self {
            frame,
            unit,
            geometry,
        })
    }

    /// The bounding box in `frame`, in this shape's unit. `position` gives the position of a
    /// member object in this shape's frame. Only a member list whose members have no known
    /// position has no box.
    #[must_use]
    pub fn bounds_in(
        &self,
        frame: Frame,
        position: impl Fn(ObjectId) -> Option<Vec3>,
    ) -> Option<Aabb> {
        self.geometry.bounds(&between(self.frame, frame), position)
    }
}

impl Geometry {
    fn check(&self) -> Result<(), GeometryError> {
        let fail = |kind, need| Err(GeometryError::Malformed(kind, need));
        match self {
            Self::Ray { reach, .. } if !positive(*reach) => fail("ray", "a positive reach"),
            Self::Range { near, far, .. } | Self::SkyRegion { near, far, .. }
                if !(*near >= 0.0 && near <= far) =>
            {
                fail("range or sky region", "0 ≤ near ≤ far")
            }
            Self::Path { points } if points.len() < 2 => fail("path", "two or more points"),
            Self::Members { members } if members.is_empty() => fail("member list", "a member"),
            Self::SkyRegion { order, cells, .. } if *order > 29 || cells.is_empty() => {
                fail("sky region", "HEALPix order 0 to 29 and a cell")
            }
            Self::VolumeGrid { cell, shape, .. } if !positive(*cell) || shape.contains(&0) => {
                fail("volume grid", "a positive cell and shape")
            }
            _ => Ok(()),
        }
    }

    /// The box in the frame `rotation` takes this geometry's frame to.
    fn bounds(
        &self,
        rotation: &Rotation,
        position: impl Fn(ObjectId) -> Option<Vec3>,
    ) -> Option<Aabb> {
        let r = |v: Vec3| rotation.apply(v);
        match self {
            Self::Point { at } => Aabb::around([r(*at)]),
            Self::Ray {
                origin,
                direction,
                reach,
            } => Aabb::around([r(*origin), r(sum(*origin, scaled(*direction, *reach)))]),
            Self::Range {
                direction,
                near,
                far,
            } => Aabb::around([r(scaled(*direction, *near)), r(scaled(*direction, *far))]),
            Self::Primitive { primitive } => Some(primitive.bounds(rotation)),
            Self::Path { points } => Aabb::around(points.iter().map(|p| r(*p))),
            Self::Members { members } => Aabb::around(members.iter().filter_map(|m| match m {
                Member::Object { id } => position(*id).map(r),
                Member::Tracer { at, .. } => Some(r(*at)),
            })),
            // Every direction within `far` of the observer: a box around the sphere of that radius.
            Self::SkyRegion { far, .. } => Some(Aabb::centred([0.0; 3], [*far; 3])),
            Self::VolumeGrid {
                origin,
                cell,
                shape,
                ..
            } => {
                let size = shape.map(|n| f64::from(n) * cell);
                Aabb::around((0..8).map(|corner| {
                    r([0, 1, 2]
                        .map(|i| origin[i] + if corner >> i & 1 == 1 { size[i] } else { 0.0 }))
                }))
            }
        }
    }
}

impl Primitive {
    fn bounds(&self, rotation: &Rotation) -> Aabb {
        match self {
            Self::Sphere { centre, radius } => Aabb::centred(rotation.apply(*centre), [*radius; 3]),
            Self::Ellipsoid {
                centre,
                semi_axes,
                orientation,
            } => {
                let m = rotation.then_after(orientation).0;
                let half = m.map(|row| {
                    (0..3)
                        .map(|j| (row[j] * semi_axes[j]).powi(2))
                        .sum::<f64>()
                        .sqrt()
                });
                Aabb::centred(rotation.apply(*centre), half)
            }
            Self::OrientedBox {
                centre,
                half_extents,
                orientation,
            } => {
                let m = rotation.then_after(orientation).0;
                let half = m.map(|row| {
                    (0..3)
                        .map(|j| (row[j] * half_extents[j]).abs())
                        .sum::<f64>()
                });
                Aabb::centred(rotation.apply(*centre), half)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none(_: ObjectId) -> Option<Vec3> {
        None
    }

    fn shape(geometry: Geometry) -> Shape {
        Shape::new(Frame::Supergalactic, Unit::Megaparsec, geometry).unwrap()
    }

    fn near(a: Aabb, b: Aabb) -> bool {
        (0..3).all(|i| (a.min[i] - b.min[i]).abs() < 1e-9 && (a.max[i] - b.max[i]).abs() < 1e-9)
    }

    /// A rotation by 45° about z.
    fn turn() -> Rotation {
        let c = std::f64::consts::FRAC_1_SQRT_2;
        Rotation([[c, -c, 0.0], [c, c, 0.0], [0.0, 0.0, 1.0]])
    }

    #[test]
    fn a_turned_ellipsoid_and_box_are_bounded_tightly() {
        let ellipsoid = Primitive::Ellipsoid {
            centre: [1.0, 2.0, 3.0],
            semi_axes: [2.0, 1.0, 0.5],
            orientation: turn(),
        };
        let half = (2.5f64).sqrt(); // sqrt((2² + 1²) / 2)
        let expected = Aabb::centred([1.0, 2.0, 3.0], [half, half, 0.5]);
        assert!(near(
            shape(Geometry::Primitive {
                primitive: ellipsoid
            })
            .bounds_in(Frame::Supergalactic, none)
            .unwrap(),
            expected
        ));
        let cuboid = Primitive::OrientedBox {
            centre: [0.0; 3],
            half_extents: [2.0, 1.0, 0.5],
            orientation: turn(),
        };
        let h = 3.0 * std::f64::consts::FRAC_1_SQRT_2;
        let expected = Aabb::centred([0.0; 3], [h, h, 0.5]);
        assert!(near(
            shape(Geometry::Primitive { primitive: cuboid })
                .bounds_in(Frame::Supergalactic, none)
                .unwrap(),
            expected
        ));
    }

    #[test]
    fn every_kind_has_a_box() {
        let members = Geometry::Members {
            members: vec![
                Member::Object {
                    id: ObjectId::new(7).unwrap(),
                },
                Member::Tracer {
                    at: [4.0, -1.0, 0.0],
                    kind: "quasar".into(),
                },
            ],
        };
        let at_seven = |id: ObjectId| (id.get() == 7).then_some([-2.0, 3.0, 1.0]);
        let b = shape(members)
            .bounds_in(Frame::Supergalactic, at_seven)
            .unwrap();
        assert_eq!(
            b,
            Aabb {
                min: [-2.0, -1.0, 0.0],
                max: [4.0, 3.0, 1.0]
            }
        );
        let ray = Geometry::Ray {
            origin: [0.0; 3],
            direction: [0.0, 1.0, 0.0],
            reach: 100.0,
        };
        assert_eq!(
            shape(ray)
                .bounds_in(Frame::Supergalactic, none)
                .unwrap()
                .max,
            [0.0, 100.0, 0.0]
        );
        let grid = Geometry::VolumeGrid {
            grid: "cf4-density".into(),
            origin: [-10.0; 3],
            cell: 2.0,
            shape: [10, 5, 1],
        };
        let b = shape(grid).bounds_in(Frame::Supergalactic, none).unwrap();
        assert_eq!(
            b,
            Aabb {
                min: [-10.0; 3],
                max: [10.0, 0.0, -8.0]
            }
        );
        let region = Geometry::SkyRegion {
            order: 3,
            cells: vec![12],
            near: 0.0,
            far: 50.0,
        };
        assert_eq!(
            shape(region)
                .bounds_in(Frame::Supergalactic, none)
                .unwrap()
                .min,
            [-50.0; 3]
        );
    }

    #[test]
    fn a_point_is_bounded_in_another_frame() {
        let point = Shape::new(
            Frame::Galactic,
            Unit::Parsec,
            Geometry::Point {
                at: [8_000.0, 0.0, 0.0],
            },
        )
        .unwrap();
        let b = point.bounds_in(Frame::Icrs, none).unwrap();
        let length = b.min.iter().map(|c| c * c).sum::<f64>().sqrt();
        assert!(
            (length - 8_000.0).abs() < 1e-6
                && near(
                    b,
                    Aabb {
                        min: b.max,
                        max: b.max
                    }
                )
        );
    }

    #[test]
    fn malformed_geometries_and_units_are_refused() {
        let point = Geometry::Point { at: [0.0; 3] };
        assert_eq!(
            Shape::new(Frame::Icrs, Unit::Degree, point.clone()),
            Err(GeometryError::NotALength(Unit::Degree))
        );
        assert!(Shape::new(Frame::Icrs, Unit::MegaparsecOverH, point).is_err());
        for bad in [
            Geometry::Path {
                points: vec![[0.0; 3]],
            },
            Geometry::Members { members: vec![] },
            Geometry::Range {
                direction: [1.0, 0.0, 0.0],
                near: 5.0,
                far: 1.0,
            },
            Geometry::Ray {
                origin: [0.0; 3],
                direction: [1.0, 0.0, 0.0],
                reach: f64::NAN,
            },
            Geometry::SkyRegion {
                order: 30,
                cells: vec![1],
                near: 0.0,
                far: 1.0,
            },
        ] {
            assert!(
                Shape::new(Frame::Icrs, Unit::Megaparsec, bad.clone()).is_err(),
                "{bad:?}"
            );
        }
    }
}
