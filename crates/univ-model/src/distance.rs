//! Distances as published, with their kind, method and the Hubble constant they assume.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::units::{Dimension, Unit};
use crate::value::{KindOfValue, Quantity, Uncertainty, ValueError, positive};

/// How a distance was obtained, from the most direct to the most model-dependent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DistanceKind {
    Parallax,
    Geometric,
    Photogeometric,
    PrimaryIndicator,
    SecondaryIndicator,
    SpectroscopicRedshift,
    PhotometricRedshift,
}

/// The indicator or technique behind a distance.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    TrigonometricParallax,
    /// Bailer-Jones et al. 2021, geometric prior.
    BayesianGeometric,
    /// Bailer-Jones et al. 2021, photogeometric prior.
    BayesianPhotogeometric,
    Cepheids,
    TipOfTheRedGiantBranch,
    Masers,
    RrLyrae,
    HorizontalBranch,
    TypeIaSupernovae,
    TypeIiSupernovae,
    SurfaceBrightnessFluctuations,
    TullyFisher,
    FundamentalPlane,
    Redshift,
    /// A distance its source combined from several indicators, as its source lists them, such
    /// as a Cosmicflows-4 group distance.
    Mixed(Vec<Method>),
    /// An indicator named by its source that has no variant here.
    Other(String),
}

/// A distance as its source published it, beside its normalized value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawDistance")]
pub struct Distance {
    /// The value the normalized distance comes from, as published: a length, a length in h⁻¹
    /// Mpc, or a distance modulus.
    pub native: Quantity,
    /// The other form its source published beside `native`, kept and not used: such as the
    /// length `McConnachie` 2012 gives beside each distance modulus.
    pub also_published: Option<Quantity>,
    /// The distance in parsecs, for a star, or megaparsecs.
    pub normalized: Quantity,
    pub kind_of_value: KindOfValue,
    pub kind: DistanceKind,
    pub method: Method,
    /// The Hubble constant in km/s/Mpc that the value was published for, where it depends on
    /// one: 100 for a length in h⁻¹ Mpc.
    pub hubble_constant: Option<f64>,
}

/// A distance as read, before its checks.
#[derive(Deserialize)]
struct RawDistance {
    native: Quantity,
    also_published: Option<Quantity>,
    normalized: Quantity,
    kind_of_value: KindOfValue,
    kind: DistanceKind,
    method: Method,
    hubble_constant: Option<f64>,
}

impl TryFrom<RawDistance> for Distance {
    type Error = DistanceError;
    fn try_from(raw: RawDistance) -> Result<Self, DistanceError> {
        let distance = Self::new(
            raw.native,
            raw.also_published,
            raw.normalized.unit,
            raw.kind_of_value,
            (raw.kind, raw.method),
            raw.hubble_constant,
        )?;
        let (ours, theirs) = (distance.normalized.value, raw.normalized.value);
        if (ours - theirs).abs() > 1e-9 * ours.abs().max(theirs.abs()) {
            return Err(DistanceError::Inconsistent {
                normalized: theirs,
                derived: ours,
            });
        }
        Ok(distance)
    }
}

/// A Hubble-scaled length is the length for H0 = 100 h km/s/Mpc.
const HUBBLE_SCALED: f64 = 100.0;

impl Distance {
    /// A distance from its value as published, normalized to `unit`: a modulus through
    /// [`from_modulus`], a length in h⁻¹ Mpc as the length for H0 = 100 km/s/Mpc, which it then
    /// records, and a length as it is.
    ///
    /// # Errors
    ///
    /// Fails when `unit` is neither parsecs nor megaparsecs, `native` is not a length, a
    /// Hubble-scaled length or a modulus, `also_published` is neither a length nor a modulus,
    /// a mixed method lists fewer than two methods, or the Hubble constant is not positive or,
    /// for a Hubble-scaled length, not 100.
    pub fn new(
        native: Quantity,
        also_published: Option<Quantity>,
        unit: Unit,
        kind_of_value: KindOfValue,
        (kind, method): (DistanceKind, Method),
        hubble_constant: Option<f64>,
    ) -> Result<Self, DistanceError> {
        if !matches!(unit, Unit::Parsec | Unit::Megaparsec) {
            return Err(DistanceError::NormalizedUnit(unit));
        }
        if let Some(h) = hubble_constant {
            checked(h)?;
        }
        if let Method::Mixed(methods) = &method
            && methods.len() < 2
        {
            return Err(DistanceError::MixedOfOne);
        }
        if let Some(other) = also_published
            && !matches!(
                other.unit.dimension(),
                Dimension::Length | Dimension::Magnitude
            )
        {
            return Err(DistanceError::NotADistance(other.unit));
        }
        let (normalized, hubble_constant) = match native.unit.dimension() {
            Dimension::Magnitude => (from_modulus(&native)?.to(unit)?, hubble_constant),
            Dimension::Length => (native.to(unit)?, hubble_constant),
            Dimension::HubbleScaledLength => {
                if hubble_constant.is_some_and(|h| h != HUBBLE_SCALED) {
                    return Err(DistanceError::WrongUnit {
                        expected: Unit::Megaparsec,
                        found: native.unit,
                    });
                }
                let mpc = Quantity::new(native.value, Unit::Megaparsec, native.uncertainty)?;
                (mpc.to(unit)?, Some(HUBBLE_SCALED))
            }
            _ => return Err(DistanceError::NotADistance(native.unit)),
        };
        Ok(Self {
            native,
            also_published,
            normalized,
            kind_of_value,
            kind,
            method,
            hubble_constant,
        })
    }

    /// The distance modulus as published, whether it is the native value or published beside
    /// a native length.
    #[must_use]
    pub fn modulus(&self) -> Option<&Quantity> {
        [Some(&self.native), self.also_published.as_ref()]
            .into_iter()
            .flatten()
            .find(|q| q.unit == Unit::Magnitude)
    }
}

/// A distance that cannot be derived.
#[derive(Debug, Error, PartialEq)]
pub enum DistanceError {
    #[error("expected a value in {expected:?}, found {found:?}")]
    WrongUnit { expected: Unit, found: Unit },
    #[error("{0:?} is not a unit of length, so it does not scale with the Hubble constant")]
    NotALength(Unit),
    #[error("{0:?} is neither a length nor a distance modulus")]
    NotADistance(Unit),
    #[error("a distance is normalized to parsecs or megaparsecs, not {0:?}")]
    NormalizedUnit(Unit),
    #[error("a mixed method lists at least two methods")]
    MixedOfOne,
    #[error("the normalized distance {normalized} is not the {derived} its native value gives")]
    Inconsistent { normalized: f64, derived: f64 },
    #[error("the Hubble constant {0} km/s/Mpc is not positive")]
    BadHubbleConstant(f64),
    #[error(transparent)]
    Value(#[from] ValueError),
}

/// The distance in parsecs that a distance modulus `μ` stands for, `10^(μ/5 + 1)`, with its
/// uncertainty carried through. The modulus itself is not changed; keep it beside the result.
///
/// # Errors
///
/// Fails when `modulus` is not in magnitudes.
pub fn from_modulus(modulus: &Quantity) -> Result<Quantity, DistanceError> {
    if modulus.unit != Unit::Magnitude {
        return Err(DistanceError::WrongUnit {
            expected: Unit::Magnitude,
            found: modulus.unit,
        });
    }
    let parsecs = |mu: f64| 10f64.powf(mu / 5.0 + 1.0);
    let distance = parsecs(modulus.value);
    let uncertainty = match modulus.uncertainty {
        Uncertainty::Symmetric { sigma } => Uncertainty::Symmetric {
            sigma: distance * std::f64::consts::LN_10 / 5.0 * sigma,
        },
        Uncertainty::Asymmetric { minus, plus } => Uncertainty::Asymmetric {
            minus: distance - parsecs(modulus.value - minus),
            plus: parsecs(modulus.value + plus) - distance,
        },
        // Quantiles of a posterior map through a monotonic function unchanged (Bailer-Jones et
        // al. 2021, abstract), so the interval's bounds are converted, not its widths.
        Uncertainty::CredibleInterval {
            minus,
            plus,
            probability,
        } => Uncertainty::CredibleInterval {
            minus: distance - parsecs(modulus.value - minus),
            plus: parsecs(modulus.value + plus) - distance,
            probability,
        },
        other => other,
    };
    Ok(Quantity::new(distance, Unit::Parsec, uncertainty)?)
}

/// A length in megaparsecs from one in h⁻¹ Mpc, for the Hubble constant `hubble_constant`.
///
/// # Errors
///
/// Fails when `scaled` is not in h⁻¹ Mpc or the Hubble constant is not positive.
pub fn from_hubble_scaled(
    scaled: &Quantity,
    hubble_constant: f64,
) -> Result<Quantity, DistanceError> {
    if scaled.unit != Unit::MegaparsecOverH {
        return Err(DistanceError::WrongUnit {
            expected: Unit::MegaparsecOverH,
            found: scaled.unit,
        });
    }
    let h = checked(hubble_constant)? / 100.0;
    let uncertainty = scaled.uncertainty.scaled(1.0 / h);
    Ok(Quantity::new(
        scaled.value / h,
        Unit::Megaparsec,
        uncertainty,
    )?)
}

/// A length published for the Hubble constant `published` rescaled to `target`: such a length
/// scales as 1/H0, so the value is multiplied by `published / target`.
///
/// # Errors
///
/// Fails when `length` is not a length, such as a modulus, a parallax or a value in h⁻¹ Mpc,
/// or either Hubble constant is not positive.
pub fn rescale_hubble(
    length: &Quantity,
    published: f64,
    target: f64,
) -> Result<Quantity, DistanceError> {
    if length.unit.dimension() != Dimension::Length {
        return Err(DistanceError::NotALength(length.unit));
    }
    let factor = checked(published)? / checked(target)?;
    Ok(Quantity::new(
        length.value * factor,
        length.unit,
        length.uncertainty.scaled(factor),
    )?)
}

fn checked(hubble_constant: f64) -> Result<f64, DistanceError> {
    if positive(hubble_constant) {
        Ok(hubble_constant)
    } else {
        Err(DistanceError::BadHubbleConstant(hubble_constant))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quantity(value: f64, unit: Unit, uncertainty: Uncertainty) -> Quantity {
        Quantity::new(value, unit, uncertainty).unwrap()
    }

    #[test]
    fn a_modulus_of_zero_is_ten_parsecs_and_its_error_is_carried() {
        let mu = quantity(0.0, Unit::Magnitude, Uncertainty::Symmetric { sigma: 0.1 });
        let d = from_modulus(&mu).unwrap();
        assert!((d.value - 10.0).abs() < 1e-12);
        let Uncertainty::Symmetric { sigma } = d.uncertainty else {
            panic!("symmetric")
        };
        assert!((sigma - 10.0 * std::f64::consts::LN_10 / 5.0 * 0.1).abs() < 1e-12);
    }

    #[test]
    fn m31s_modulus_gives_its_distance() {
        // McConnachie 2012 gives (m - M)0 = 24.47 for M31, 783 kpc.
        let mu = quantity(
            24.47,
            Unit::Magnitude,
            Uncertainty::Asymmetric {
                minus: 0.07,
                plus: 0.07,
            },
        );
        let d = from_modulus(&mu).unwrap().to(Unit::Kiloparsec).unwrap();
        assert!((d.value - 783.0).abs() < 1.0);
        let Uncertainty::Asymmetric { minus, plus } = d.uncertainty else {
            panic!("asymmetric")
        };
        assert!(
            minus < plus,
            "the error in distance is wider on the far side"
        );
    }

    #[test]
    fn a_hubble_scaled_length_becomes_megaparsecs_for_a_chosen_h() {
        let scaled = quantity(
            100.0,
            Unit::MegaparsecOverH,
            Uncertainty::Symmetric { sigma: 5.0 },
        );
        let mpc = from_hubble_scaled(&scaled, 67.66).unwrap();
        assert!((mpc.value - 100.0 / 0.6766).abs() < 1e-9);
        assert_eq!(mpc.unit, Unit::Megaparsec);
        assert!(from_hubble_scaled(&scaled, 0.0).is_err());
        let plain = quantity(1.0, Unit::Megaparsec, Uncertainty::NoneReported);
        assert!(from_hubble_scaled(&plain, 70.0).is_err());
    }

    #[test]
    fn rescaling_to_another_hubble_constant_scales_as_its_inverse_and_back() {
        let at_75 = quantity(
            40.0,
            Unit::Megaparsec,
            Uncertainty::Symmetric { sigma: 4.0 },
        );
        let at_planck = rescale_hubble(&at_75, 75.0, 67.66).unwrap();
        assert!((at_planck.value - 40.0 * 75.0 / 67.66).abs() < 1e-12);
        let back = rescale_hubble(&at_planck, 67.66, 75.0).unwrap();
        assert!((back.value - 40.0).abs() < 1e-12);
        assert!(rescale_hubble(&at_75, -1.0, 67.66).is_err());
        let modulus = quantity(33.0, Unit::Magnitude, Uncertainty::NoneReported);
        let refused = rescale_hubble(&modulus, 75.0, 67.66);
        assert_eq!(refused, Err(DistanceError::NotALength(Unit::Magnitude)));
    }

    fn measured(native: Quantity, unit: Unit, h: Option<f64>) -> Result<Distance, DistanceError> {
        let how = (DistanceKind::SecondaryIndicator, Method::TullyFisher);
        Distance::new(native, None, unit, KindOfValue::Measured, how, h)
    }

    #[test]
    fn a_modulus_is_kept_as_published_beside_the_distance_derived_from_it() {
        let mu = quantity(30.0, Unit::Magnitude, Uncertainty::Symmetric { sigma: 0.2 });
        let d = measured(mu, Unit::Megaparsec, None).unwrap();
        assert_eq!(d.native, mu);
        assert_eq!(d.modulus(), Some(&mu));
        assert!(
            (d.normalized.value - 10.0).abs() < 1e-12,
            "10 Mpc at μ = 30"
        );
        assert_eq!(d.normalized.unit, Unit::Megaparsec);
    }

    #[test]
    fn a_hubble_scaled_length_keeps_h0_100_and_any_other_constant_is_refused() {
        let scaled = quantity(50.0, Unit::MegaparsecOverH, Uncertainty::NoneReported);
        let d = measured(scaled, Unit::Megaparsec, None).unwrap();
        assert_eq!(d.hubble_constant, Some(100.0));
        assert_eq!(d.normalized.value, 50.0);
        assert!(measured(scaled, Unit::Megaparsec, Some(73.0)).is_err());
        let at_73 = quantity(8.0, Unit::Megaparsec, Uncertainty::NoneReported);
        assert_eq!(
            measured(at_73, Unit::Megaparsec, Some(73.0))
                .unwrap()
                .hubble_constant,
            Some(73.0)
        );
    }

    #[test]
    fn a_distance_is_normalized_to_parsecs_or_megaparsecs_only() {
        let d = quantity(0.097, Unit::Kiloparsec, Uncertainty::NoneReported);
        assert_eq!(
            measured(d, Unit::Parsec, None).unwrap().normalized.value,
            97.0
        );
        assert_eq!(
            measured(d, Unit::Kiloparsec, None),
            Err(DistanceError::NormalizedUnit(Unit::Kiloparsec))
        );
        let parallax = quantity(10.0, Unit::Milliarcsecond, Uncertainty::NoneReported);
        assert_eq!(
            measured(parallax, Unit::Parsec, None),
            Err(DistanceError::NotADistance(Unit::Milliarcsecond))
        );
        let how = (
            DistanceKind::SecondaryIndicator,
            Method::Mixed(vec![Method::TullyFisher]),
        );
        let one = Distance::new(d, None, Unit::Parsec, KindOfValue::Measured, how, None);
        assert_eq!(one, Err(DistanceError::MixedOfOne));
    }

    #[test]
    fn a_credible_interval_on_a_modulus_converts_its_bounds() {
        let interval = Uncertainty::CredibleInterval {
            minus: 0.1,
            plus: 0.1,
            probability: 0.68,
        };
        let d = from_modulus(&quantity(10.0, Unit::Magnitude, interval)).unwrap();
        let Uncertainty::CredibleInterval { minus, plus, .. } = d.uncertainty else {
            panic!("an interval")
        };
        assert!((d.value - minus - 10f64.powf(2.98)).abs() < 1e-9);
        assert!((d.value + plus - 10f64.powf(3.02)).abs() < 1e-9);
    }

    #[test]
    fn a_distance_read_back_is_checked_against_its_native_value() {
        let mu = quantity(30.0, Unit::Magnitude, Uncertainty::NoneReported);
        let d = measured(mu, Unit::Megaparsec, None).unwrap();
        let yaml = serde_norway::to_string(&d).unwrap();
        assert_eq!(serde_norway::from_str::<Distance>(&yaml).unwrap(), d);
        let mut tampered = d;
        tampered.normalized.value = 11.0;
        let yaml = serde_norway::to_string(&tampered).unwrap();
        assert!(serde_norway::from_str::<Distance>(&yaml).is_err(), "{yaml}");
    }
}
