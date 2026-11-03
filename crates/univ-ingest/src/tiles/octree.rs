//! A scale domain's octree: its root cube, its nodes split down to the release's maximum depth,
//! and the seeded subsample each inner node keeps.

use rand_chacha::ChaCha20Rng;
use rand_chacha::rand_core::{Rng, SeedableRng};
use univ_model::position::Vec3;
use univ_model::tile::{MAX_POINTS, NodeBox, TileError, tile_bytes};

use super::TilesError;
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

/// The root of a domain whose points are `positions`: a cube centred on the origin whose
/// half-edge is the smallest power of two not below the largest |coordinate|, 1 for none.
///
/// # Errors
///
/// Fails only on a coordinate that is not finite, which leaves no cube to hold it.
pub fn root_box(positions: &[Vec3]) -> Result<NodeBox, TileError> {
    let largest = positions
        .iter()
        .flatten()
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

/// A node still to be laid out.
struct Pending {
    address: String,
    level: u8,
    bounds: NodeBox,
    points: Vec<usize>,
}

/// Lays out the `domain`'s `positions` (all inside `root`) as an octree under `rules`, drawing each inner
/// node's subsample from `rng`. Nodes come in depth-first order of address, children in octant
/// order, and that is the order the draws are made in.
///
/// # Errors
///
/// Fails when a leaf at the maximum depth holds more points than a tile within 1 MiB can,
/// naming it.
pub fn build(
    domain: &str,
    positions: &[Vec3],
    root: NodeBox,
    rules: &TileRules,
    rng: &mut ChaCha20Rng,
) -> Result<Vec<Node>, TilesError> {
    let mut nodes = Vec::new();
    if positions.is_empty() {
        return Ok(nodes);
    }
    let mut stack = vec![Pending {
        address: "r".to_owned(),
        level: 0,
        bounds: root,
        points: (0..positions.len()).collect(),
    }];
    while let Some(Pending {
        address,
        level,
        bounds,
        mut points,
    }) = stack.pop()
    {
        let n = points.len();
        if n <= rules.split_threshold || level >= rules.max_depth {
            if n > MAX_POINTS {
                return Err(TilesError::TileTooLarge {
                    domain: domain.to_owned(),
                    address,
                    points: n,
                    bytes: tile_bytes(n),
                });
            }
            nodes.push(Node {
                address,
                level,
                bounds,
                sampled_from: n,
                points,
            });
            continue;
        }
        sample_to_front(&mut points, rules.level_points, rng);
        let mut rest = points.split_off(rules.level_points);
        points.sort_unstable();
        rest.sort_unstable();
        let mut children: [Vec<usize>; 8] = Default::default();
        for i in rest {
            children[usize::from(bounds.octant_of(positions[i]))].push(i);
        }
        nodes.push(Node {
            address: address.clone(),
            level,
            bounds,
            sampled_from: n,
            points,
        });
        for (octant, child) in (0u8..8).zip(children).rev() {
            if !child.is_empty() {
                stack.push(Pending {
                    address: format!("{address}{octant}"),
                    level: level + 1,
                    bounds: bounds.child(octant),
                    points: child,
                });
            }
        }
    }
    Ok(nodes)
}
