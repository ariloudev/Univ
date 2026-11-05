//! A row's colour indices and spectral type.

use thin_vec::ThinVec;
use univ_model::entry::{ColourIndex, SpectralType};
use univ_model::units::Unit;
use univ_model::value::{KindOfValue, Quantity, Uncertainty, Value};

use super::{Row, fitted, value_reason};
use crate::normalize::plan::ColourPlan;
use crate::source::LeftOutReason;

impl Row<'_, '_> {
    /// The colour indices the plan reads, each where its cells are not blank.
    pub(super) fn colours(&mut self) -> Result<ThinVec<ColourIndex>, LeftOutReason> {
        let mut colours = ThinVec::with_capacity(self.cx.plan.colours.len());
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
                    match first_value.zip(second_value) {
                        None => None,
                        Some((a, b)) => {
                            // A difference of two magnitudes, neither of which gives its error
                            // here, of the weaker kind of the two.
                            let difference = Quantity::new(
                                a.normalized.value - b.normalized.value,
                                Unit::Magnitude,
                                Uncertainty::NoneReported,
                            )
                            .and_then(|q| {
                                Value::normalize(q, Unit::Magnitude, weaker(a.kind, b.kind))
                            })
                            .map_err(|e| value_reason(&e))?;
                            Some(ColourIndex {
                                bands: bands.clone(),
                                value: difference,
                                derived_from: Some([first.name.clone(), second.name.clone()]),
                            })
                        }
                    }
                }
            };
            colours.extend(colour);
        }
        Ok(fitted(colours))
    }

    /// The spectral type the plan reads, as published, where its cell is not blank and not
    /// what the source writes for no type.
    pub(super) fn spectral_type(&self) -> Option<SpectralType> {
        let plan = self.cx.plan.spectral_type.as_ref()?;
        self.text(plan.column)
            .filter(|text| !plan.none.contains(text))
            .map(|text| SpectralType {
                text,
                kind: plan.kind,
            })
    }
}

/// The weaker of two kinds of value: the later in measured, estimated, inferred,
/// reconstructed.
fn weaker(a: KindOfValue, b: KindOfValue) -> KindOfValue {
    let rank = |k| match k {
        KindOfValue::Measured => 0,
        KindOfValue::Estimated => 1,
        KindOfValue::Inferred => 2,
        KindOfValue::Reconstructed => 3,
    };
    if rank(b) > rank(a) { b } else { a }
}
