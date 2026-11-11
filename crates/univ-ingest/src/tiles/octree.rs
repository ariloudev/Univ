//! A scale domain's octree: its root cube, its nodes split down to the release's maximum depth,
//! and the seeded subsample each inner node keeps, one fraction for each level.

use rand_chacha::ChaCha20Rng;
use rand_chacha::rand_core::{Rng, SeedableRng};
use univ_model::tile::{NodeBox, TileError, max_points, tile_bytes};

use super::TilesError;
use super::domain::DomainPoint;
use super::rules::TileRules;

/// One node of an octree: its address, level and cube, how many points reached it, and the
/// points it keeps, as indices into the domain's points in increasing order.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// `r` for the root, then one octant digit, 0 to 7, per level.
    pub address: String,
    pub level: u8,
    pub bounds: NodeBox,
    /// The points that reached it: those of its cube that no node above keeps.
    pub sampled_from: usize,
    pub points: Vec<usize>,
}

/// A domain's octree: its nodes in depth-first order of address, and the fraction of the
/// points reaching it that each inner node of a level keeps, by level from the root.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Octree {
    pub nodes: Vec<Node>,
    /// For each level, `level_points / m`, `m` the most points reaching any inner node of the
    /// level; 1 for a level of leaves alone, which keep every point reaching them.
    pub fractions: Vec<f64>,
}

/// The root of a domain of `points`: a cube centred on the origin whose half-edge is the
/// smallest power of two not below the largest |coordinate|, 1 for none.
///
/// # Errors
///
/// Fails only on a coordinate too large for a cube to hold it.
pub fn root_box(points: &[DomainPoint]) -> Result<NodeBox, TileError> {
    let largest = points
        .iter()
        .flat_map(|p| p.position)
        .fold(0.0_f64, |m, c| m.max(c.abs()));
    let mut half = 1.0_f64;
    while half < largest && half.is_finite() {
        half *= 2.0;
    }
    while largest > 0.0 && half / 2.0 >= largest {
        half /= 2.0;
    }
    NodeBox::new([-half; 3], 2.0 * half)
}

/// The `ChaCha20` generator of a domain: keyed by the seed's eight little-endian bytes and 24
/// zero bytes, its stream the domain's number.
#[must_use]
pub fn generator(seed: u64, stream: u64) -> ChaCha20Rng {
    let mut key = [0u8; 32];
    key[..8].copy_from_slice(&seed.to_le_bytes());
    let mut rng = ChaCha20Rng::from_seed(key);
    rng.set_stream(stream);
    rng
}

/// A number below `n` (positive), each equally likely: a 64-bit draw, redrawn while it falls in
/// the 2^64 mod `n` values that would favour the smallest.
fn below(rng: &mut ChaCha20Rng, n: usize) -> usize {
    let n = u64::try_from(n).unwrap_or(u64::MAX);
    let threshold = n.wrapping_neg() % n;
    loop {
        let r = rng.next_u64();
        if r >= threshold {
            // `r % n` is below `n`, which came from a usize.
            return usize::try_from(r % n).unwrap_or_default();
        }
    }
}

/// Moves an even random subsample of `k` of `points` to its front: a partial Fisher–Yates
/// shuffle, each point equally likely, without replacement.
fn sample_to_front(points: &mut [usize], k: usize, rng: &mut ChaCha20Rng) {
    let n = points.len();
    for i in 0..k.min(n) {
        let j = i + below(rng, n - i);
        points.swap(i, j);
    }
}

/// The points an inner node of `n` points keeps at a level whose fullest inner node has
/// `largest`: `floor(level_points × n / largest)`, in integers, so never more than
/// `level_points`.
fn kept(level_points: usize, n: usize, largest: usize) -> usize {
    let wide = |x: usize| u128::try_from(x).unwrap_or(u128::MAX);
    let k = wide(level_points).saturating_mul(wide(n)) / wide(largest).max(1);
    usize::try_from(k).unwrap_or(level_points).min(level_points)
}

/// `level_points / largest`, the fraction a level's inner nodes keep.
#[expect(
    clippy::cast_precision_loss,
    reason = "point counts stay far below 2^53, where an f64 holds every integer"
)]
fn fraction(level_points: usize, largest: usize) -> f64 {
    level_points as f64 / largest as f64
}

/// A node still to be laid out.
struct Pending {
    address: String,
    bounds: NodeBox,
    points: Vec<usize>,
}

/// Lays out the `domain`'s `points` (all inside `root`) as an octree under `rules`, level by
/// level from the root. A node is inner when more points than the split threshold reach it,
/// above the maximum depth. Each level then has one fraction, `level_points / m` with `m` the
/// most points reaching any of its inner nodes, and an inner node of `n` points keeps an even
/// random subsample of `floor(level_points × n / m)` of them, drawn from `rng`, and passes the
/// rest to its children by octant; a leaf keeps every point reaching it. Draws are made level
/// by level, the nodes of a level in order of address.
///
/// # Errors
///
/// Fails when a leaf at the maximum depth holds more points than a tile of `point_bytes` a
/// point, the domain's encoding's, can within 1 MiB, naming it.
pub fn build(
    domain: &str,
    point_bytes: usize,
    points: &[DomainPoint],
    root: NodeBox,
    rules: &TileRules,
    rng: &mut ChaCha20Rng,
) -> Result<Octree, TilesError> {
    let mut tree = Octree::default();
    if points.is_empty() {
        return Ok(tree);
    }
    let mut level_nodes = vec![Pending {
        address: "r".to_owned(),
        bounds: root,
        points: (0..points.len()).collect(),
    }];
    let mut level = 0u8;
    while !level_nodes.is_empty() {
        let inner = |n: usize| n > rules.split_threshold && level < rules.max_depth;
        let largest = level_nodes
            .iter()
            .map(|p| p.points.len())
            .filter(|n| inner(*n))
            .max();
        tree.fractions
            .push(largest.map_or(1.0, |m| fraction(rules.level_points, m)));
        let mut next = Vec::new();
        for Pending {
            address,
            bounds,
            points: mut members,
        } in level_nodes
        {
            let n = members.len();
            let Some(largest) = largest.filter(|_| inner(n)) else {
                if n > max_points(point_bytes) {
                    return Err(TilesError::TileTooLarge {
                        domain: domain.to_owned(),
                        address,
                        points: n,
                        bytes: tile_bytes(point_bytes, n),
                    });
                }
                tree.nodes.push(Node {
                    address,
                    level,
                    bounds,
                    sampled_from: n,
                    points: members,
                });
                continue;
            };
            let k = kept(rules.level_points, n, largest);
            sample_to_front(&mut members, k, rng);
            let rest = members.split_off(k);
            members.sort_unstable();
            let mut children: [Vec<usize>; 8] = Default::default();
            for i in rest {
                children[usize::from(bounds.octant_of(points[i].position))].push(i);
            }
            for (octant, mut child) in (0u8..8).zip(children) {
                if !child.is_empty() {
                    child.sort_unstable();
                    next.push(Pending {
                        address: format!("{address}{octant}"),
                        bounds: bounds.child(octant),
                        points: child,
                    });
                }
            }
            tree.nodes.push(Node {
                address,
                level,
                bounds,
                sampled_from: n,
                points: members,
            });
        }
        level_nodes = next;
        level += 1;
    }
    tree.nodes.sort_by(|a, b| a.address.cmp(&b.address));
    Ok(tree)
}
