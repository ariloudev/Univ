//! Distances as published, with their kind, method and the Hubble constant they assume.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::units::Unit;
use crate::value::{Quantity, Uncertainty, ValueError};

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
    TypeIaSupernovae,
    TypeIiSupernovae,
    SurfaceBrightnessFluctuations,
    TullyFisher,
    FundamentalPlane,
    Redshift,
    /// An indicator named by its source that has no variant here.
    Other(String),
}

/// A distance as its source published it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Distance {
    /// The value as published: a parallax, a distance modulus, a length or a length in h⁻¹ Mpc.
    pub native: Quantity,
    pub kind: DistanceKind,
    pub method: Method,
    /// The Hubble constant in km/s/Mpc that the value was published for, where it depends on one.
    pub hubble_constant: Option<f64>,
}

/// A distance that cannot be derived.
#[derive(Debug, Error, PartialEq)]
pub enum DistanceError {
    #[error("expected a value in {expected:?}, found {found:?}")]
    WrongUnit { expected: Unit, found: Unit },
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
/// Fails when either Hubble constant is not positive.
pub fn rescale_hubble(
    length: &Quantity,
    published: f64,
    target: f64,
) -> Result<Quantity, DistanceError> {
    let factor = checked(published)? / checked(target)?;
    Ok(Quantity::new(
        length.value * factor,
        length.unit,
        length.uncertainty.scaled(factor),
    )?)
}

fn checked(hubble_constant: f64) -> Result<f64, DistanceError> {
    if hubble_constant.is_finite() && hubble_constant > 0.0 {
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
    }
}
