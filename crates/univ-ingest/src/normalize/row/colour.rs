//! A row's colour indices and spectral type.

use univ_model::entry::{ColourIndex, SpectralType};
use univ_model::units::Unit;
use univ_model::value::{Quantity, Uncertainty, Value};

use super::Row;
use crate::normalize::plan::ColourPlan;
use crate::source::LeftOutReason;

impl Row<'_, '_> {
    /// The colour indices the plan reads, each where its cells are not blank.
    pub(super) fn colours(&mut self) -> Result<Vec<ColourIndex>, LeftOutReason> {
        let mut colours = Vec::new();
        for plan in &self.cx.plan.colours {
            let colour = match plan {
                ColourPlan::Published { bands, value } => self
                    .value(Some(value), Unit::Magnitude)?
                    .map(|value| ColourIndex {
                        bands: bands.clone(),
                        value,
                        derived_from: None,
                    }),
                ColourPlan::Derived {
                    bands,
                    from: [first, second],
                } => {
                    let first_value = self.plain(first, Unit::Magnitude)?;
                    let second_value = self.plain(second, Unit::Magnitude)?;
                    first_value.zip(second_value).map(|(a, b)| {
                        // A difference of two magnitudes, neither of which gives its error here.
                        let native = Quantity {
                            value: a.normalized.value - b.normalized.value,
                            unit: Unit::Magnitude,
                            uncertainty: Uncertainty::NoneReported,
                        };
                        ColourIndex {
                            bands: bands.clone(),
                            value: Value {
                                native,
                                normalized: native,
                                kind: a.kind,
                            },
                            derived_from: Some([first.name.clone(), second.name.clone()]),
                        }
                    })
                }
            };
            colours.extend(colour);
        }
        Ok(colours)
    }

    /// The spectral type the plan reads, as published, where its cell is not blank.
    pub(super) fn spectral_type(&self) -> Option<SpectralType> {
        let (column, kind) = self.cx.plan.spectral_type?;
        self.text(column).map(|text| SpectralType { text, kind })
    }
}
