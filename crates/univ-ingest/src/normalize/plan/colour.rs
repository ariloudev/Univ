//! The colour indices and the spectral type a definition reads, resolved against a header.

use univ_model::value::KindOfValue;

use super::{Col, Errors, Measured, Resolver};
use crate::normalize::NormalizeError;
use crate::source::ColourReading;

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
                value: Measured {
                    value: self.col(column)?,
                    error: c.error.as_deref().map(|e| self.col(e)).transpose()?.map(Errors::Symmetric),
                },
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

    pub(super) fn spectral_type(&self) -> Result<Option<(usize, KindOfValue)>, NormalizeError> {
        self.definition
            .normalization
            .spectral_type
            .as_ref()
            .map(|s| Ok((self.col(&s.column)?.index, s.kind_of_value)))
            .transpose()
    }
}
