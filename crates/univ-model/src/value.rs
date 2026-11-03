//! Values with their unit, typed uncertainty and kind of value, kept as published beside their
//! normalized form.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::units::{Unit, UnitError};

/// Whether a value was measured, estimated, inferred by analysis or reconstructed by a model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KindOfValue {
    Measured,
    Estimated,
    Inferred,
    Reconstructed,
}

/// The uncertainty of a value, in the form its source published it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "form")]
pub enum Uncertainty {
    /// The source published no uncertainty: not the same as an uncertainty of zero.
    NoneReported,
    /// A symmetric error, ±`sigma`.
    Symmetric { sigma: f64 },
    /// An asymmetric interval: the value is `value - minus` to `value + plus`.
    Asymmetric { minus: f64, plus: f64 },
    /// The value is an upper limit.
    UpperLimit,
    /// The value is a lower limit.
    LowerLimit,
    /// The value lies in a credible region of this probability, described by its geometry.
    CredibleRegion { probability: f64 },
}

/// A value, an uncertainty or a unit that cannot stand.
#[derive(Debug, Error, PartialEq)]
pub enum ValueError {
    #[error("the value {0} is not a finite number")]
    NotFinite(f64),
    #[error("the uncertainty {0} is negative or not a finite number")]
    BadUncertainty(f64),
    #[error("the credible region's probability {0} lies outside (0, 1]")]
    BadProbability(f64),
    #[error(transparent)]
    Unit(#[from] UnitError),
}

impl Uncertainty {
    /// The same uncertainty for the value multiplied by a positive `factor`.
    #[must_use]
    pub fn scaled(self, factor: f64) -> Self {
        match self {
            Self::Symmetric { sigma } => Self::Symmetric {
                sigma: sigma * factor,
            },
            Self::Asymmetric { minus, plus } => Self::Asymmetric {
                minus: minus * factor,
                plus: plus * factor,
            },
            other => other,
        }
    }

    fn check(self) -> Result<(), ValueError> {
        let error = |x: f64| x.is_finite() && x >= 0.0;
        match self {
            Self::Symmetric { sigma } if !error(sigma) => Err(ValueError::BadUncertainty(sigma)),
            Self::Asymmetric { minus, .. } if !error(minus) => {
                Err(ValueError::BadUncertainty(minus))
            }
            Self::Asymmetric { plus, .. } if !error(plus) => Err(ValueError::BadUncertainty(plus)),
            Self::CredibleRegion { probability } if !(probability > 0.0 && probability <= 1.0) => {
                Err(ValueError::BadProbability(probability))
            }
            _ => Ok(()),
        }
    }
}

/// A finite number above zero.
#[must_use]
pub fn positive(x: f64) -> bool {
    x.is_finite() && x > 0.0
}

/// A number with its unit and uncertainty.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawQuantity")]
pub struct Quantity {
    pub value: f64,
    pub unit: Unit,
    pub uncertainty: Uncertainty,
}

impl Quantity {
    /// A quantity, checked: the value is finite and the uncertainty well formed.
    ///
    /// # Errors
    ///
    /// Fails on a value that is not finite or an uncertainty that cannot stand.
    pub fn new(value: f64, unit: Unit, uncertainty: Uncertainty) -> Result<Self, ValueError> {
        if !value.is_finite() {
            return Err(ValueError::NotFinite(value));
        }
        uncertainty.check()?;
        Ok(Self {
            value,
            unit,
            uncertainty,
        })
    }

    /// The same quantity in another unit of its dimension, its uncertainty scaled with it.
    ///
    /// # Errors
    ///
    /// Fails when `unit` measures another dimension.
    pub fn to(&self, unit: Unit) -> Result<Self, ValueError> {
        let factor = self.unit.factor_to(unit)?;
        Ok(Self {
            value: self.value * factor,
            unit,
            uncertainty: self.uncertainty.scaled(factor),
        })
    }
}

/// A quantity as read, before its checks.
#[derive(Deserialize)]
struct RawQuantity {
    value: f64,
    unit: Unit,
    uncertainty: Uncertainty,
}

impl TryFrom<RawQuantity> for Quantity {
    type Error = ValueError;
    fn try_from(raw: RawQuantity) -> Result<Self, ValueError> {
        Self::new(raw.value, raw.unit, raw.uncertainty)
    }
}

/// A value as its source published it, beside its normalized form, with its kind of value.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Value {
    pub native: Quantity,
    pub normalized: Quantity,
    pub kind: KindOfValue,
}

impl Value {
    /// Keeps `native` as published and normalizes it to `unit`.
    ///
    /// # Errors
    ///
    /// Fails when `unit` measures another dimension than `native`.
    pub fn normalize(native: Quantity, unit: Unit, kind: KindOfValue) -> Result<Self, ValueError> {
        Ok(Self {
            native,
            normalized: native.to(unit)?,
            kind,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizing_keeps_the_native_value_and_scales_its_uncertainty() {
        let native = Quantity::new(
            15.0,
            Unit::Kiloparsec,
            Uncertainty::Symmetric { sigma: 2.0 },
        )
        .unwrap();
        let value = Value::normalize(native, Unit::Parsec, KindOfValue::Measured).unwrap();
        assert_eq!(value.native, native);
        assert_eq!(value.normalized.value, 15_000.0);
        assert_eq!(
            value.normalized.uncertainty,
            Uncertainty::Symmetric { sigma: 2_000.0 }
        );
        assert_eq!(value.kind, KindOfValue::Measured);
    }

    #[test]
    fn an_uncertainty_none_reported_stays_none_reported_never_zero() {
        let native = Quantity::new(0.78, Unit::Megaparsec, Uncertainty::NoneReported).unwrap();
        let value = Value::normalize(native, Unit::Parsec, KindOfValue::Measured).unwrap();
        assert_eq!(value.normalized.uncertainty, Uncertainty::NoneReported);
    }

    #[test]
    fn asymmetric_errors_and_limits_survive_conversion() {
        let asymmetric = Uncertainty::Asymmetric {
            minus: 1.0,
            plus: 3.0,
        };
        let q = Quantity::new(10.0, Unit::Megaparsec, asymmetric).unwrap();
        let in_kpc = q.to(Unit::Kiloparsec).unwrap();
        assert_eq!(
            in_kpc.uncertainty,
            Uncertainty::Asymmetric {
                minus: 1_000.0,
                plus: 3_000.0
            }
        );
        let limit = Quantity::new(5.0, Unit::Arcsecond, Uncertainty::UpperLimit).unwrap();
        assert_eq!(
            limit.to(Unit::Degree).unwrap().uncertainty,
            Uncertainty::UpperLimit
        );
    }

    #[test]
    fn malformed_values_are_refused() {
        let none = Uncertainty::NoneReported;
        assert_eq!(
            Quantity::new(f64::NAN, Unit::Degree, none)
                .unwrap_err()
                .to_string(),
            { "the value NaN is not a finite number" }
        );
        let negative = Uncertainty::Symmetric { sigma: -1.0 };
        assert!(Quantity::new(1.0, Unit::Degree, negative).is_err());
        let region = Uncertainty::CredibleRegion { probability: 1.5 };
        assert!(Quantity::new(1.0, Unit::Degree, region).is_err());
        let q = Quantity::new(1.0, Unit::Degree, none).unwrap();
        assert!(Value::normalize(q, Unit::Parsec, KindOfValue::Measured).is_err());
    }
}
