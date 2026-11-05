//! A geometry in another unit of length: every length scaled by one factor, every direction,
//! orientation, cell index and grid shape kept.

use super::{Geometry, Member, Primitive};
use crate::position::Vec3;

impl Geometry {
    /// This geometry with every length multiplied by `factor`, as when its unit changes: its
    /// positions, radii, extents, reaches, distances and cell edges. Directions, orientations,
    /// `HEALPix` cells and grid indices are unchanged.
    #[must_use]
    pub fn scaled(self, factor: f64) -> Self {
        let s = |v: Vec3| v.map(|c| c * factor);
        match self {
            Self::Point { at } => Self::Point { at: s(at) },
            Self::Ray {
                origin,
                direction,
                reach,
            } => Self::Ray {
                origin: s(origin),
                direction,
                reach: reach * factor,
            },
            Self::Range {
                direction,
                near,
                far,
            } => Self::Range {
                direction,
                near: near * factor,
                far: far * factor,
            },
            Self::Primitive { primitive } => Self::Primitive {
                primitive: primitive.scaled(factor),
            },
            Self::Path { points } => Self::Path {
                points: points.into_iter().map(s).collect(),
            },
            Self::Members { members } => Self::Members {
                members: members
                    .into_iter()
                    .map(|m| match m {
                        Member::Object { id, at } => Member::Object { id, at: s(at) },
                        Member::Tracer { at, kind } => Member::Tracer { at: s(at), kind },
                    })
                    .collect(),
            },
            Self::SkyRegion {
                order,
                cells,
                near,
                far,
            } => Self::SkyRegion {
                order,
                cells,
                near: near * factor,
                far: far * factor,
            },
            Self::VolumeGrid {
                grid,
                origin,
                cell,
                shape,
            } => Self::VolumeGrid {
                grid,
                origin: s(origin),
                cell: cell * factor,
                shape,
            },
            Self::GridRegion {
                grid,
                origin,
                cell,
                shape,
                cells,
            } => Self::GridRegion {
                grid,
                origin: s(origin),
                cell: cell * factor,
                shape,
                cells,
            },
        }
    }
}

impl Primitive {
    /// This solid with its centre and size multiplied by `factor`, its orientation kept.
    #[must_use]
    pub fn scaled(self, factor: f64) -> Self {
        let s = |v: Vec3| v.map(|c| c * factor);
        match self {
            Self::Sphere { centre, radius } => Self::Sphere {
                centre: s(centre),
                radius: radius * factor,
            },
            Self::Ellipsoid {
                centre,
                semi_axes,
                orientation,
            } => Self::Ellipsoid {
                centre: s(centre),
                semi_axes: s(semi_axes),
                orientation,
            },
            Self::OrientedBox {
                centre,
                half_extents,
                orientation,
            } => Self::OrientedBox {
                centre: s(centre),
                half_extents: s(half_extents),
                orientation,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::position::Rotation;

    #[test]
    fn every_length_is_scaled_and_every_direction_kept() {
        let ray = Geometry::Ray {
            origin: [1.0, 2.0, 3.0],
            direction: [0.0, 0.0, 1.0],
            reach: 4.0,
        };
        assert_eq!(
            ray.scaled(1000.0),
            Geometry::Ray {
                origin: [1000.0, 2000.0, 3000.0],
                direction: [0.0, 0.0, 1.0],
                reach: 4000.0,
            }
        );
        let ellipsoid = Geometry::Primitive {
            primitive: Primitive::Ellipsoid {
                centre: [1.0, 0.0, 0.0],
                semi_axes: [3.0, 2.0, 1.0],
                orientation: Rotation::IDENTITY,
            },
        };
        assert_eq!(
            ellipsoid.scaled(2.0),
            Geometry::Primitive {
                primitive: Primitive::Ellipsoid {
                    centre: [2.0, 0.0, 0.0],
                    semi_axes: [6.0, 4.0, 2.0],
                    orientation: Rotation::IDENTITY,
                },
            }
        );
    }
}
