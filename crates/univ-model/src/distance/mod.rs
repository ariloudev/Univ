//! Distances as published, with their kind, method and the Hubble constant they assume.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::cosmology::{AssumedCosmology, CosmologyError};
use crate::units::{Dimension, Unit};
use crate::value::{KindOfValue, Quantity, ValueError};

mod convert;

pub use convert::{from_hubble_scaled, from_modulus, rescale_hubble};

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

/// Which distance a value is, as its source states it. Beyond the nearby universe the
/// cosmological distances differ, by a factor 1 + z between the luminosity and the comoving
/// distance of a flat universe; a later stage converts each to the comoving distance with the
/// release cosmology.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Measure {
    /// A luminosity distance: every distance derived from a distance modulus is one.
    Luminosity,
    /// A comoving distance.
    Comoving,
    /// A distance within the Galaxy measured geometrically, such as from a parallax, where
    /// space is Euclidean to the data's precision and the cosmological distances coincide.
    Euclidean,
    /// The source does not say which distance it publishes.
    Unspecified,
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
    /// Which distance it is: a luminosity distance for one derived from a modulus.
    pub measure: Measure,
    /// The cosmology the value was published for, where it depends on one: H0 = 100 km/s/Mpc
    /// for a length in h⁻¹ Mpc.
    pub assumed_cosmology: Option<AssumedCosmology>,
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
    measure: Measure,
    assumed_cosmology: Option<AssumedCosmology>,
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
            raw.measure,
            raw.assumed_cosmology,
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

impl Distance {
    /// A distance from its value as published, normalized to `unit`: a modulus through
    /// [`from_modulus`], a length in h⁻¹ Mpc as the length for H0 = 100 km/s/Mpc, which it then
    /// records, and a length as it is.
    ///
    /// # Errors
    ///
    /// Fails when `unit` is neither parsecs nor megaparsecs, `native` is not a length, a
    /// Hubble-scaled length or a modulus, `also_published` is neither a length nor a modulus,
    /// a mixed method lists fewer than two methods, a modulus is not given as a luminosity
    /// distance, or the assumed cosmology cannot stand or, for a Hubble-scaled length, has
    /// another Hubble constant than 100.
    pub fn new(
        native: Quantity,
        also_published: Option<Quantity>,
        unit: Unit,
        kind_of_value: KindOfValue,
        (kind, method): (DistanceKind, Method),
        measure: Measure,
        assumed_cosmology: Option<AssumedCosmology>,
    ) -> Result<Self, DistanceError> {
        if native.unit == Unit::Magnitude && measure != Measure::Luminosity {
            return Err(DistanceError::ModulusNotLuminosity(measure));
        }
        if !matches!(unit, Unit::Parsec | Unit::Megaparsec) {
            return Err(DistanceError::NormalizedUnit(unit));
        }
        if let Some(assumed) = &assumed_cosmology {
            assumed.check()?;
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
        let (normalized, assumed_cosmology) = match native.unit.dimension() {
            Dimension::Magnitude => (from_modulus(&native)?.to(unit)?, assumed_cosmology),
            Dimension::Length => (native.to(unit)?, assumed_cosmology),
            Dimension::HubbleScaledLength => {
                let scaled = AssumedCosmology::HUBBLE_SCALED;
                if assumed_cosmology.is_some_and(|a| !a.is_hubble_scaled()) {
                    return Err(DistanceError::WrongUnit {
                        expected: Unit::Megaparsec,
                        found: native.unit,
                    });
                }
                let mpc = Quantity::new(native.value, Unit::Megaparsec, native.uncertainty)?;
                (mpc.to(unit)?, Some(assumed_cosmology.unwrap_or(scaled)))
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
            measure,
            assumed_cosmology,
        })
    }

    /// The Hubble constant in km/s/Mpc the value was published for, where it depends on one.
    #[must_use]
    pub fn hubble_constant(&self) -> Option<f64> {
        self.assumed_cosmology.map(|a| a.hubble_constant)
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
    #[error("a distance from a distance modulus is a luminosity distance, not {0:?}")]
    ModulusNotLuminosity(Measure),
    #[error("a mixed method lists at least two methods")]
    MixedOfOne,
    #[error("the normalized distance {normalized} is not the {derived} its native value gives")]
    Inconsistent { normalized: f64, derived: f64 },
    #[error("the Hubble constant {0} km/s/Mpc is not positive")]
    BadHubbleConstant(f64),
    #[error(transparent)]
    Cosmology(#[from] CosmologyError),
    #[error(transparent)]
    Value(#[from] ValueError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Uncertainty;

    fn quantity(value: f64, unit: Unit, uncertainty: Uncertainty) -> Quantity {
        Quantity::new(value, unit, uncertainty).unwrap()
    }

    fn measured(native: Quantity, unit: Unit, h: Option<f64>) -> Result<Distance, DistanceError> {
        let h = h.map(AssumedCosmology::hubble);
        let how = (DistanceKind::SecondaryIndicator, Method::TullyFisher);
        let measure = if native.unit == Unit::Magnitude {
            Measure::Luminosity
        } else {
            Measure::Unspecified
        };
        Distance::new(native, None, unit, KindOfValue::Measured, how, measure, h)
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
        assert_eq!(d.hubble_constant(), Some(100.0));
        assert_eq!(d.normalized.value, 50.0);
        assert!(measured(scaled, Unit::Megaparsec, Some(73.0)).is_err());
        let at_73 = quantity(8.0, Unit::Megaparsec, Uncertainty::NoneReported);
        assert_eq!(
            measured(at_73, Unit::Megaparsec, Some(73.0))
                .unwrap()
                .hubble_constant(),
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
        let one = Distance::new(
            d,
            None,
            Unit::Parsec,
            KindOfValue::Measured,
            how,
            Measure::Euclidean,
            None,
        );
        assert_eq!(one, Err(DistanceError::MixedOfOne));
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

    #[test]
    fn a_distance_from_a_modulus_is_a_luminosity_distance_and_nothing_else() {
        let mu = quantity(30.0, Unit::Magnitude, Uncertainty::NoneReported);
        assert_eq!(
            measured(mu, Unit::Megaparsec, None).unwrap().measure,
            Measure::Luminosity
        );
        let how = (DistanceKind::SecondaryIndicator, Method::TullyFisher);
        let comoving = Distance::new(
            mu,
            None,
            Unit::Megaparsec,
            KindOfValue::Measured,
            how,
            Measure::Comoving,
            None,
        );
        assert_eq!(
            comoving,
            Err(DistanceError::ModulusNotLuminosity(Measure::Comoving))
        );
    }
}
