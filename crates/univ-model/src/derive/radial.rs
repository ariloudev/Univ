//! Distances along the line of sight under the release cosmology, and lengths rescaled from the
//! Hubble constant they were published for.

use serde::Serialize;

use super::chain::{Step, Transformation};
use super::{DeriveError, MODEL_VERSION, Release};
use crate::cosmology::{AssumedCosmology, Cosmology, MAX_REDSHIFT, SPEED_OF_LIGHT};
use crate::distance::{Distance, Measure};
use crate::redshift::CmbRedshift;
use crate::units::{Dimension, Unit};
use crate::value::{Quantity, Uncertainty};

/// A distance along the line of sight in Mpc, derived under a release cosmology from a
/// published distance or a CMB-frame redshift. Only a [`Release`] makes one, and a position
/// needs one: an object without a distance or redshift cannot be placed.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Radial {
    native: Quantity,
    assumed: Option<AssumedCosmology>,
    distance: Quantity,
    measure: Measure,
    cosmology: Cosmology,
    chain: Vec<Step>,
    model_version: &'static str,
}

impl Radial {
    /// What it was derived from, as published: a distance, modulus or redshift.
    #[must_use]
    pub fn native(&self) -> &Quantity {
        &self.native
    }

    /// The cosmology the native value was published for, where it depends on one.
    #[must_use]
    pub fn assumed(&self) -> Option<&AssumedCosmology> {
        self.assumed.as_ref()
    }

    /// The distance in Mpc, with its uncertainty.
    #[must_use]
    pub fn distance(&self) -> &Quantity {
        &self.distance
    }

    /// Which distance it is: comoving, or as published where the chain says why.
    #[must_use]
    pub fn measure(&self) -> Measure {
        self.measure
    }

    #[must_use]
    pub fn cosmology(&self) -> &Cosmology {
        &self.cosmology
    }

    #[must_use]
    pub fn chain(&self) -> &[Step] {
        &self.chain
    }

    #[must_use]
    pub fn model_version(&self) -> &'static str {
        self.model_version
    }

    pub(super) fn into_parts(self) -> (Quantity, Option<AssumedCosmology>, Quantity, Measure) {
        (self.native, self.assumed, self.distance, self.measure)
    }
}

/// A length beside the length it was published as and the cosmology that was for.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Rescaled {
    pub native: Quantity,
    pub assumed: AssumedCosmology,
    pub rescaled: Quantity,
    pub factor: f64,
}

/// The factor that takes lengths published for one cosmology to the release's: the ratio of
/// the Hubble constants, or of the comoving distances at the structure's redshift. Applying
/// one rescaling to a structure's distance and to its size keeps their ratio.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Rescaling {
    pub assumed: AssumedCosmology,
    pub release_hubble_constant: f64,
    pub factor: f64,
    /// The redshift of the comoving ratio, or none for the ratio of Hubble constants.
    pub redshift: Option<f64>,
}

impl Rescaling {
    /// `length` rescaled, beside its native value: a length in any unit, or one in h⁻¹ Mpc
    /// when the assumed cosmology is H0 = 100, which becomes Mpc.
    ///
    /// # Errors
    ///
    /// Fails when `length` is not a length, or is in h⁻¹ Mpc for another Hubble constant.
    pub fn apply(&self, length: &Quantity) -> Result<Rescaled, DeriveError> {
        let unit = match length.unit.dimension() {
            Dimension::Length => length.unit,
            Dimension::HubbleScaledLength => {
                if !self.assumed.is_hubble_scaled() {
                    return Err(DeriveError::HubbleScaled(self.assumed.hubble_constant));
                }
                Unit::Megaparsec
            }
            _ => return Err(DeriveError::NotALength(length.unit)),
        };
        let rescaled = Quantity::new(
            length.value * self.factor,
            unit,
            length.uncertainty.scaled(self.factor),
        )?;
        Ok(Rescaled {
            native: *length,
            assumed: self.assumed,
            rescaled,
            factor: self.factor,
        })
    }

    /// The step this rescaling adds to a chain.
    #[must_use]
    pub fn step(&self) -> Step {
        let (assumed, factor) = (self.assumed, self.factor);
        match self.redshift {
            Some(redshift) => Transformation::ComovingRescaled {
                assumed,
                redshift,
                factor,
            },
            None => Transformation::HubbleRescaled {
                assumed,
                release_hubble_constant: self.release_hubble_constant,
                factor,
            },
        }
        .into()
    }
}

const NO_REDSHIFT: &str = "the entry has no CMB-frame redshift to convert it with: it stays a \
                           luminosity distance, larger than the comoving one by the factor 1 + z";
const UNSPECIFIED: &str = "its source does not say which distance it publishes";

impl Release {
    /// The rescaling of lengths published for `assumed` by the ratio of Hubble constants.
    ///
    /// # Errors
    ///
    /// Fails when `assumed` cannot stand.
    pub fn hubble_rescaling(&self, assumed: AssumedCosmology) -> Result<Rescaling, DeriveError> {
        assumed.check()?;
        let release = self.cosmology().hubble_constant;
        Ok(Rescaling {
            assumed,
            release_hubble_constant: release,
            factor: assumed.hubble_constant / release,
            redshift: None,
        })
    }

    /// The rescaling of the lengths of a structure at `redshift` published for `assumed`: the
    /// ratio of the comoving distances of `redshift` when `assumed` states a flat ΛCDM, which
    /// keeps a size in proportion to its distance at any redshift; otherwise the ratio of
    /// Hubble constants, which does so where `D_C ∝ 1 / H0`, at z ≪ 1.
    ///
    /// # Errors
    ///
    /// Fails when `assumed` cannot stand or `redshift` lies outside the model.
    pub fn rescaling_at(
        &self,
        assumed: AssumedCosmology,
        redshift: f64,
    ) -> Result<Rescaling, DeriveError> {
        let hubble = self.hubble_rescaling(assumed)?;
        let Some(theirs) = assumed.flat().filter(|_| redshift > 0.0) else {
            return Ok(hubble);
        };
        Ok(Rescaling {
            factor: self.table.distance(redshift)? / theirs.comoving_distance(redshift)?,
            redshift: Some(redshift),
            ..hubble
        })
    }

    /// The radial distance of a published distance: rescaled to the release's Hubble constant
    /// where it was published for another, then a luminosity distance made comoving with
    /// `cmb`, the CMB-frame redshift of the same entry, where it has one. See the module for
    /// what is kept as published.
    ///
    /// # Errors
    ///
    /// Fails when the distance cannot be put in Mpc or its assumed cosmology cannot stand.
    pub fn radial_from_distance(
        &self,
        distance: &Distance,
        cmb: Option<&CmbRedshift>,
    ) -> Result<Radial, DeriveError> {
        let mut chain: Vec<Step> = vec![
            Transformation::Published {
                native: distance.native,
                measure: distance.measure,
            }
            .into(),
        ];
        if distance.native.unit == Unit::Magnitude {
            let modulus = distance.native;
            chain.push(Transformation::FromModulus { modulus }.into());
        }
        let mut length = distance.normalized.to(Unit::Megaparsec)?;
        if let Some(assumed) = distance.assumed_cosmology {
            let rescaling = self.hubble_rescaling(assumed)?;
            length = rescaling.apply(&length)?.rescaled;
            chain.push(rescaling.step());
        }
        let (length, measure) = match (distance.measure, cmb) {
            (Measure::Luminosity, Some(cmb)) => {
                chain.push(
                    Transformation::LuminosityToComoving {
                        cmb_redshift: cmb.value.value,
                    }
                    .into(),
                );
                (
                    luminosity_to_comoving(&length, &cmb.value)?,
                    Measure::Comoving,
                )
            }
            (Measure::Luminosity | Measure::Unspecified, _) => {
                let reason = if distance.measure == Measure::Luminosity {
                    NO_REDSHIFT
                } else {
                    UNSPECIFIED
                };
                chain.push(
                    Transformation::KeptAsPublished {
                        measure: distance.measure,
                        reason: reason.into(),
                    }
                    .into(),
                );
                (length, distance.measure)
            }
            (Measure::Euclidean, _) => {
                let largest_difference =
                    self.cosmology().hubble_constant * length.value / SPEED_OF_LIGHT;
                chain.push(Transformation::Euclidean { largest_difference }.into());
                (length, Measure::Euclidean)
            }
            (Measure::Comoving, _) => (length, Measure::Comoving),
        };
        Ok(self.radial(
            distance.native,
            distance.assumed_cosmology,
            length,
            measure,
            chain,
        ))
    }

    /// The radial distance of a CMB-frame redshift: its comoving distance in the release
    /// cosmology, the redshift's error carried to an asymmetric interval.
    ///
    /// # Errors
    ///
    /// Fails on a blueshift, which gives no distance, or a redshift beyond the model.
    pub fn radial_from_redshift(&self, cmb: &CmbRedshift) -> Result<Radial, DeriveError> {
        let z = cmb.value;
        if z.value < 0.0 {
            return Err(DeriveError::Blueshift(z.value));
        }
        let at = |z: f64| self.table.distance(z.clamp(0.0, MAX_REDSHIFT));
        let centre = self.table.distance(z.value)?;
        let interval = |minus: f64, plus: f64| -> Result<(f64, f64), DeriveError> {
            Ok((centre - at(z.value - minus)?, at(z.value + plus)? - centre))
        };
        let uncertainty = match z.uncertainty {
            Uncertainty::Symmetric { sigma } => {
                let (minus, plus) = interval(sigma, sigma)?;
                Uncertainty::Asymmetric { minus, plus }
            }
            Uncertainty::Asymmetric { minus, plus } => {
                let (minus, plus) = interval(minus, plus)?;
                Uncertainty::Asymmetric { minus, plus }
            }
            Uncertainty::CredibleInterval {
                minus,
                plus,
                probability,
            } => {
                let (minus, plus) = interval(minus, plus)?;
                Uncertainty::CredibleInterval {
                    minus,
                    plus,
                    probability,
                }
            }
            other => other,
        };
        let chain = vec![
            Transformation::ComovingFromRedshift {
                cmb_redshift: z.value,
                heliocentric_redshift: cmb.heliocentric.value,
                dipole: cmb.dipole,
            }
            .into(),
        ];
        let length = Quantity::new(centre, Unit::Megaparsec, uncertainty)?;
        Ok(self.radial(z, None, length, Measure::Comoving, chain))
    }

    fn radial(
        &self,
        native: Quantity,
        assumed: Option<AssumedCosmology>,
        distance: Quantity,
        measure: Measure,
        chain: Vec<Step>,
    ) -> Radial {
        Radial {
            native,
            assumed,
            distance,
            measure,
            cosmology: self.cosmology().clone(),
            chain,
            model_version: MODEL_VERSION,
        }
    }
}

/// `D_C = D_L / (1 + z)`. The error of `D_L` scales by 1 / (1 + z); a symmetric error in z
/// adds `D_C σz / (1 + z)` in quadrature to each side of a symmetric or asymmetric one. A
/// distance with no reported error keeps none: the redshift's alone would understate it.
fn luminosity_to_comoving(luminosity: &Quantity, z: &Quantity) -> Result<Quantity, DeriveError> {
    let factor = 1.0 / (1.0 + z.value);
    if !factor.is_finite() || factor <= 0.0 {
        return Err(DeriveError::Blueshift(z.value));
    }
    let comoving = luminosity.value * factor;
    let extra = match z.uncertainty {
        Uncertainty::Symmetric { sigma } => comoving * sigma * factor,
        _ => 0.0,
    };
    let uncertainty = match luminosity.uncertainty.scaled(factor) {
        Uncertainty::Symmetric { sigma } => Uncertainty::Symmetric {
            sigma: sigma.hypot(extra),
        },
        Uncertainty::Asymmetric { minus, plus } => Uncertainty::Asymmetric {
            minus: minus.hypot(extra),
            plus: plus.hypot(extra),
        },
        other => other,
    };
    Ok(Quantity::new(comoving, luminosity.unit, uncertainty)?)
}
