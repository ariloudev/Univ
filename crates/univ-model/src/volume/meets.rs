//! Whether a geometry meets a region: exactly where a closed form exists, and otherwise by
//! best-first refinement.
//!
//! - **Exact.** A point or member list, point by point; a sphere, by the region's distance from
//!   its centre; a box (an oriented box, a volume grid or a grid cell) against a spherical
//!   region, by the sphere centre's distance from the box, and against a box region by
//!   separating axes.
//! - **Refined.** Everything else is cut into pieces, each bounded by a sphere: a segment, a
//!   part of a solid's parameter cube, or a `HEALPix` cell of a sky region between two distances.
//!   A piece whose bounding sphere the region's distance clears is dropped; a piece holding a
//!   point of the region (its witness) decides a meeting; the piece nearest the region is split
//!   next. A piece smaller than [`TOLERANCE`] of the region's extent that is still undecided,
//!   or a geometry still undecided after [`REFINEMENT_BUDGET`] splits, counts as meeting: a
//!   geometry within that tolerance of the region may match it.

use super::Region;
use super::refine::Piece;
use crate::geometry::{Aabb, Geometry, Member, Primitive, Shape};
use crate::position::{Frame, Rotation, Vec3, add, between, cross, dot, norm, scaled, sub, zip};
use crate::units::Unit;

/// A solid as the image of the cube [-1, 1]³: `centre + Σ uᵢ axes[i]`, its axes orthogonal,
/// or of the unit ball within it for an ellipsoid.
#[derive(Clone, Copy, Debug)]
pub(super) struct Solid {
    pub(super) centre: Vec3,
    pub(super) axes: [Vec3; 3],
    pub(super) ball: bool,
}

impl Solid {
    pub(super) fn point(&self, u: Vec3) -> Vec3 {
        (0..3).fold(self.centre, |p, i| add(p, scaled(self.axes[i], u[i])))
    }

    /// The axis-aligned box `bounds`.
    fn cube(bounds: &Aabb) -> Self {
        let half = zip(bounds.max, bounds.min, |a, b| (a - b) / 2.0);
        Self {
            centre: bounds.centre(),
            axes: std::array::from_fn(|i| {
                std::array::from_fn(|j| if i == j { half[i] } else { 0.0 })
            }),
            ball: false,
        }
    }
}

/// How a shape's frame and unit are brought to the region's: rotated into the ICRS, scaled.
pub(super) struct Placing {
    pub(super) rotation: Rotation,
    pub(super) factor: f64,
}

impl Placing {
    pub(super) fn at(&self, p: Vec3) -> Vec3 {
        scaled(self.rotation.apply(p), self.factor)
    }
}

impl Region {
    /// Whether `shape` meets the region, whose lengths are in `unit`. None for a shape that
    /// cannot be brought into the ICRS (FK4 B1950 is not a rotation of it) or whose unit is
    /// not a length.
    #[must_use]
    pub fn meets(&self, unit: Unit, shape: &Shape) -> Option<bool> {
        let placing = Placing {
            rotation: between(shape.frame, Frame::Icrs).rotation()?,
            factor: shape.unit.factor_to(unit).ok()?,
        };
        Some(self.meets_geometry(&placing, &shape.geometry))
    }

    /// Whether the axis-aligned ICRS box `bounds`, in the region's unit, meets the region.
    #[must_use]
    pub fn meets_box(&self, bounds: &Aabb) -> bool {
        self.meets_solids(
            &[Solid::cube(bounds)],
            &Placing {
                rotation: Rotation::IDENTITY,
                factor: 1.0,
            },
        )
    }

    fn meets_geometry(&self, placing: &Placing, geometry: &Geometry) -> bool {
        let p = |v: Vec3| placing.at(v);
        match geometry {
            Geometry::Point { at } => self.contains(p(*at)),
            Geometry::Members { members } => members.iter().any(|m| match m {
                Member::Object { at, .. } | Member::Tracer { at, .. } => self.contains(p(*at)),
            }),
            Geometry::Ray {
                origin,
                direction,
                reach,
            } => self.refine(
                placing,
                vec![Piece::Segment {
                    a: p(*origin),
                    b: p(add(*origin, scaled(*direction, *reach))),
                }],
            ),
            Geometry::Range {
                direction,
                near,
                far,
            } => self.refine(
                placing,
                vec![Piece::Segment {
                    a: p(scaled(*direction, *near)),
                    b: p(scaled(*direction, *far)),
                }],
            ),
            Geometry::Path { points } => match points.as_slice() {
                [only] => self.contains(p(*only)),
                _ => self.refine(
                    placing,
                    points
                        .windows(2)
                        .map(|w| Piece::Segment {
                            a: p(w[0]),
                            b: p(w[1]),
                        })
                        .collect(),
                ),
            },
            Geometry::Primitive { primitive } => self.meets_primitive(placing, primitive),
            Geometry::VolumeGrid {
                origin,
                cell,
                shape,
                ..
            } => {
                let size = shape.map(|n| f64::from(n) * cell);
                let cells = [[0_u32; 3]];
                self.meets_solids(&grid_cells(placing, *origin, size, &cells), placing)
            }
            Geometry::GridRegion {
                origin,
                cell,
                cells,
                ..
            } => self.meets_solids(&grid_cells(placing, *origin, [*cell; 3], cells), placing),
            Geometry::SkyRegion {
                order,
                cells,
                near,
                far,
            } => self.refine(
                placing,
                cells
                    .iter()
                    .map(|&hash| Piece::Sky {
                        order: *order,
                        hash,
                        near: *near,
                        far: *far,
                    })
                    .collect(),
            ),
        }
    }

    fn meets_primitive(&self, placing: &Placing, primitive: &Primitive) -> bool {
        let p = |v: Vec3| placing.at(v);
        match primitive {
            Primitive::Sphere { centre, radius } => {
                self.distance(p(*centre)) <= radius * placing.factor
            }
            Primitive::Ellipsoid {
                centre,
                semi_axes,
                orientation,
            } => self.meets_solids(
                &[oriented(placing, *centre, *semi_axes, orientation, true)],
                placing,
            ),
            Primitive::OrientedBox {
                centre,
                half_extents,
                orientation,
            } => self.meets_solids(
                &[oriented(
                    placing,
                    *centre,
                    *half_extents,
                    orientation,
                    false,
                )],
                placing,
            ),
        }
    }

    /// Whether any of `solids` meets the region: exactly for boxes against a sphere or a box.
    fn meets_solids(&self, solids: &[Solid], placing: &Placing) -> bool {
        let exact = |solid: &Solid| match self {
            Self::Sphere { centre, radius } if !solid.ball => {
                Some(box_distance(solid, *centre) <= *radius)
            }
            Self::Box {
                to_frame,
                centre,
                half,
            } if !solid.ball => Some(boxes_meet(to_frame, *centre, *half, solid)),
            _ => None,
        };
        let mut refined = Vec::new();
        for solid in solids {
            match exact(solid) {
                Some(true) => return true,
                Some(false) => {}
                None => refined.push(Piece::Cube {
                    solid: *solid,
                    lo: [-1.0; 3],
                    hi: [1.0; 3],
                }),
            }
        }
        !refined.is_empty() && self.refine(placing, refined)
    }
}

/// A primitive solid of the shape's frame, placed.
fn oriented(
    placing: &Placing,
    centre: Vec3,
    half: Vec3,
    orientation: &Rotation,
    ball: bool,
) -> Solid {
    Solid {
        centre: placing.at(centre),
        axes: std::array::from_fn(|i| {
            placing.at(std::array::from_fn(|row| orientation.0[row][i] * half[i]))
        }),
        ball,
    }
}

/// The cells `cells` of a grid from `origin` with cells of `size`, axis-aligned in its frame,
/// placed.
fn grid_cells(placing: &Placing, origin: Vec3, size: Vec3, cells: &[[u32; 3]]) -> Vec<Solid> {
    cells
        .iter()
        .map(|c| Solid {
            centre: placing.at(std::array::from_fn(|i| {
                origin[i] + (f64::from(c[i]) + 0.5) * size[i]
            })),
            axes: std::array::from_fn(|i| {
                placing.at(std::array::from_fn(|j| {
                    if i == j { size[i] / 2.0 } else { 0.0 }
                }))
            }),
            ball: false,
        })
        .collect()
}

/// The distance from `p` to a box solid.
fn box_distance(solid: &Solid, p: Vec3) -> f64 {
    let d = sub(p, solid.centre);
    norm(std::array::from_fn(|i| {
        let length = norm(solid.axes[i]);
        if length == 0.0 {
            0.0
        } else {
            ((dot(d, solid.axes[i]) / length).abs() - length).max(0.0)
        }
    }))
}

/// Whether the region box (axes the rows of `to_frame`) and a box solid overlap, by the
/// separating axis theorem: they are apart exactly when some face normal of either, or some
/// cross product of an edge of each, separates their projections.
fn boxes_meet(to_frame: &Rotation, centre: Vec3, half: Vec3, solid: &Solid) -> bool {
    let a_axes = to_frame.0;
    let a_centre = to_frame.transpose().apply(centre);
    let b_axes: Vec<(Vec3, f64)> = solid
        .axes
        .iter()
        .map(|a| {
            let length = norm(*a);
            (
                if length > 0.0 {
                    scaled(*a, 1.0 / length)
                } else {
                    [0.0; 3]
                },
                length,
            )
        })
        .collect();
    let between_centres = sub(solid.centre, a_centre);
    let separates = |axis: Vec3| {
        let length = norm(axis);
        if length < 1e-12 {
            return false;
        }
        let axis = scaled(axis, 1.0 / length);
        let ra: f64 = (0..3).map(|i| half[i] * dot(a_axes[i], axis).abs()).sum();
        let rb: f64 = b_axes.iter().map(|(u, h)| h * dot(*u, axis).abs()).sum();
        dot(between_centres, axis).abs() > ra + rb
    };
    let mut axes: Vec<Vec3> = a_axes.to_vec();
    axes.extend(b_axes.iter().map(|(u, _)| *u));
    for a in a_axes {
        for (b, _) in &b_axes {
            axes.push(cross(a, *b));
        }
    }
    !axes.into_iter().any(separates)
}
