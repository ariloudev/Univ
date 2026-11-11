//! The reference decoder of the tile encoding.

use super::value::point_value;
use super::{
    DecodedPoint, HEADER_BYTES, MAGIC, MAX_TILE_BYTES, NodeBox, POINT_BYTES, Tile, TileError,
    V1_POINT_BYTES, has_colour, has_object_kind, kind_of_code, max_points, object_kind_of_code,
    point_bytes, tile_bytes,
};
use crate::ids::ObjectId;

/// The `N` bytes of `bytes` at `at`.
fn array<const N: usize>(bytes: &[u8], at: usize) -> Result<[u8; N], TileError> {
    bytes
        .get(at..at + N)
        .and_then(|b| b.try_into().ok())
        .ok_or(TileError::Truncated {
            expected: at + N,
            found: bytes.len(),
        })
}

fn f64_at(bytes: &[u8], at: usize) -> Result<f64, TileError> {
    array(bytes, at).map(f64::from_le_bytes)
}

/// Decodes a tile of a version this decoder knows (1 to 4) into its node and points.
///
/// # Errors
///
/// Fails, never panicking, on another magic or an unknown version (named), both read before the
/// size, a reserved field that is not zero, a length other than its count gives or beyond [`MAX_TILE_BYTES`], a node that
/// cannot stand, an object id outside 1 to 2^53 − 1, an unknown kind of value, or, from version
/// 2, a value code the version does not define or one its value contradicts, or, from version
/// 4, a kind of object code beyond the encoding's kinds of object.
pub fn decode(bytes: &[u8]) -> Result<Tile, TileError> {
    let magic: [u8; 4] = array(bytes, 0)?;
    if magic != MAGIC {
        return Err(TileError::Magic(magic));
    }
    let version = u16::from_le_bytes(array(bytes, 4)?);
    let Some(point_bytes) = point_bytes(version) else {
        return Err(TileError::UnknownVersion(version));
    };
    if bytes.len() > MAX_TILE_BYTES {
        return Err(TileError::Oversized(bytes.len()));
    }
    if bytes.len() < HEADER_BYTES {
        return Err(TileError::Truncated {
            expected: HEADER_BYTES,
            found: bytes.len(),
        });
    }
    let reserved =
        u16::from_le_bytes(array(bytes, 6)?) != 0 || u32::from_le_bytes(array(bytes, 12)?) != 0;
    if reserved {
        return Err(TileError::Reserved);
    }
    let count = u32::from_le_bytes(array(bytes, 8)?);
    let n = usize::try_from(count).unwrap_or(usize::MAX);
    if n > max_points(point_bytes) {
        return Err(TileError::TooManyPoints {
            points: n,
            point_bytes,
        });
    }
    let expected = tile_bytes(point_bytes, n);
    if bytes.len() < expected {
        return Err(TileError::Truncated {
            expected,
            found: bytes.len(),
        });
    }
    if bytes.len() > expected {
        return Err(TileError::TrailingBytes {
            expected,
            found: bytes.len(),
        });
    }
    let min = [f64_at(bytes, 16)?, f64_at(bytes, 24)?, f64_at(bytes, 32)?];
    let node = NodeBox::new(min, f64_at(bytes, 40)?)?;
    let step = node.step();
    let (ids, sources, kinds) = (
        HEADER_BYTES + 6 * n,
        HEADER_BYTES + 14 * n,
        HEADER_BYTES + 16 * n,
    );
    let (colours, bands) = (
        HEADER_BYTES + V1_POINT_BYTES * n,
        HEADER_BYTES + (V1_POINT_BYTES + 2) * n,
    );
    let objects = HEADER_BYTES + POINT_BYTES * n;
    let mut points = Vec::with_capacity(n);
    for k in 0..n {
        let mut quantized = [0u16; 3];
        for (i, q) in quantized.iter_mut().enumerate() {
            *q = u16::from_le_bytes(array(bytes, HEADER_BYTES + 6 * k + 2 * i)?);
        }
        let raw = u64::from_le_bytes(array(bytes, ids + 8 * k)?);
        let id = ObjectId::new(raw).map_err(|_| TileError::ObjectId { point: k, id: raw })?;
        let source = u16::from_le_bytes(array(bytes, sources + 2 * k)?);
        let [code] = array(bytes, kinds + k)?;
        let kind = kind_of_code(code).ok_or(TileError::Kind { point: k, code })?;
        let position = [0, 1, 2].map(|i| min[i] + (f64::from(quantized[i]) + 0.5) * step);
        let value = if has_colour(version) {
            let held = i16::from_le_bytes(array(bytes, colours + 2 * k)?);
            let [code] = array(bytes, bands + k)?;
            point_value(k, version, held, code)?
        } else {
            None
        };
        let object = if has_object_kind(version) {
            let [code] = array(bytes, objects + k)?;
            object_kind_of_code(code).map_err(|code| TileError::ObjectKind { point: k, code })?
        } else {
            None
        };
        points.push(DecodedPoint {
            position,
            quantized,
            id,
            source,
            kind,
            value,
            object,
        });
    }
    Ok(Tile {
        version,
        node,
        points,
    })
}
