//! What a source publishes of a star cluster's age.

use serde::{Deserialize, Serialize};

use crate::units::Unit;
use crate::value::{KindOfValue, Quantity, Uncertainty, Value, ValueError};

/// An age, normalized to years, its native value as the source publishes it, such as the
/// base-10 logarithm of the age in years Hunt & Reffert 2023 give.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Age {
    pub value: Value,
}

impl Age {
    /// Keeps `native` as published and normalizes it to years: a logarithm by a power of ten,
    /// its bounds each raised with it, so that a symmetric error in the logarithm becomes an
    /// asymmetric one in years; any other time by its factor.
    ///
    /// # Errors
    ///
    /// Fails when `native` is not a time, or its age in years is not finite.
    pub fn normalize(native: Quantity, kind: KindOfValue) -> Result<Self, ValueError> {
        if native.unit != Unit::LogYear {
            return Ok(Self {
                value: Value::normalize(native, Unit::Year, kind)?,
            });
        }
        let x = native.value;
        let years = 10_f64.powf(x);
        let below = |minus: f64| years - 10_f64.powf(x - minus);
        let above = |plus: f64| 10_f64.powf(x + plus) - years;
        let uncertainty = match native.uncertainty {
            Uncertainty::Symmetric { sigma } => Uncertainty::Asymmetric {
                minus: below(sigma),
                plus: above(sigma),
            },
            Uncertainty::Asymmetric { minus, plus } => Uncertainty::Asymmetric {
                minus: below(minus),
                plus: above(plus),
            },
            Uncertainty::Interval { minus, plus } => Uncertainty::Interval {
                minus: below(minus),
                plus: above(plus),
            },
            Uncertainty::CredibleInterval {
                minus,
                plus,
                probability,
            } => Uncertainty::CredibleInterval {
                minus: below(minus),
                plus: above(plus),
                probability,
            },
            other => other,
        };
        Ok(Self {
            value: Value {
                native,
                normalized: Quantity::new(years, Unit::Year, uncertainty)?,
                kind,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_log_age_is_kept_and_normalized_to_years_with_its_interval_raised() {
        // logAge16, 50 and 84 of 7.0, 7.3 and 7.5: a credible interval of −0.3/+0.2 in the log.
        let interval = Uncertainty::CredibleInterval {
            minus: 0.3,
            plus: 0.2,
            probability: 0.68,
        };
        let native = Quantity::new(7.3, Unit::LogYear, interval).unwrap();
        let age = Age::normalize(native, KindOfValue::Estimated).unwrap();
        assert_eq!(age.value.native, native);
        assert_eq!(age.value.kind, KindOfValue::Estimated);
        let n = age.value.normalized;
        assert_eq!(n.unit, Unit::Year);
        assert!((n.value - 10_f64.powf(7.3)).abs() < 1e-3);
        let Uncertainty::CredibleInterval {
            minus,
            plus,
            probability,
        } = n.uncertainty
        else {
            panic!("not a credible interval: {:?}", n.uncertainty)
        };
        assert!((minus - (10_f64.powf(7.3) - 1e7)).abs() < 1e-3);
        assert!((plus - (10_f64.powf(7.5) - 10_f64.powf(7.3))).abs() < 1e-3);
        assert_eq!(probability, 0.68);
    }

    #[test]
    fn a_symmetric_log_error_becomes_asymmetric_in_years_and_none_reported_stays_none() {
        let native =
            Quantity::new(8.0, Unit::LogYear, Uncertainty::Symmetric { sigma: 0.1 }).unwrap();
        let n = Age::normalize(native, KindOfValue::Estimated)
            .unwrap()
            .value
            .normalized;
        let Uncertainty::Asymmetric { minus, plus } = n.uncertainty else {
            panic!("not asymmetric: {:?}", n.uncertainty)
        };
        assert!(plus > minus);
        let bare = Quantity::new(8.0, Unit::LogYear, Uncertainty::NoneReported).unwrap();
        let n = Age::normalize(bare, KindOfValue::Estimated)
            .unwrap()
            .value
            .normalized;
        assert_eq!(n.uncertainty, Uncertainty::NoneReported);
    }

    #[test]
    fn an_age_in_years_is_kept_and_a_length_is_refused() {
        let years = Quantity::new(2.5e7, Unit::Year, Uncertainty::NoneReported).unwrap();
        let age = Age::normalize(years, KindOfValue::Estimated).unwrap();
        assert_eq!(age.value.normalized.value, 2.5e7);
        let parsecs = Quantity::new(2.5e7, Unit::Parsec, Uncertainty::NoneReported).unwrap();
        assert!(Age::normalize(parsecs, KindOfValue::Estimated).is_err());
    }
}
