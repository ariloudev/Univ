//! Units and their conversion within a dimension.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Parsecs in one light-year (IAU: 9 460 730 472 580 800 m; pc = 648 000 / π au).
const PARSECS_PER_LIGHT_YEAR: f64 = 0.306_601_393_788_06;
/// Parsecs in one astronomical unit: 1 / (648 000 / π).
const PARSECS_PER_AU: f64 = 4.848_136_811_095_36e-6;

/// A unit that a source publishes a value in, or that a value is normalized to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Degree,
    Arcminute,
    Arcsecond,
    Milliarcsecond,
    KilometrePerSecond,
    MetrePerSecond,
    MilliarcsecondPerYear,
    AstronomicalUnit,
    LightYear,
    Parsec,
    Kiloparsec,
    Megaparsec,
    /// Megaparsecs times the dimensionless Hubble parameter: a value in h⁻¹ Mpc, which becomes a
    /// length only once a Hubble constant is chosen.
    MegaparsecOverH,
    /// A magnitude, such as a distance modulus.
    Magnitude,
    /// A pure number, such as a redshift.
    Dimensionless,
    /// Dust extinction per parsec along a line of sight, the extinction in the units of Zhang,
    /// Green & Rix 2023, as the Edenhofer et al. 2024 dust map gives its
    /// density: its line integral is that extinction, which no other unit measures.
    ExtinctionZgr23PerParsec,
}

/// What a unit measures; units convert only within one dimension.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dimension {
    Angle,
    Velocity,
    ProperMotion,
    Length,
    /// Length in h⁻¹ Mpc, kept apart from length until a Hubble constant is chosen.
    HubbleScaledLength,
    Magnitude,
    Dimensionless,
    /// Extinction per unit length in the extinction scale of Zhang, Green & Rix 2023, which no
    /// other scale converts to without a cited relation.
    ExtinctionZgr23Density,
}

/// A conversion that cannot be made.
#[derive(Debug, Error, PartialEq)]
pub enum UnitError {
    #[error("cannot convert {from:?} to {to:?}: they measure different things")]
    Incompatible { from: Unit, to: Unit },
}

impl Unit {
    /// The dimension this unit measures.
    #[must_use]
    pub fn dimension(self) -> Dimension {
        match self {
            Self::Degree | Self::Arcminute | Self::Arcsecond | Self::Milliarcsecond => {
                Dimension::Angle
            }
            Self::KilometrePerSecond | Self::MetrePerSecond => Dimension::Velocity,
            Self::MilliarcsecondPerYear => Dimension::ProperMotion,
            Self::AstronomicalUnit
            | Self::LightYear
            | Self::Parsec
            | Self::Kiloparsec
            | Self::Megaparsec => Dimension::Length,
            Self::MegaparsecOverH => Dimension::HubbleScaledLength,
            Self::Magnitude => Dimension::Magnitude,
            Self::Dimensionless => Dimension::Dimensionless,
            Self::ExtinctionZgr23PerParsec => Dimension::ExtinctionZgr23Density,
        }
    }

    /// How many of the dimension's base unit one of this unit is: degrees, km/s, mas/yr, parsecs.
    fn in_base(self) -> f64 {
        match self {
            Self::Arcminute => 1.0 / 60.0,
            Self::Arcsecond => 1.0 / 3_600.0,
            Self::Milliarcsecond => 1.0 / 3_600_000.0,
            Self::MetrePerSecond => 1.0e-3,
            Self::AstronomicalUnit => PARSECS_PER_AU,
            Self::LightYear => PARSECS_PER_LIGHT_YEAR,
            Self::Kiloparsec => 1.0e3,
            Self::Megaparsec => 1.0e6,
            Self::Degree
            | Self::KilometrePerSecond
            | Self::MilliarcsecondPerYear
            | Self::Parsec
            | Self::MegaparsecOverH
            | Self::Magnitude
            | Self::Dimensionless
            | Self::ExtinctionZgr23PerParsec => 1.0,
        }
    }

    /// The factor that turns a value in this unit into one in `to`.
    ///
    /// # Errors
    ///
    /// Fails when the two units measure different dimensions.
    pub fn factor_to(self, to: Self) -> Result<f64, UnitError> {
        if self.dimension() == to.dimension() {
            Ok(self.in_base() / to.in_base())
        } else {
            Err(UnitError::Incompatible { from: self, to })
        }
    }
}

/// The name of the scale domain of stars, whose positions are in parsecs.
pub const STARS_DOMAIN: &str = "stars";

/// The name of the scale domain of the Galaxy's tracers, whose positions are in parsecs.
pub const GALAXY_TRACERS_DOMAIN: &str = "galaxy_tracers";

/// The unit of the positions of the scale domain named `domain` (`stars`, `galaxy_tracers`,
/// `nearby_galaxies` or `cosmic_web`): parsecs for stars and Galaxy tracers, megaparsecs for
/// every other.
#[must_use]
pub fn scale_domain_unit(domain: &str) -> Unit {
    if domain == STARS_DOMAIN || domain == GALAXY_TRACERS_DOMAIN {
        Unit::Parsec
    } else {
        Unit::Megaparsec
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-12 * b.abs().max(1.0)
    }

    #[test]
    fn units_convert_within_a_dimension() {
        assert!(close(
            Unit::Megaparsec.factor_to(Unit::Parsec).unwrap(),
            1.0e6
        ));
        assert!(close(
            Unit::Arcsecond.factor_to(Unit::Degree).unwrap(),
            1.0 / 3_600.0
        ));
        assert!(close(
            Unit::MetrePerSecond
                .factor_to(Unit::KilometrePerSecond)
                .unwrap(),
            1e-3
        ));
        assert!(close(
            Unit::Kiloparsec.factor_to(Unit::Megaparsec).unwrap(),
            1e-3
        ));
    }

    #[test]
    fn a_light_year_and_an_astronomical_unit_have_their_iau_lengths() {
        let au_per_pc = 1.0 / Unit::AstronomicalUnit.factor_to(Unit::Parsec).unwrap();
        assert!((au_per_pc - 206_264.806_247).abs() < 1e-3);
        let ly_per_pc = 1.0 / Unit::LightYear.factor_to(Unit::Parsec).unwrap();
        assert!((ly_per_pc - 3.261_563_777).abs() < 1e-8);
    }

    #[test]
    fn units_of_different_dimensions_do_not_convert() {
        assert_eq!(
            Unit::Degree.factor_to(Unit::Parsec),
            Err(UnitError::Incompatible {
                from: Unit::Degree,
                to: Unit::Parsec
            })
        );
        assert!(Unit::MegaparsecOverH.factor_to(Unit::Megaparsec).is_err());
        assert!(Unit::Magnitude.factor_to(Unit::Parsec).is_err());
    }

    #[test]
    fn stars_are_in_parsecs_and_every_other_domain_in_megaparsecs() {
        assert_eq!(scale_domain_unit("stars"), Unit::Parsec);
        assert_eq!(scale_domain_unit("nearby_galaxies"), Unit::Megaparsec);
        assert_eq!(scale_domain_unit("cosmic_web"), Unit::Megaparsec);
    }
}
