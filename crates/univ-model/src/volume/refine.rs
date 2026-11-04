//! Best-first refinement of a geometry against a region (see [`super::meets`]): pieces each
//! bounded by a sphere, the nearest split first. A piece smaller than [`TOLERANCE`] of the
//! region's extent that is still undecided, or a geometry still undecided after
//! [`REFINEMENT_BUDGET`] splits, counts as meeting.

use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::f64::consts::PI;

use super::Region;
use super::meets::{Placing, Solid};
use crate::healpix;
use crate::position::{Vec3, norm, scaled, sub, zip};

/// The fraction of a region's extent below which an undecided piece counts as meeting.
pub const TOLERANCE: f64 = 1e-6;
/// The most splits refinement makes for one geometry before counting it as meeting.
pub const REFINEMENT_BUDGET: usize = 20_000;

/// A piece of a geometry under refinement.
#[derive(Clone, Copy, Debug)]
pub(super) enum Piece {
    Segment {
        a: Vec3,
        b: Vec3,
    },
    Cube {
        solid: Solid,
        lo: Vec3,
        hi: Vec3,
    },
    Sky {
        order: u8,
        hash: u64,
        near: f64,
        far: f64,
    },
}

/// A piece waiting to be split, nearest the region first.
struct Waiting {
    bound: f64,
    radius: f64,
    piece: Piece,
}

impl PartialEq for Waiting {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for Waiting {}
impl PartialOrd for Waiting {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Waiting {
    fn cmp(&self, other: &Self) -> Ordering {
        other.bound.total_cmp(&self.bound)
    }
}

impl Region {
    /// Best-first refinement of `pieces`, as the module describes it.
    pub(super) fn refine(&self, placing: &Placing, pieces: Vec<Piece>) -> bool {
        let smallest = TOLERANCE * self.extent();
        let mut waiting = BinaryHeap::new();
        for piece in pieces {
            if self.assess(placing, piece, &mut waiting) {
                return true;
            }
        }
        let mut splits = 0;
        while let Some(Waiting { radius, piece, .. }) = waiting.pop() {
            if radius <= smallest || splits >= REFINEMENT_BUDGET {
                return true;
            }
            splits += 1;
            for child in split(placing, &piece) {
                if self.assess(placing, child, &mut waiting) {
                    return true;
                }
            }
        }
        false
    }

    /// Whether `piece` holds a point of the region; otherwise queues it unless the region's
    /// distance clears its bounding sphere.
    fn assess(&self, placing: &Placing, piece: Piece, waiting: &mut BinaryHeap<Waiting>) -> bool {
        if let Piece::Cube { solid, lo, hi } = &piece
            && solid.ball
            && norm(std::array::from_fn(|i| 0.0_f64.clamp(lo[i], hi[i]))) > 1.0
        {
            return false;
        }
        let (centre, radius, witness) = bounding_sphere(placing, &piece);
        if witness && self.contains(centre) {
            return true;
        }
        let distance = self.distance(centre);
        if distance <= radius {
            waiting.push(Waiting {
                bound: distance - radius,
                radius,
                piece,
            });
        }
        false
    }
}

/// A piece's bounding sphere, and whether its centre lies in the geometry, so that the
/// region containing it decides a meeting.
fn bounding_sphere(placing: &Placing, piece: &Piece) -> (Vec3, f64, bool) {
    match piece {
        Piece::Segment { a, b } => (zip(*a, *b, f64::midpoint), norm(sub(*b, *a)) / 2.0, true),
        Piece::Cube { solid, lo, hi } => {
            let mid = zip(*lo, *hi, f64::midpoint);
            let reach: Vec3 = std::array::from_fn(|i| (hi[i] - lo[i]) / 2.0 * norm(solid.axes[i]));
            (
                solid.point(mid),
                norm(reach),
                !solid.ball || norm(mid) <= 1.0,
            )
        }
        Piece::Sky {
            order,
            hash,
            near,
            far,
        } => {
            let middle = f64::midpoint(*near, *far);
            let beta = healpix::radius_bound(*order).min(PI);
            let reach = |r: f64| {
                (r * r + middle * middle - 2.0 * r * middle * beta.cos())
                    .max(0.0)
                    .sqrt()
            };
            let direction = healpix::centre(*order, *hash);
            (
                placing.at(scaled(direction, middle)),
                reach(*near).max(reach(*far)) * placing.factor,
                true,
            )
        }
    }
}

/// A piece cut in two, or in four for a sky cell split by direction.
fn split(placing: &Placing, piece: &Piece) -> Vec<Piece> {
    match *piece {
        Piece::Segment { a, b } => {
            let m = zip(a, b, f64::midpoint);
            vec![Piece::Segment { a, b: m }, Piece::Segment { a: m, b }]
        }
        Piece::Cube { solid, lo, hi } => {
            let longest = (0..3)
                .max_by(|&i, &j| {
                    ((hi[i] - lo[i]) * norm(solid.axes[i]))
                        .total_cmp(&((hi[j] - lo[j]) * norm(solid.axes[j])))
                })
                .unwrap_or(0);
            let cut = f64::midpoint(lo[longest], hi[longest]);
            let mut lower_hi = hi;
            lower_hi[longest] = cut;
            let mut upper_lo = lo;
            upper_lo[longest] = cut;
            vec![
                Piece::Cube {
                    solid,
                    lo,
                    hi: lower_hi,
                },
                Piece::Cube {
                    solid,
                    lo: upper_lo,
                    hi,
                },
            ]
        }
        Piece::Sky {
            order,
            hash,
            near,
            far,
        } => {
            let across = f64::midpoint(near, far) * healpix::radius_bound(order) * placing.factor;
            let deep = (far - near) / 2.0 * placing.factor;
            if order < healpix::MAX_ORDER && across >= deep {
                (0..4)
                    .map(|j| Piece::Sky {
                        order: order + 1,
                        hash: hash << 2 | j,
                        near,
                        far,
                    })
                    .collect()
            } else {
                let middle = f64::midpoint(near, far);
                vec![
                    Piece::Sky {
                        order,
                        hash,
                        near,
                        far: middle,
                    },
                    Piece::Sky {
                        order,
                        hash,
                        near: middle,
                        far,
                    },
                ]
            }
        }
    }
}
