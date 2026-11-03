//! Axis-aligned bounding boxes of every geometry kind, for the cell index.

use serde::{Deserialize, Serialize};

use super::{Geometry, Member, Primitive};
use crate::position::{Rotation, Vec3, dot, scaled, zip};

/// An axis-aligned box.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Aabb {
    #[cfg_attr(feature = "openapi", schema(value_type = Vec<f64>, min_items = 3, max_items = 3))]
    pub min: Vec3,
    #[cfg_attr(feature = "openapi", schema(value_type = Vec<f64>, min_items = 3, max_items = 3))]
    pub max: Vec3,
}

impl Aabb {
    fn around(points: impl IntoIterator<Item = Vec3>) -> Option<Self> {
        points.into_iter().fold(None, |acc, p| {
            Some(match acc {
                None => Self { min: p, max: p },
                Some(b) => Self {
                    min: zip(p, b.min, f64::min),
                    max: zip(p, b.max, f64::max),
                },
            })
        })
    }

    fn centred(centre: Vec3, half: Vec3) -> Self {
        Self {
            min: zip(centre, half, |c, h| c - h),
            max: zip(centre, half, |c, h| c + h),
        }
    }
}

impl Geometry {
    /// The box in the frame `rotation` takes this geometry's frame to.
    pub(super) fn bounds(&self, rotation: &Rotation) -> Option<Aabb> {
        let r = |v: Vec3| rotation.apply(v);
        match self {
            Self::Point { at } => Aabb::around([r(*at)]),
            Self::Ray {
                origin,
                direction,
                reach,
            } => Aabb::around([
                r(*origin),
                r(zip(*origin, scaled(*direction, *reach), |o, d| o + d)),
            ]),
            Self::Range {
                direction,
                near,
                far,
            } => Aabb::around([r(scaled(*direction, *near)), r(scaled(*direction, *far))]),
            Self::Primitive { primitive } => Some(primitive.bounds(rotation)),
            Self::Path { points } => Aabb::around(points.iter().map(|p| r(*p))),
            Self::Members { members } => Aabb::around(members.iter().map(|m| match m {
                Member::Object { at, .. } | Member::Tracer { at, .. } => r(*at),
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
                    r(std::array::from_fn(|i| {
                        origin[i] + if corner >> i & 1 == 1 { size[i] } else { 0.0 }
                    }))
                }))
            }
            Self::GridRegion {
                origin,
                cell,
                cells,
                ..
            } => Aabb::around(cells.iter().flat_map(|c| {
                (0..8).map(move |corner| {
                    r(std::array::from_fn(|i| {
                        let near = f64::from(c[i] + u32::from(corner >> i & 1 == 1));
                        origin[i] + near * cell
                    }))
                })
            })),
        }
    }
}

impl Primitive {
    fn bounds(&self, rotation: &Rotation) -> Aabb {
        match self {
            Self::Sphere { centre, radius } => Aabb::centred(rotation.apply(*centre), [*radius; 3]),
            Self::Ellipsoid {
                centre,
                semi_axes: axes,
                orientation,
            }
            | Self::OrientedBox {
                centre,
                half_extents: axes,
                orientation,
            } => {
                // Each axis of the solid, turned into the target frame, reaches |row · axis| along
                // a box axis: the box needs the sum of those reaches, the ellipsoid their norm.
                let ellipsoid = matches!(self, Self::Ellipsoid { .. });
                let half = rotation.then_after(orientation).0.map(|row| {
                    let reach = zip(row, *axes, |r, a| (r * a).abs());
                    if ellipsoid {
                        dot(reach, reach).sqrt()
                    } else {
                        reach.iter().sum()
                    }
                });
                Aabb::centred(rotation.apply(*centre), half)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Shape;
    use crate::ids::ObjectId;
    use crate::position::Frame;
    use crate::units::Unit;

    fn shape(geometry: Geometry) -> Shape {
        Shape::new(Frame::Supergalactic, Unit::Megaparsec, geometry).unwrap()
    }

    /// The box of `geometry` in its own frame.
    fn boxed(geometry: Geometry) -> Aabb {
        shape(geometry).bounds_in(Frame::Supergalactic).unwrap()
    }

    fn near(a: Aabb, b: Aabb) -> bool {
        (0..3).all(|i| (a.min[i] - b.min[i]).abs() < 1e-9 && (a.max[i] - b.max[i]).abs() < 1e-9)
    }

    /// A rotation by 45° about z.
    fn turn() -> Rotation {
        let c = std::f64::consts::FRAC_1_SQRT_2;
        Rotation([[c, -c, 0.0], [c, c, 0.0], [0.0, 0.0, 1.0]])
    }

    fn members() -> Geometry {
        Geometry::Members {
            members: vec![
                Member::Object {
                    id: ObjectId::new(7).unwrap(),
                    at: [-2.0, 3.0, 1.0],
                },
                Member::Tracer {
                    at: [4.0, -1.0, 0.0],
                    kind: "quasar".into(),
                },
            ],
        }
    }

    #[test]
    fn a_turned_ellipsoid_and_box_are_bounded_tightly() {
        let (centre, axes, orientation) = ([1.0, 2.0, 3.0], [2.0, 1.0, 0.5], turn());
        let ellipsoid = Primitive::Ellipsoid {
            centre,
            semi_axes: axes,
            orientation,
        };
        let half = 2.5f64.sqrt(); // sqrt((2² + 1²) / 2)
        let expected = Aabb::centred(centre, [half, half, 0.5]);
        assert!(near(
            boxed(Geometry::Primitive {
                primitive: ellipsoid
            }),
            expected
        ));
        let cuboid = Primitive::OrientedBox {
            centre,
            half_extents: axes,
            orientation,
        };
        let half = 3.0 * std::f64::consts::FRAC_1_SQRT_2;
        let expected = Aabb::centred(centre, [half, half, 0.5]);
        assert!(near(
            boxed(Geometry::Primitive { primitive: cuboid }),
            expected
        ));
    }

    #[test]
    fn every_kind_has_a_box() {
        let b = boxed(members());
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
        assert_eq!(boxed(ray).max, [0.0, 100.0, 0.0]);
        let grid = Geometry::VolumeGrid {
            grid: "cf4-density".into(),
            origin: [-10.0; 3],
            cell: 2.0,
            shape: [10, 5, 1],
        };
        assert_eq!(
            boxed(grid),
            Aabb {
                min: [-10.0; 3],
                max: [10.0, 0.0, -8.0]
            }
        );
        let outlined = Geometry::GridRegion {
            grid: "cf4pp-density".into(),
            origin: [-10.0; 3],
            cell: 2.0,
            shape: [10, 10, 10],
            cells: vec![[1, 2, 3], [2, 2, 3]],
        };
        assert_eq!(
            boxed(outlined),
            Aabb {
                min: [-8.0, -6.0, -4.0],
                max: [-4.0, -4.0, -2.0]
            }
        );
        let region = Geometry::SkyRegion {
            order: 3,
            cells: vec![12],
            near: 0.0,
            far: 50.0,
        };
        assert_eq!(boxed(region).min, [-50.0; 3]);
    }

    #[test]
    fn a_member_list_with_a_member_it_cannot_place_has_no_box() {
        // A member's position is resolved at ingest; one that could not be is no shape at all.
        let unplaced = Geometry::Members {
            members: vec![Member::Object {
                id: ObjectId::new(7).unwrap(),
                at: [f64::NAN, 0.0, 0.0],
            }],
        };
        assert!(Shape::new(Frame::Supergalactic, Unit::Megaparsec, unplaced).is_err());
    }

    #[test]
    fn a_point_is_bounded_in_another_frame() {
        let at = [8_000.0, 0.0, 0.0];
        let point = Shape::new(Frame::Galactic, Unit::Parsec, Geometry::Point { at }).unwrap();
        let b = point.bounds_in(Frame::Icrs).unwrap();
        assert!((dot(b.min, b.min).sqrt() - 8_000.0).abs() < 1e-6);
        assert!(near(b, Aabb::around([b.max]).unwrap()));
    }
}
