//! A point's value in versions 2 and 3: its colour index, or in version 3 its age, and the code
//! that says which.

use serde::{Deserialize, Serialize};

use super::TileError;

/// Whether tiles of `version` carry each point's colour index: from version 2 on.
#[must_use]
pub const fn has_colour(version: u16) -> bool {
    version >= 2
}

/// The value a version 2 or 3 point holds when it has none.
pub const NO_VALUE: i16 = i16::MIN;

/// The two bands a colour index lies between.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum ColourBands {
    #[serde(rename = "BP-RP")]
    BpRp,
    #[serde(rename = "B-V")]
    BV,
}

/// The colour bands of version 2, each coded as its index plus one; 0 codes none.
pub const COLOUR_BANDS: [ColourBands; 2] = [ColourBands::BpRp, ColourBands::BV];

impl ColourBands {
    /// The bands a pair such as `["B", "V"]` names, as a source definition gives them, if
    /// version 2 codes them.
    #[must_use]
    pub fn of(bands: &[String; 2]) -> Option<Self> {
        match [bands[0].as_str(), bands[1].as_str()] {
            ["BP", "RP"] => Some(Self::BpRp),
            ["B", "V"] => Some(Self::BV),
            _ => None,
        }
    }

    /// The code version 2 gives the bands: their index in [`COLOUR_BANDS`] plus one, which
    /// [`ColourBands::of_code`] reads back (the codec's tests check every band both ways).
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::BpRp => 1,
            Self::BV => 2,
        }
    }

    /// The bands a version 2 code other than 0 gives, if [`COLOUR_BANDS`] holds them.
    #[must_use]
    pub fn of_code(code: u8) -> Option<Self> {
        COLOUR_BANDS.get(usize::from(code).checked_sub(1)?).copied()
    }
}

/// A point's colour index, in magnitudes, and the bands it lies between.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointColour {
    pub index: f64,
    pub bands: ColourBands,
}

/// What a point carries in its value's place: a colour index, or from version 3 the base-10
/// logarithm of its object's age in years.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PointValue {
    Colour(PointColour),
    LogAge(f64),
}

impl PointValue {
    /// The colour index, if the value is one.
    #[must_use]
    pub const fn colour(self) -> Option<PointColour> {
        match self {
            Self::Colour(c) => Some(c),
            Self::LogAge(_) => None,
        }
    }

    /// The log age, if the value is one.
    #[must_use]
    pub const fn log_age(self) -> Option<f64> {
        match self {
            Self::LogAge(a) => Some(a),
            Self::Colour(_) => None,
        }
    }

    /// The number held, before it is rounded to hundredths, and its code.
    pub(super) const fn coded(self) -> (f64, u8) {
        match self {
            Self::Colour(c) => (c.index, c.bands.code()),
            Self::LogAge(a) => (a, LOG_AGE_CODE),
        }
    }
}

/// A colour index, or a log age, in hundredths, if a tile can hold it.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the value is checked to lie within i16's range, above NO_VALUE, before the cast"
)]
pub(super) fn hundredths(index: f64) -> Option<i16> {
    let h = (index * 100.0).round();
    (h.is_finite() && h > f64::from(NO_VALUE) && h <= f64::from(i16::MAX)).then_some(h as i16)
}

/// Whether tiles of `version` may carry an age in place of a colour index: from version 3 on.
#[must_use]
pub const fn has_age(version: u16) -> bool {
    version >= 3
}

/// The code of a point whose value is the base-10 logarithm of its age in years, from version 3.
pub const LOG_AGE_CODE: u8 = 3;

/// A point's value as held: a colour index, or a log age, by its code; refused where the code
/// and the value disagree or the code is not one `version` defines.
pub(super) fn point_value(
    point: usize,
    version: u16,
    held: i16,
    code: u8,
) -> Result<Option<PointValue>, TileError> {
    let refused = TileError::ValueCode { point, code };
    let value = f64::from(held) / 100.0;
    match (held, code) {
        (NO_VALUE, 0) => Ok(None),
        (NO_VALUE, _) | (_, 0) => Err(refused),
        (_, LOG_AGE_CODE) if has_age(version) => Ok(Some(PointValue::LogAge(value))),
        (_, c) => ColourBands::of_code(c)
            .map(|bands| {
                Some(PointValue::Colour(PointColour {
                    index: value,
                    bands,
                }))
            })
            .ok_or(refused),
    }
}
