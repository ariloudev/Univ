//! The tile encoding described as data (decode-the-tile-format): for each version, the byte
//! layout of a tile, its header fields and then its per-point fields in order, each with its
//! name, type, place, size and meaning, and how a position is dequantized.
//!
//! The description is built from the constants and the field types the encoder in
//! [`super`] writes with, and its tests check it against encoded tiles, so it cannot drift from
//! the encoder.

use std::mem::size_of;

use serde::Serialize;

use super::{
    COLOUR_BANDS, ColourBands, HEADER_BYTES, KINDS, LOG_AGE_CODE, MAGIC, MAX_TILE_BYTES,
    OBJECT_KINDS, STEPS, has_age, has_colour, has_object_kind, max_points, point_bytes,
};
use crate::object::ObjectKind;
use crate::value::KindOfValue;

/// The type of a field's elements, every number little-endian and unaligned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    /// ASCII bytes.
    Ascii,
    U8,
    I16le,
    U16le,
    U32le,
    U64le,
    F64le,
}

impl FieldType {
    /// The bytes one element of the type takes.
    #[must_use]
    pub const fn bytes(self) -> usize {
        match self {
            Self::Ascii | Self::U8 => size_of::<u8>(),
            Self::I16le => size_of::<i16>(),
            Self::U16le => size_of::<u16>(),
            Self::U32le => size_of::<u32>(),
            Self::U64le => size_of::<u64>(),
            Self::F64le => size_of::<f64>(),
        }
    }
}

/// The value a header field must hold.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(untagged)]
pub enum Fixed {
    Number(u64),
    Text(String),
}

/// A field of the header, at a fixed offset from the start of the tile.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct HeaderField {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: FieldType,
    /// The elements of the type it holds, such as 3 for a corner's x, y and z.
    pub count: usize,
    /// Its first byte, from the start of the tile.
    pub offset: usize,
    /// `count` times the bytes of its type.
    pub bytes: usize,
    /// The value it must hold, if fixed; a decoder refuses any other.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Fixed>,
    pub meaning: String,
}

/// A field of every point. The points' fields are stored as columns, in the order listed: the
/// column of a field starts at `header_bytes + column_offset × n`, `n` the tile's point count,
/// and point `k` (from 0) holds its value at `bytes × k` within the column.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PointField {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: FieldType,
    /// The elements of the type it holds per point, such as 3 for x, y and z.
    pub count: usize,
    /// The bytes of the columns before it, per point.
    pub column_offset: usize,
    /// The bytes it takes per point: `count` times the bytes of its type.
    pub bytes: usize,
    pub meaning: String,
}

/// The byte layout of a tile of one version of the encoding.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct EncodingLayout {
    pub version: u32,
    pub byte_order: String,
    /// The bytes before the first point: the header fields' bytes together.
    pub header_bytes: usize,
    /// The bytes each point takes: the point fields' bytes together.
    pub point_bytes: usize,
    /// The size of a tile of `n` points.
    pub tile_bytes: String,
    pub max_tile_bytes: usize,
    pub max_points: usize,
    /// The quantization steps along a node's edge.
    pub steps_per_edge: u32,
    pub header: Vec<HeaderField>,
    pub points: Vec<PointField>,
    /// The kinds of value, coded by their index.
    pub kinds: Vec<KindOfValue>,
    /// The colour bands, coded by their index plus one, 0 coding none; none before version 2.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub colour_bands: Vec<ColourBands>,
    /// The code of a point whose value is the base-10 logarithm of its age in years; none before
    /// version 3.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_age_code: Option<u8>,
    /// The kinds of object, coded by their index plus one, 0 coding none; none before version 4.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub object_kinds: Vec<ObjectKind>,
    /// How a quantized position becomes a position in the manifest's frame and unit.
    pub dequantization: String,
    /// What a decoder refuses.
    pub refusals: Vec<String>,
}

/// The layout of tiles of encoding `version`, or `None` for a version this module does not know.
#[must_use]
pub fn layout(version: u32) -> Option<EncodingLayout> {
    let version = u16::try_from(version).ok()?;
    point_bytes(version).map(|point_bytes| layout_of(version, point_bytes))
}

/// The fields before the offsets: name, type, count, fixed value and meaning.
type Spec = (&'static str, FieldType, usize, Option<Fixed>, String);

/// The header fields of `version`, in order: the same for both but for the version's value.
fn header_of(version: u16) -> [Spec; 7] {
    let magic = String::from_utf8_lossy(&MAGIC).into_owned();
    [
        (
            "magic",
            FieldType::Ascii,
            MAGIC.len(),
            Some(Fixed::Text(magic.clone())),
            format!("the bytes {magic}, which every tile starts with"),
        ),
        (
            "version",
            FieldType::U16le,
            1,
            Some(Fixed::Number(version.into())),
            "the encoding version; a decoder reads it before the size and refuses, naming it, \
             a version it does not know"
                .to_owned(),
        ),
        (
            "reserved",
            FieldType::U16le,
            1,
            Some(Fixed::Number(0)),
            "reserved, 0".to_owned(),
        ),
        (
            "count",
            FieldType::U32le,
            1,
            None,
            "n, the points the tile holds: the node's point count in the manifest".to_owned(),
        ),
        (
            "reserved",
            FieldType::U32le,
            1,
            Some(Fixed::Number(0)),
            "reserved, 0".to_owned(),
        ),
        (
            "node_min",
            FieldType::F64le,
            3,
            None,
            "the node's minimum corner x, y, z in the manifest's frame and unit: the node's \
             bounds.min in the manifest"
                .to_owned(),
        ),
        (
            "node_edge",
            FieldType::F64le,
            1,
            None,
            "the node's edge in the manifest's unit: bounds.max - bounds.min on each axis, and \
             the edge of the node's level"
                .to_owned(),
        ),
    ]
}

/// The point fields of version 1, in the order of their columns.
fn points_v1() -> [Spec; 4] {
    [
        (
            "position",
            FieldType::U16le,
            3,
            None,
            format!(
                "the quantized x, y, z: q = floor((p - node_min) / step) on each axis, clamped \
                 to 0 to {}",
                STEPS - 1
            ),
        ),
        (
            "object_id",
            FieldType::U64le,
            1,
            None,
            "the object's id, 1 to 2^53 - 1, exact in a double".to_owned(),
        ),
        (
            "source",
            FieldType::U16le,
            1,
            None,
            "the index in the manifest's sources of the source of the point's preferred \
             distance, which gives its kind of value too"
                .to_owned(),
        ),
        (
            "kind",
            FieldType::U8,
            1,
            None,
            "the point's kind of value: its index in kinds, which the manifest's encoding.kinds \
             repeats"
                .to_owned(),
        ),
    ]
}

/// The point fields of `version`, in order: version 1's, then version 2's colour, or version
/// 3's value, after them, and version 4's kind of object last.
fn points_of(version: u16) -> Vec<Spec> {
    let mut fields = points_v1().to_vec();
    if has_age(version) {
        fields.extend([
            (
                "value",
                FieldType::I16le,
                1,
                None,
                "the point's value in hundredths, within 0.005 of the value it was built from: a \
                 colour index in magnitudes, or the base-10 logarithm of an age in years, as its \
                 code says; -32768 where it has none"
                    .to_owned(),
            ),
            (
                "value_code",
                FieldType::U8,
                1,
                None,
                format!(
                    "what the value is: 1 plus the index in colour_bands of the bands a colour \
                     index lies between, which the manifest's encoding.colour_bands repeats, or \
                     {LOG_AGE_CODE} for a log age, which encoding.log_age_code repeats; 0 where the \
                     point has no value"
                ),
            ),
        ]);
    } else if has_colour(version) {
        fields.extend([
            (
                "colour",
                FieldType::I16le,
                1,
                None,
                "the colour index chosen for the point's object, in hundredths of a magnitude, \
                 within 0.005 magnitudes of the value it was built from; -32768 where it has none"
                    .to_owned(),
            ),
            (
                "colour_bands",
                FieldType::U8,
                1,
                None,
                "the bands the colour index lies between: 1 plus their index in colour_bands, \
                 which the manifest's encoding.colour_bands repeats; 0 where the point has no \
                 colour index"
                    .to_owned(),
            ),
        ]);
    }
    if has_object_kind(version) {
        fields.push((
            "object_kind",
            FieldType::U8,
            1,
            None,
            "the kind of object the point is: 1 plus its index in object_kinds, which the \
             manifest's encoding.object_kinds repeats; 0 for one body, which names no kind"
                .to_owned(),
        ));
    }
    fields
}

fn layout_of(version: u16, point_bytes: usize) -> EncodingLayout {
    let mut offset = 0;
    let header = header_of(version)
        .into_iter()
        .map(|(name, field_type, count, value, meaning)| {
            let bytes = field_type.bytes() * count;
            offset += bytes;
            HeaderField {
                name: name.to_owned(),
                field_type,
                count,
                offset: offset - bytes,
                bytes,
                value,
                meaning,
            }
        })
        .collect();
    let mut column_offset = 0;
    let points = points_of(version)
        .into_iter()
        .map(|(name, field_type, count, _, meaning)| {
            let bytes = field_type.bytes() * count;
            column_offset += bytes;
            PointField {
                name: name.to_owned(),
                field_type,
                count,
                column_offset: column_offset - bytes,
                bytes,
                meaning,
            }
        })
        .collect();
    let coloured = has_colour(version);
    let mut refusals = vec![
        "another magic",
        "an encoding version the decoder does not know, named",
        "a reserved field that is not 0",
        "fewer or more bytes than header_bytes + point_bytes n",
        "more than max_tile_bytes",
        "a node whose corners are not finite or whose step is not a positive normal number",
        "an object id outside 1 to 2^53 - 1",
        "a kind of value beyond kinds",
    ];
    if has_age(version) {
        refusals.push(
            "a value code other than 0, 1 plus an index in colour_bands, or log_age_code, or a \
             value of -32768 with a code other than 0, or 0 with another value",
        );
    } else if coloured {
        refusals.push(
            "a colour bands code beyond colour_bands, or a colour index of -32768 with a code \
             other than 0, or 0 with another colour index",
        );
    }
    if has_object_kind(version) {
        refusals.push("a kind of object code beyond 1 plus the last index in object_kinds");
    }
    EncodingLayout {
        version: u32::from(version),
        byte_order: "little-endian, unaligned, unpadded".to_owned(),
        header_bytes: HEADER_BYTES,
        point_bytes,
        tile_bytes: format!("{HEADER_BYTES} + {point_bytes} n"),
        max_tile_bytes: MAX_TILE_BYTES,
        max_points: max_points(point_bytes),
        steps_per_edge: STEPS,
        header,
        points,
        kinds: KINDS.to_vec(),
        colour_bands: if coloured {
            COLOUR_BANDS.to_vec()
        } else {
            Vec::new()
        },
        log_age_code: has_age(version).then_some(LOG_AGE_CODE),
        object_kinds: if has_object_kind(version) {
            OBJECT_KINDS.to_vec()
        } else {
            Vec::new()
        },
        dequantization: format!(
            "step = node_edge / {STEPS}, the step of the node's level in the manifest; each \
             coordinate decodes to the centre of its step, node_min + (q + 0.5) * step, within \
             half a step of the position the tile was built from"
        ),
        refusals: refusals.into_iter().map(str::to_owned).collect(),
    }
}
