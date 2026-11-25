//! Version 5: the Galaxy model's points (build-the-galaxy-model).
//!
//! A model point stands for a share of the Galaxy's light, not for an object: it has no object
//! id, source or kind of value. A tile of `n` model points is `48 + 8 n` bytes, its header
//! version 1's with the version field 5:
//!
//! | Offset | Bytes | Field |
//! |---|---|---|
//! | 48 | 6 n | Each point's quantized position: x, y, z as three `u16`, as in version 1 |
//! | 48 + 6 n | n | Each point's population, `u8`: its code in the Galaxy model's definition, from 1 |
//! | 48 + 7 n | n | Each point's flags, `u8`: bit 0 set where real objects cover it ([`COVERED`]); no other bit is set |
//!
//! Only the tiles of the Galaxy model domain take version 5.

use super::read::{Header, byte_at, header, position_at};
use super::{
    NodeBox, TileError, header as write_header, max_points, quantized_positions, tile_bytes,
};
use crate::position::Vec3;

/// The encoding version of the Galaxy model's tiles.
pub const MODEL_VERSION: u16 = 5;
/// The bytes each model point takes: 6 of position, 1 of population, 1 of flags.
pub const MODEL_POINT_BYTES: usize = 8;
/// The flag of a model point that real objects cover.
pub const COVERED: u8 = 1;

/// A model point to encode: its position in the manifest's frame and unit, its population's
/// code, and whether real objects cover it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelPoint {
    pub position: Vec3,
    pub population: u8,
    pub covered: bool,
}

/// A decoded model point: its position at the centre of its quantization step, the step it was
/// stored as, its population's code and whether real objects cover it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecodedModelPoint {
    pub position: Vec3,
    pub quantized: [u16; 3],
    pub population: u8,
    pub covered: bool,
}

/// A decoded version 5 tile: its node and its model points, in the order stored.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelTile {
    pub node: NodeBox,
    pub points: Vec<DecodedModelPoint>,
}

/// Encodes `points` as a version 5 tile of `node`, in the order given.
///
/// # Errors
///
/// Fails on a node that cannot stand, more points than a tile holds, a point outside the node,
/// or a population code of 0.
pub fn encode_model(node: &NodeBox, points: &[ModelPoint]) -> Result<Vec<u8>, TileError> {
    let node = NodeBox::new(node.min(), node.edge())?;
    let n = points.len();
    let count = u32::try_from(n)
        .ok()
        .filter(|_| n <= max_points(MODEL_POINT_BYTES))
        .ok_or(TileError::TooManyPoints {
            points: n,
            point_bytes: MODEL_POINT_BYTES,
        })?;
    for (point, p) in points.iter().enumerate() {
        if !node.contains(p.position) {
            return Err(TileError::ModelOutsideNode {
                point,
                position: p.position,
            });
        }
        if p.population == 0 {
            return Err(TileError::Population { point });
        }
    }
    let mut bytes = write_header(
        MODEL_VERSION,
        count,
        &node,
        tile_bytes(MODEL_POINT_BYTES, n),
    );
    quantized_positions(&mut bytes, &node, points.iter().map(|p| p.position));
    bytes.extend(points.iter().map(|p| p.population));
    bytes.extend(points.iter().map(|p| if p.covered { COVERED } else { 0 }));
    Ok(bytes)
}

/// Decodes a version 5 tile into its node and model points.
///
/// # Errors
///
/// Fails, never panicking, as [`super::decode`] does on its header; on a tile of another
/// version, which holds objects; and on a population code of 0 or a flag beyond [`COVERED`].
pub fn decode_model(bytes: &[u8]) -> Result<ModelTile, TileError> {
    let Header {
        version,
        points: n,
        node,
    } = header(bytes)?;
    if version != MODEL_VERSION {
        return Err(TileError::ModelVersion(version));
    }
    let (populations, flags) = (super::HEADER_BYTES + 6 * n, super::HEADER_BYTES + 7 * n);
    let mut points = Vec::with_capacity(n);
    for k in 0..n {
        let (quantized, position) = position_at(bytes, &node, k)?;
        let population = byte_at(bytes, populations + k)?;
        if population == 0 {
            return Err(TileError::Population { point: k });
        }
        let flag = byte_at(bytes, flags + k)?;
        if flag & !COVERED != 0 {
            return Err(TileError::Flags {
                point: k,
                flags: flag,
            });
        }
        points.push(DecodedModelPoint {
            position,
            quantized,
            population,
            covered: flag & COVERED != 0,
        });
    }
    Ok(ModelTile { node, points })
}
