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
//! # Version 2
//!
//! Version 2 is version 1 with the version field 2 and two columns more, after the kinds of
//! value, so a tile of `n` points is `48 + 20 n` bytes:
//!
//! | Offset | Bytes | Field |
//! |---|---|---|
//! | 48 + 17 n | 2 n | Each point's colour index in hundredths of a magnitude, `i16`; −32768 for none |
//! | 48 + 19 n | n | Each point's colour bands, `u8`: 0 for none, else 1 + its index in [`COLOUR_BANDS`] |
//!
//! A point carries the colour index chosen for its object (build-octree-tiles), rounded to the
//! nearest hundredth of a magnitude, so it decodes within 0.005 magnitudes of the value it was
//! built from. A point with no colour index has both −32768 and 0, and a decoder refuses a point
//! with one and not the other.
//!
//! # Version 3
//!
//! Version 3 is version 2 with the version field 3, its points the same 20 bytes. The two last
//! columns hold a point's value, a colour index or an age, and the code of what it is:
//!
//! | Offset | Bytes | Field |
//! |---|---|---|
//! | 48 + 17 n | 2 n | Each point's value in hundredths, `i16`; −32768 for none |
//! | 48 + 19 n | n | Its code, `u8`: 0 for none, 1 or 2 for a colour index between the bands of [`COLOUR_BANDS`] (in hundredths of a magnitude), [`LOG_AGE_CODE`] for the base-10 logarithm of an age in years (in hundredths) |
//!
//! A point of the Galaxy tracers domain carries its object's preferred age where it has one,
//! else its colour index; a point never carries both. An age decodes within 0.005 of the
//! logarithm it was built from. A version 2 decoder refuses code 3, and a version 3 decoder
//! refuses it in a version 2 tile.
//!
//! # Version 4
//!
//! Version 4 is version 3 with the version field 4 and one column more, after the value codes,
//! so a tile of `n` points is `48 + 21 n` bytes:
//!
//! | Offset | Bytes | Field |
//! |---|---|---|
//! | 48 + 20 n | n | Each point's kind of object, `u8`: 0 for none, else 1 + its index in [`OBJECT_KINDS`] |
//!
//! Only the tiles of the Galaxy tracers domain take version 4 (build-octree-tiles); every other
//! domain's tiles keep version 3, so that a release adding version 4 leaves their files as they
//! were. A point whose object is one body, named by no kind of object, has code 0. A decoder
//! refuses a code beyond [`OBJECT_KINDS`].
//!
//! A decoder refuses a tile with another magic, a version it does not know (naming it), a
//! reserved field that is not zero, fewer or more bytes than its count gives, more than
//! [`MAX_TILE_BYTES`], a node that cannot stand (see [`NodeBox::new`]), an object id outside 1
//! to 2^53 − 1, a kind of value or, from version 4, a kind of object it does not know; it never
//! panics. It reads the magic and the
//! version before the size, so a tile of a version it does not know is named as such at any
//! size.

#[cfg(test)]
mod age_tests;
pub mod format;
#[cfg(test)]
mod format_tests;
pub mod manifest;
mod object;
mod read;
#[cfg(test)]
mod tests;
mod value;

use serde::Serialize;
use thiserror::Error;

pub use object::{OBJECT_KINDS, has_object_kind, object_kind_code, object_kind_of_code};
pub use read::decode;
pub use value::{
    COLOUR_BANDS, ColourBands, LOG_AGE_CODE, NO_VALUE, PointColour, PointValue, has_age, has_colour,
};

use value::hundredths;

use crate::ids::ObjectId;
use crate::object::ObjectKind;
use crate::position::Vec3;
use crate::value::KindOfValue;

/// The magic a tile starts with.
pub const MAGIC: [u8; 4] = *b"UVTL";
/// The newest version of the encoding this module writes: the Galaxy tracers' tiles take it.
/// It also writes [`BASE_VERSION`], for every other domain, and reads versions 1 to 4.
pub const VERSION: u16 = 4;
/// The version every scale domain's tiles take but the Galaxy tracers'.
pub const BASE_VERSION: u16 = 3;
/// The bytes before the first point.
pub const HEADER_BYTES: usize = 48;
/// The bytes each point takes in version 1: 6 of position, 8 of object id, 2 of source, 1 of
/// kind of value.
pub const V1_POINT_BYTES: usize = 17;
/// The bytes each point takes in versions 2 and 3: version 1's, then 2 of a colour index (or,
/// in version 3, an age) and 1 of its code.
pub const POINT_BYTES: usize = 20;
/// The bytes each point takes in version 4: version 3's, then 1 of its kind of object.
pub const V4_POINT_BYTES: usize = 21;
/// The largest tile a release may hold: 1 MiB, the release's quality bound.
pub const MAX_TILE_BYTES: usize = 1_048_576;
/// The most points a tile of any version this module writes may hold within
/// [`MAX_TILE_BYTES`]: 49,930, a version 4 tile's, the widest points.
pub const MAX_POINTS: usize = max_points(V4_POINT_BYTES);

/// The encoding versions this module reads, oldest first; it writes the last two,
/// [`BASE_VERSION`] and [`VERSION`].
pub const VERSIONS: [u16; 4] = [1, 2, BASE_VERSION, VERSION];

/// The bytes each point takes in tiles of `version`, if one of [`VERSIONS`].
#[must_use]
pub const fn point_bytes(version: u16) -> Option<usize> {
    match version {
        1 => Some(V1_POINT_BYTES),
        2 | BASE_VERSION => Some(POINT_BYTES),
        VERSION => Some(V4_POINT_BYTES),
        _ => None,
    }
}

/// The most points a tile of `point_bytes` a point may hold within [`MAX_TILE_BYTES`].
#[must_use]
pub const fn max_points(point_bytes: usize) -> usize {
    (MAX_TILE_BYTES - HEADER_BYTES) / point_bytes
}

/// The quantization steps along each axis of a node's edge.
pub const STEPS: u32 = 65_536;

/// The size of a tile of `points` points of the widest version this module writes, in bytes.
#[must_use]
pub const fn tile_bytes(points: usize) -> usize {
    HEADER_BYTES + V4_POINT_BYTES * points
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
    /// The colour index chosen for its object, or the log of its age, if it carries one.
    pub value: Option<PointValue>,
    /// Its object's kind, where a version 4 tile carries it; none for one body.
    pub object: Option<ObjectKind>,
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
    /// Its colour index, or from version 3 its log age, to the hundredth; none in a version 1
    /// tile.
    pub value: Option<PointValue>,
    /// Its object's kind, from version 4; none for one body or in an older tile.
    pub object: Option<ObjectKind>,
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
        "{points} points of {point_bytes} bytes take {} bytes, beyond the {MAX_TILE_BYTES} a tile \
         may hold (at most {} points)",
        HEADER_BYTES.saturating_add(point_bytes.saturating_mul(*points)),
        max_points(*point_bytes)
    )]
    TooManyPoints { points: usize, point_bytes: usize },
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
    #[error("the tile encoding version {0} is not one this decoder knows (it knows {VERSIONS:?})")]
    UnknownVersion(u16),
    #[error("a reserved field of the tile header is not zero")]
    Reserved,
    #[error("point {point} has the object id {id}, outside 1 to 2^53 - 1")]
    ObjectId { point: usize, id: u64 },
    #[error("point {point} has the kind of value {code}, which the encoding does not define")]
    Kind { point: usize, code: u8 },
    #[error("point {point} has the kind of object code {code}, which the encoding does not define")]
    ObjectKind { point: usize, code: u8 },
    #[error("tiles are written in encoding versions {BASE_VERSION} and {VERSION}, not {0}")]
    UnwrittenVersion(u16),
    #[error(
        "point {point} has the value code {code}, which the tile's version does not define or \
         its value contradicts"
    )]
    ValueCode { point: usize, code: u8 },
    #[error(
        "object {id}'s colour index {index} lies beyond the -327.67 to 327.67 magnitudes a tile \
         holds"
    )]
    ColourRange { id: ObjectId, index: f64 },
    #[error("object {id}'s log age {log_age} lies beyond the -327.67 to 327.67 a tile holds")]
    AgeRange { id: ObjectId, log_age: f64 },
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

/// Encodes `points` as a version 3 tile of `node`, in the order given: [`encode_as`] with
/// [`BASE_VERSION`].
///
/// # Errors
///
/// As [`encode_as`].
pub fn encode(node: &NodeBox, points: &[TilePoint]) -> Result<Vec<u8>, TileError> {
    encode_as(BASE_VERSION, node, points)
}

/// Encodes `points` as a tile of `node` in encoding `version`, [`BASE_VERSION`] or
/// [`VERSION`], in the order given. Version 3 leaves out each point's kind of object.
///
/// # Errors
///
/// Fails on another version, a node that cannot stand (a child cube may halve below a normal
/// step), more points than the version's tile holds, a point outside the node or not finite, or
/// a colour index or an age beyond what a tile holds.
pub fn encode_as(version: u16, node: &NodeBox, points: &[TilePoint]) -> Result<Vec<u8>, TileError> {
    let point_bytes = match version {
        BASE_VERSION | VERSION => point_bytes(version).unwrap_or(V4_POINT_BYTES),
        other => return Err(TileError::UnwrittenVersion(other)),
    };
    let node = NodeBox::new(node.min, node.edge)?;
    let n = points.len();
    let count = u32::try_from(n)
        .ok()
        .filter(|_| n <= max_points(point_bytes))
        .ok_or(TileError::TooManyPoints {
            points: n,
            point_bytes,
        })?;
    if let Some(p) = points.iter().find(|p| !node.contains(p.position)) {
        return Err(TileError::OutsideNode {
            id: p.id,
            position: p.position,
        });
    }
    let mut bytes = Vec::with_capacity(HEADER_BYTES + point_bytes * n);
    bytes.extend_from_slice(&MAGIC);
    bytes.extend_from_slice(&version.to_le_bytes());
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
    let mut codes = Vec::with_capacity(n);
    for p in points {
        let (held, code) = match p.value {
            None => (NO_VALUE, 0),
            Some(v) => {
                let (number, code) = v.coded();
                let held = hundredths(number).ok_or(match v {
                    PointValue::Colour(c) => TileError::ColourRange {
                        id: p.id,
                        index: c.index,
                    },
                    PointValue::LogAge(log_age) => TileError::AgeRange { id: p.id, log_age },
                })?;
                (held, code)
            }
        };
        bytes.extend_from_slice(&held.to_le_bytes());
        codes.push(code);
    }
    bytes.extend(codes);
    if has_object_kind(version) {
        bytes.extend(points.iter().map(|p| object_kind_code(p.object)));
    }
    Ok(bytes)
}
