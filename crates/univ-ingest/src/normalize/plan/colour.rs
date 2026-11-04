//! The colour indices and the spectral type a definition reads, resolved against a header.

use univ_model::value::KindOfValue;

use super::{Col, Measured, Resolver};
use crate::normalize::NormalizeError;
use crate::source::{ColourReading, ErrorColumns};

/// A spectral type to read from a row: its column, its kind of value, and what the source
/// writes there for no type.
#[derive(Clone, Debug)]
pub(in crate::normalize) struct SpectralTypePlan {
    pub column: usize,
    pub kind: KindOfValue,
    pub none: Vec<String>,
}

/// A colour index to read from a row.
#[derive(Clone, Debug)]
pub(in crate::normalize) enum ColourPlan {
    /// Published in a column, with its error where the source gives one.
    Published { bands: [String; 2], value: Measured },
    /// The first magnitude minus the second.
    Derived { bands: [String; 2], from: [Col; 2] },
}

impl Resolver<'_> {
    pub(super) fn colour(&self, c: &ColourReading) -> Result<ColourPlan, NormalizeError> {
        let bands = c.bands.clone();
        match (&c.column, &c.from) {
            (Some(column), None) => Ok(ColourPlan::Published {
                bands,
                value: self.reading(column, c.error.clone().map(ErrorColumns::Symmetric).as_ref())?,
            }),
            (None, Some([first, second])) if c.error.is_none() => Ok(ColourPlan::Derived {
                bands,
                from: [self.col(first)?, self.col(second)?],
            }),
            _ => Err(self.unsupported(format!(
                "the colour {}-{} must name either a column, with an optional error, or the two magnitudes it is derived from",
                c.bands[0], c.bands[1]
            ))),
        }
    }

    pub(super) fn spectral_type(&self) -> Result<Option<SpectralTypePlan>, NormalizeError> {
        self.definition
            .normalization
            .spectral_type
            .as_ref()
            .map(|s| {
                Ok(SpectralTypePlan {
                    column: self.index(&s.column)?,
                    kind: s.kind_of_value,
                    none: s.none.clone(),
                })
            })
            .transpose()
    }
}
