//! The region a volume encloses, in the ICRS about the barycentre: its exact distance from any
//! point and its exact bounding box.

use std::f64::consts::PI;

use crate::geometry::Aabb;
use crate::position::{Rotation, Vec3, cross, dot, scaled, zip};

/// A region of ICRS space, in one unit of length.
#[derive(Clone, Debug, PartialEq)]
pub enum Region {
    Sphere {
        centre: Vec3,
        radius: f64,
    },
    /// A box axis-aligned in another frame: a point `p` lies in it when each coordinate of
    /// `to_frame · p − centre` is within `half` of zero.
    Box {
        to_frame: Rotation,
        centre: Vec3,
        half: Vec3,
    },
    /// The points from `near` to `far` from the barycentre within `half_angle` (radians, at
    /// most π/2) of the unit vector `axis`.
    Sector {
        axis: Vec3,
        half_angle: f64,
        near: f64,
        far: f64,
    },
}

fn norm(v: Vec3) -> f64 {
    dot(v, v).sqrt()
}

impl Region {
    /// The region with every length multiplied by `factor`, a positive change of unit.
    #[must_use]
    pub fn scaled(self, factor: f64) -> Self {
        match self {
            Self::Sphere { centre, radius } => Self::Sphere {
                centre: scaled(centre, factor),
                radius: radius * factor,
            },
            Self::Box {
                to_frame,
                centre,
                half,
            } => Self::Box {
                to_frame,
                centre: scaled(centre, factor),
                half: scaled(half, factor),
            },
            Self::Sector {
                axis,
                half_angle,
                near,
                far,
            } => Self::Sector {
                axis,
                half_angle,
                near: near * factor,
                far: far * factor,
            },
        }
    }

    /// The Euclidean distance from `p` to the nearest point of the region: zero within it.
    #[must_use]
    pub fn distance(&self, p: Vec3) -> f64 {
        match self {
            Self::Sphere { centre, radius } => {
                (norm(zip(p, *centre, |a, b| a - b)) - radius).max(0.0)
            }
            Self::Box {
                to_frame,
                centre,
                half,
            } => {
                let local = zip(to_frame.apply(p), *centre, |a, b| a - b);
                norm(std::array::from_fn(|i| (local[i].abs() - half[i]).max(0.0)))
            }
            Self::Sector {
                axis,
                half_angle,
                near,
                far,
            } => {
                // In the half-plane through the axis and `p` the sector is the annular sector
                // of radii near to far and angles 0 to half_angle, and the nearest point of the
                // solid lies in that half-plane.
                let r = norm(p);
                let theta = norm(cross(p, *axis)).atan2(dot(p, *axis));
                if theta <= *half_angle {
                    return (near - r).max(r - far).max(0.0);
                }
                // Along and across the sector's edge, so that a point on it is at no distance
                // rather than at a difference of squares that loses digits.
                let phi = theta - half_angle;
                let (along, across) = (r * phi.cos(), r * phi.sin());
                (along - along.clamp(*near, *far)).hypot(across)
            }
        }
    }

    /// Whether `p` lies in the region.
    #[must_use]
    pub fn contains(&self, p: Vec3) -> bool {
        self.distance(p) <= 0.0
    }

    /// The smallest axis-aligned ICRS box holding the region.
    #[must_use]
    pub fn bounds(&self) -> Aabb {
        match self {
            Self::Sphere { centre, radius } => Aabb {
                min: centre.map(|c| c - radius),
                max: centre.map(|c| c + radius),
            },
            Self::Box {
                to_frame,
                centre,
                half,
            } => {
                let back = to_frame.transpose();
                let corners = (0..8).map(|corner| {
                    back.apply(std::array::from_fn(|i| {
                        let sign = if corner >> i & 1 == 1 { 1.0 } else { -1.0 };
                        centre[i] + sign * half[i]
                    }))
                });
                bounding(corners)
            }
            Self::Sector {
                axis,
                half_angle,
                near,
                far,
            } => {
                let reach = |e: Vec3| {
                    let m = cap_extreme(*axis, *half_angle, e);
                    if m >= 0.0 { far * m } else { near * m }
                };
                let unit = |i: usize, s: f64| std::array::from_fn(|j| if i == j { s } else { 0.0 });
                Aabb {
                    min: std::array::from_fn(|i| -reach(unit(i, -1.0))),
                    max: std::array::from_fn(|i| reach(unit(i, 1.0))),
                }
            }
        }
    }

    /// A length of the region's own scale: its diameter, or a cone's far distance.
    #[must_use]
    pub fn extent(&self) -> f64 {
        match self {
            Self::Sphere { radius, .. } => 2.0 * radius,
            Self::Box { half, .. } => 2.0 * norm(*half),
            Self::Sector { far, .. } => *far,
        }
    }
}

/// The largest `v · e` over the unit vectors `v` within `half_angle` of `axis`.
fn cap_extreme(axis: Vec3, half_angle: f64, e: Vec3) -> f64 {
    let gamma = dot(axis, e).clamp(-1.0, 1.0).acos();
    if gamma <= half_angle {
        1.0
    } else {
        (gamma - half_angle).min(PI).cos()
    }
}

/// The box around `points`, of which there is at least one.
pub(super) fn bounding(points: impl IntoIterator<Item = Vec3>) -> Aabb {
    let mut points = points.into_iter();
    let first = points.next().unwrap_or([0.0; 3]);
    points.fold(
        Aabb {
            min: first,
            max: first,
        },
        |b, p| Aabb {
            min: zip(b.min, p, f64::min),
            max: zip(b.max, p, f64::max),
        },
    )
}
