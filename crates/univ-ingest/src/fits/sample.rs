//! The samples FITS arrays and binary table columns store, big-endian (FITS 4.0 §5.2): one
//! decoder for the image reader of `grids` and the table reader of `normalize`.

/// The type of a stored sample: an image's `BITPIX` (§4.4.1.1) or a table column's type letter
/// (§7.3.3), big-endian, `U8` unsigned and the other integers two's complement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleKind {
    U8,
    I16,
    I32,
    I64,
    F32,
    F64,
}

impl SampleKind {
    /// The type `BITPIX` names.
    #[must_use]
    pub fn from_bitpix(bitpix: i64) -> Option<Self> {
        Some(match bitpix {
            8 => Self::U8,
            16 => Self::I16,
            32 => Self::I32,
            64 => Self::I64,
            -32 => Self::F32,
            -64 => Self::F64,
            _ => return None,
        })
    }

    /// The type a binary table's `TFORMn` letter names, of those that hold one number.
    #[must_use]
    pub fn from_tform(letter: u8) -> Option<Self> {
        Some(match letter {
            b'B' => Self::U8,
            b'I' => Self::I16,
            b'J' => Self::I32,
            b'K' => Self::I64,
            b'E' => Self::F32,
            b'D' => Self::F64,
            _ => return None,
        })
    }

    /// The `BITPIX` of this type.
    #[must_use]
    pub fn bitpix(self) -> i32 {
        match self {
            Self::U8 => 8,
            Self::I16 => 16,
            Self::I32 => 32,
            Self::I64 => 64,
            Self::F32 => -32,
            Self::F64 => -64,
        }
    }

    /// The bytes of one sample.
    #[must_use]
    pub fn width(self) -> usize {
        match self {
            Self::U8 => 1,
            Self::I16 => 2,
            Self::I32 | Self::F32 => 4,
            Self::I64 | Self::F64 => 8,
        }
    }
}

/// One stored sample, in the precision it was stored with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sample {
    Integer(i64),
    Single(f32),
    Double(f64),
}

impl Sample {
    /// The sample as a float: exact, but for a 64-bit integer beyond 2^53, read as the nearest.
    #[must_use]
    pub fn to_f64(self) -> f64 {
        match self {
            #[expect(
                clippy::cast_precision_loss,
                reason = "a 64-bit integer is read as the nearest float"
            )]
            Self::Integer(i) => i as f64,
            Self::Single(x) => f64::from(x),
            Self::Double(x) => x,
        }
    }
}

/// Decodes one big-endian sample; `None` when `bytes` is not one sample's width.
#[must_use]
pub fn sample(kind: SampleKind, bytes: &[u8]) -> Option<Sample> {
    Some(match kind {
        SampleKind::U8 => Sample::Integer(i64::from(u8::from_be_bytes(bytes.try_into().ok()?))),
        SampleKind::I16 => Sample::Integer(i64::from(i16::from_be_bytes(bytes.try_into().ok()?))),
        SampleKind::I32 => Sample::Integer(i64::from(i32::from_be_bytes(bytes.try_into().ok()?))),
        SampleKind::I64 => Sample::Integer(i64::from_be_bytes(bytes.try_into().ok()?)),
        SampleKind::F32 => Sample::Single(f32::from_be_bytes(bytes.try_into().ok()?)),
        SampleKind::F64 => Sample::Double(f64::from_be_bytes(bytes.try_into().ok()?)),
    })
}
