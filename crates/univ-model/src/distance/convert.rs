//! Conversions of published distances: a modulus to a length, a length in h⁻¹ Mpc to one for a
//! chosen Hubble constant, and a length from one Hubble constant to another.

use crate::units::{Dimension, Unit};
use crate::value::{Quantity, Uncertainty, positive};

use super::DistanceError;

/// The distance in parsecs that a distance modulus `μ` stands for, `10^(μ/5 + 1)`, with its
/// uncertainty carried through exactly: a symmetric error in μ becomes an asymmetric interval.
/// The modulus itself is not changed; keep it beside the result.
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
        // A symmetric error in μ is an asymmetric one in distance: μ ± σ spans
        // d·10^(−σ/5) to d·10^(+σ/5) exactly, wider on the far side.
        Uncertainty::Symmetric { sigma } => Uncertainty::Asymmetric {
            minus: distance - parsecs(modulus.value - sigma),
            plus: parsecs(modulus.value + sigma) - distance,
        },
        Uncertainty::Asymmetric { minus, plus } => Uncertainty::Asymmetric {
            minus: distance - parsecs(modulus.value - minus),
            plus: parsecs(modulus.value + plus) - distance,
        },
        // A stated interval's bounds map through the monotonic function as bounds.
        Uncertainty::Interval { minus, plus } => Uncertainty::Interval {
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

pub(super) fn checked(hubble_constant: f64) -> Result<f64, DistanceError> {
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
        let Uncertainty::Asymmetric { minus, plus } = d.uncertainty else {
            panic!("asymmetric")
        };
        assert!((minus - (10.0 - 10f64.powf(0.98))).abs() < 1e-12);
        assert!((plus - (10f64.powf(1.02) - 10.0)).abs() < 1e-12);
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
    fn a_symmetric_modulus_error_maps_to_the_exact_asymmetric_distance_interval() {
        for sigma in [2.11, 0.4] {
            let mu = quantity(35.0, Unit::Magnitude, Uncertainty::Symmetric { sigma });
            let d = from_modulus(&mu).unwrap();
            let Uncertainty::Asymmetric { minus, plus } = d.uncertainty else {
                panic!("σ = {sigma}: asymmetric, not {:?}", d.uncertainty)
            };
            let (low, high) = (
                d.value * 10f64.powf(-sigma / 5.0),
                d.value * 10f64.powf(sigma / 5.0),
            );
            assert!(
                (d.value - minus - low).abs() < 1e-9 * d.value,
                "σ = {sigma}"
            );
            assert!(
                (d.value + plus - high).abs() < 1e-9 * d.value,
                "σ = {sigma}"
            );
        }
    }
}
