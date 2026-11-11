//! The ages a definition reads, resolved against a header.

use univ_model::value::KindOfValue;

use super::{Measured, Resolver};
use crate::normalize::NormalizeError;
use crate::source::AgeReading;

/// An age to read from a row: its value and error columns, and its kind of value.
#[derive(Clone, Debug)]
pub(in crate::normalize) struct AgePlan {
    pub value: Measured,
    pub kind: KindOfValue,
}

impl Resolver<'_> {
    pub(super) fn age(&self, a: &AgeReading) -> Result<AgePlan, NormalizeError> {
        Ok(AgePlan {
            value: self.reading(&a.value, a.error.as_ref())?,
            kind: a.kind_of_value,
        })
    }
}
