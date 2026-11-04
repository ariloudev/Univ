//! The tile encoding: the points of one octree node at one level of detail, as a release stores
//! them and a client decodes them, with its encoder and its reference decoder.
//!
//! # Version 1
//!
//! Every number is little-endian; nothing is aligned or padded. A tile of `n` points is
//! `48 + 17 n` bytes:
//!
//! | Offset | Bytes | Field |
//! |---|---|---|
//! | 0 | 4 | Magic `UVTL` (`55 56 54 4C`) |
//! | 4 | 2 | Encoding version, `u16`: 1 |
//! | 6 | 2 | Reserved, `u16`: 0 |
//! | 8 | 4 | Point count `n`, `u32` |
//! | 12 | 4 | Reserved, `u32`: 0 |
//! | 16 | 24 | The node's minimum corner x, y, z, three `f64`, in the manifest's frame and unit |
//! | 40 | 8 | The node's edge, `f64`, in the manifest's unit |
//! | 48 | 6 n | Each point's quantized position: x, y, z as three `u16` |
//! | 48 + 6 n | 8 n | Each point's object id, `u64` (1 to 2^53 − 1) |
//! | 48 + 14 n | 2 n | Each point's source: a `u16` index into the manifest's source table |
//! | 48 + 16 n | n | Each point's kind of value, `u8`: its index in [`KINDS`] |
//!
//! The columns hold the points in the same order, so the positions can go to a GPU as one
//! `Uint16Array`.
//!
//! A point's source is the source of the measurement that gives the point its distance, its
//! preferred distance, and its kind of value is that measurement's. Its sky position may come
//! from another source; the manifest's source table says what each source gives its points.
//!
//! A position is quantized relative to its node: the node's edge is cut into 65,536 steps per
//! axis, so the step is exactly 1/65,536 of the edge, and a coordinate `p` is stored as
//! `q = floor((p − min) / step)`, clamped to 0..=65535 so that the node's upper face falls in the
//! last step. It decodes to the centre of its step, `min + (q + 0.5) · step`, which lies within
//! half a step of `p` on each axis.
//!
//! A decoder refuses a tile with another magic, a version it does not know (naming it), a
//! reserved field that is not zero, fewer or more bytes than its count gives, more than
//! [`MAX_TILE_BYTES`], a node that cannot stand (see [`NodeBox::new`]), an object id outside 1
//! to 2^53 − 1, or a kind of value it does not know; it never panics. It reads the magic and the
//! version before the size, so a tile of a version it does not know is named as such at any
//! size.

pub mod format;
#[cfg(test)]
mod format_tests;
pub mod manifest;
mod read;
#[cfg(test)]
mod tests;

use serde::Serialize;
use thiserror::Error;

pub use read::decode;

use crate::ids::ObjectId;
use crate::position::Vec3;
use crate::value::KindOfValue;

/// The magic a tile starts with.
pub const MAGIC: [u8; 4] = *b"UVTL";
/// The version of the encoding this module writes and reads.
pub const VERSION: u16 = 1;
/// The bytes before the first point.
pub const HEADER_BYTES: usize = 48;
/// The bytes each point takes: 6 of position, 8 of object id, 2 of source, 1 of kind of value.
pub const POINT_BYTES: usize = 17;
/// The largest tile a release may hold: 1 MiB, the release's quality bound.
pub const MAX_TILE_BYTES: usize = 1_048_576;
/// The most points a tile may hold within [`MAX_TILE_BYTES`]: 61,678.
pub const MAX_POINTS: usize = (MAX_TILE_BYTES - HEADER_BYTES) / POINT_BYTES;
/// The quantization steps along each axis of a node's edge.
pub const STEPS: u32 = 65_536;

/// The size of a tile of `points` points, in bytes.
#[must_use]
pub const fn tile_bytes(points: usize) -> usize {
    HEADER_BYTES + POINT_BYTES * points
}

/// The cube of one octree node: its minimum corner and its edge, in the manifest's frame and
/// unit. A point on its upper faces belongs to it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct NodeBox {
    min: Vec3,
    edge: f64,
}

impl NodeBox {
    /// The cube with minimum corner `min` and edge `edge`.
    ///
    /// # Errors
    ///
    /// Fails unless the corner and the far corner (`min + edge` on each axis) are finite and
    /// the step, `edge / 65,536`, is a positive normal number, so that every decoded position
    /// is finite and every step distinct.
    pub fn new(min: Vec3, edge: f64) -> Result<Self, TileError> {
        let step = edge / f64::from(STEPS);
        let stands = step.is_normal()
            && step > 0.0
            && min.iter().all(|c| c.is_finite() && (c + edge).is_finite());
        if stands {
            Ok(Self { min, edge })
        } else {
            Err(TileError::BadNode { min, edge })
        }
    }

    #[must_use]
    pub fn min(&self) -> Vec3 {
        self.min
    }

    #[must_use]
    pub fn edge(&self) -> f64 {
        self.edge
    }

    #[must_use]
    pub fn max(&self) -> Vec3 {
        self.min.map(|c| c + self.edge)
    }

    /// The quantization step of a point in this node: 1/65,536 of its edge.
    #[must_use]
    pub fn step(&self) -> f64 {
        self.edge / f64::from(STEPS)
    }

    /// Whether `p` lies in the cube, its faces included.
    #[must_use]
    pub fn contains(&self, p: Vec3) -> bool {
        (0..3).all(|i| p[i] >= self.min[i] && p[i] <= self.min[i] + self.edge)
    }

    /// The octant of the cube that `p` falls in: bit 0 set for the upper half in x, bit 1 in y,
    /// bit 2 in z, a coordinate on the middle plane going to the upper half.
    #[must_use]
    pub fn octant_of(&self, p: Vec3) -> u8 {
        let half = self.edge / 2.0;
        (0..3).fold(0, |octant, i| {
            if p[i] >= self.min[i] + half {
                octant | (1 << i)
            } else {
                octant
            }
        })
    }

    /// The child cube of octant `octant` (0 to 7, as [`NodeBox::octant_of`] numbers them).
    #[must_use]
    pub fn child(&self, octant: u8) -> Self {
        let half = self.edge / 2.0;
        let mut min = self.min;
        for (i, c) in min.iter_mut().enumerate() {
            if octant & (1 << i) != 0 {
                *c += half;
            }
        }
        Self { min, edge: half }
    }
}

/// A point to encode: its position in the manifest's frame and unit, its object, its source's
/// index in the manifest's source table and the kind of value that places it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TilePoint {
    pub position: Vec3,
    pub id: ObjectId,
    pub source: u16,
    pub kind: KindOfValue,
}

/// A decoded point: its position at the centre of its quantization step, the step it was
/// stored as, its object, its source's index and its kind of value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecodedPoint {
    pub position: Vec3,
    pub quantized: [u16; 3],
    pub id: ObjectId,
    pub source: u16,
    pub kind: KindOfValue,
}

/// A decoded tile: its node and its points, in the order stored.
#[derive(Clone, Debug, PartialEq)]
pub struct Tile {
    pub version: u16,
    pub node: NodeBox,
    pub points: Vec<DecodedPoint>,
}

/// A tile that cannot be encoded or decoded.
#[derive(Debug, Error, PartialEq)]
pub enum TileError {
    #[error(
        "the node's corner {min:?} and edge {edge} cannot stand: its corners must be finite and \
         its step, the edge over 65,536, a positive normal number"
    )]
    BadNode { min: Vec3, edge: f64 },
    #[error(
        "{points} points take {} bytes, beyond the {MAX_TILE_BYTES} a tile may hold (at most \
         {MAX_POINTS} points)",
        tile_bytes(*points)
    )]
    TooManyPoints { points: usize },
    #[error("object {id}'s position {position:?} lies outside its node")]
    OutsideNode { id: ObjectId, position: Vec3 },
    #[error("the tile is {found} bytes, short of the {expected} its header gives")]
    Truncated { expected: usize, found: usize },
    #[error("the tile is {found} bytes, beyond the {expected} its header gives")]
    TrailingBytes { expected: usize, found: usize },
    #[error("the tile is {0} bytes, beyond the {MAX_TILE_BYTES} a tile may hold")]
    Oversized(usize),
    #[error("the tile starts with {0:?}, not the magic UVTL")]
    Magic([u8; 4]),
    #[error("the tile encoding version {0} is not one this decoder knows (it knows {VERSION})")]
    UnknownVersion(u16),
    #[error("a reserved field of the tile header is not zero")]
    Reserved,
    #[error("point {point} has the object id {id}, outside 1 to 2^53 - 1")]
    ObjectId { point: usize, id: u64 },
    #[error("point {point} has the kind of value {code}, which version 1 does not define")]
    Kind { point: usize, code: u8 },
}

/// The kinds of value of version 1, each coded as its index.
pub const KINDS: [KindOfValue; 4] = [
    KindOfValue::Measured,
    KindOfValue::Estimated,
    KindOfValue::Inferred,
    KindOfValue::Reconstructed,
];

/// The code version 1 gives a kind of value: its index in [`KINDS`].
#[must_use]
pub fn kind_code(kind: KindOfValue) -> u8 {
    KINDS
        .iter()
        .position(|k| *k == kind)
        .and_then(|i| u8::try_from(i).ok())
        .unwrap_or_default()
}

/// The kind of value of a version 1 code.
#[must_use]
pub fn kind_of_code(code: u8) -> Option<KindOfValue> {
    KINDS.get(usize::from(code)).copied()
}

/// The step of the coordinate `p` along an axis starting at `min` with step `step`, clamped
/// so that the node's upper face falls in the last step.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the value is clamped to 0..=65535 before the cast"
)]
fn quantize(p: f64, min: f64, step: f64) -> u16 {
    ((p - min) / step).floor().clamp(0.0, f64::from(u16::MAX)) as u16
}

/// Encodes `points` as a version 1 tile of `node`, in the order given.
///
/// # Errors
///
/// Fails on a node that cannot stand (a child cube may halve below a normal step), more points
/// than [`MAX_POINTS`], or a point outside the node or not finite.
pub fn encode(node: &NodeBox, points: &[TilePoint]) -> Result<Vec<u8>, TileError> {
    let node = NodeBox::new(node.min, node.edge)?;
    let n = points.len();
    let count = u32::try_from(n)
        .ok()
        .filter(|_| n <= MAX_POINTS)
        .ok_or(TileError::TooManyPoints { points: n })?;
    if let Some(p) = points.iter().find(|p| !node.contains(p.position)) {
        return Err(TileError::OutsideNode {
            id: p.id,
            position: p.position,
        });
    }
    let mut bytes = Vec::with_capacity(tile_bytes(n));
    bytes.extend_from_slice(&MAGIC);
    bytes.extend_from_slice(&VERSION.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&count.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    for c in node.min.iter().chain([&node.edge]) {
        bytes.extend_from_slice(&c.to_le_bytes());
    }
    let step = node.step();
    for p in points {
        for i in 0..3 {
            let q = quantize(p.position[i], node.min[i], step);
            bytes.extend_from_slice(&q.to_le_bytes());
        }
    }
    for p in points {
        bytes.extend_from_slice(&p.id.get().to_le_bytes());
    }
    for p in points {
        bytes.extend_from_slice(&p.source.to_le_bytes());
    }
    bytes.extend(points.iter().map(|p| kind_code(p.kind)));
    Ok(bytes)
}
