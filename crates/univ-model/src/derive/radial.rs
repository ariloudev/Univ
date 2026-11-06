//! Distances along the line of sight under the release cosmology, and lengths rescaled from the
//! Hubble constant they were published for.

use std::sync::Arc;

use serde::Serialize;

use super::chain::{Step, Transformation};
use super::{DeriveError, MODEL_VERSION, Release};
use crate::cosmology::{AssumedCosmology, Cosmology, MAX_REDSHIFT, SPEED_OF_LIGHT};
use crate::distance::{Distance, Measure};
use crate::entry::Redshift;
use crate::redshift::{CmbRedshift, RedshiftFrame};
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
    cosmology: Arc<Cosmology>,
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

    pub(super) fn into_parts(self) -> RadialParts {
        RadialParts {
            native: self.native,
            assumed: self.assumed,
            distance: self.distance,
            measure: self.measure,
            cosmology: self.cosmology,
            chain: self.chain,
        }
    }
}

/// A [`Radial`] taken apart, for the position made from it.
pub(super) struct RadialParts {
    pub native: Quantity,
    pub assumed: Option<AssumedCosmology>,
    pub distance: Quantity,
    pub measure: Measure,
    pub cosmology: Arc<Cosmology>,
    pub chain: Vec<Step>,
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
const BLUESHIFT: &str = "the entry's CMB-frame redshift is a blueshift, which gives no comoving \
                         distance: it stays a luminosity distance";
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

    /// The radial distance of a published distance: rescaled where it was published for
    /// another cosmology, by the same rule as a size ([`Release::rescaling_at`] at the CMB-frame
    /// redshift of `redshift`, the ratio of Hubble constants without one), then a luminosity
    /// distance made comoving with `redshift`, a redshift of the same entry, where it has one
    /// that is not a blueshift. See the module for what is kept as published.
    ///
    /// # Errors
    ///
    /// Fails when the distance cannot be put in Mpc, its assumed cosmology cannot stand, or
    /// the redshift's error cannot be carried into it.
    pub fn radial_from_distance(
        &self,
        distance: &Distance,
        redshift: Option<&Redshift>,
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
            let rescaling = match redshift {
                Some(z) => self.rescaling_at(assumed, z.cmb.value.value)?,
                None => self.hubble_rescaling(assumed)?,
            };
            length = rescaling.apply(&length)?.rescaled;
            chain.push(rescaling.step());
        }
        let converted = match (distance.measure, redshift) {
            (Measure::Luminosity, Some(z)) => match luminosity_to_comoving(&length, &z.cmb.value) {
                Err(DeriveError::Blueshift(_)) => None,
                other => Some((other?, z)),
            },
            _ => None,
        };
        let (length, measure) = match (distance.measure, converted) {
            (Measure::Luminosity, Some((comoving, z))) => {
                chain.push(
                    Transformation::LuminosityToComoving {
                        cmb_redshift: z.cmb.value.value,
                        redshift_kind: z.kind,
                    }
                    .into(),
                );
                (comoving, Measure::Comoving)
            }
            (Measure::Luminosity | Measure::Unspecified, _) => {
                let reason = match (distance.measure, redshift) {
                    (Measure::Luminosity, None) => NO_REDSHIFT,
                    (Measure::Luminosity, Some(_)) => BLUESHIFT,
                    _ => UNSPECIFIED,
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
    /// cosmology, the redshift's error and the peculiar-velocity term, added in quadrature by
    /// [`redshift_sigma`] as the ranking adds them, carried to an asymmetric interval through
    /// D(z). A redshift published without an error, or as a limit, still has the
    /// peculiar-velocity term: the line-of-sight velocity its redshift does not separate from the
    /// Hubble flow, the pipeline's stated term, not an invented measurement error. A stated
    /// interval with no probability, or a credible interval of another probability than one
    /// sigma, has its bounds carried through D(z) as bounds, with no term added in quadrature:
    /// a bound is not a sigma.
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
        let v = self.peculiar_velocity_km_s();
        let side = |sigma_z: f64| redshift_sigma(z.value, sigma_z, v);
        let uncertainty = match z.uncertainty {
            Uncertainty::NoneReported | Uncertainty::UpperLimit | Uncertainty::LowerLimit => {
                let (minus, plus) = interval(side(0.0), side(0.0))?;
                Uncertainty::Asymmetric { minus, plus }
            }
            Uncertainty::Symmetric { sigma } => {
                let (minus, plus) = interval(side(sigma), side(sigma))?;
                Uncertainty::Asymmetric { minus, plus }
            }
            Uncertainty::Asymmetric { minus, plus } => {
                let (minus, plus) = interval(side(minus), side(plus))?;
                Uncertainty::Asymmetric { minus, plus }
            }
            Uncertainty::CredibleInterval {
                minus,
                plus,
                probability,
            } if is_one_sigma(probability) => {
                let (minus, plus) = interval(side(minus), side(plus))?;
                Uncertainty::CredibleInterval {
                    minus,
                    plus,
                    probability,
                }
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
            Uncertainty::Interval { minus, plus } => {
                let (minus, plus) = interval(minus, plus)?;
                Uncertainty::Interval { minus, plus }
            }
            other @ Uncertainty::CredibleRegion { .. } => other,
        };
        let step = match cmb.published {
            RedshiftFrame::Cmb => Transformation::ComovingFromPublishedCmbRedshift {
                cmb_redshift: z.value,
            },
            RedshiftFrame::Barycentric => Transformation::ComovingFromBarycentricRedshift {
                cmb_redshift: z.value,
                barycentric_redshift: cmb.measured.value,
                dipole: cmb.dipole,
            },
            _ => Transformation::ComovingFromRedshift {
                cmb_redshift: z.value,
                heliocentric_redshift: cmb.measured.value,
                dipole: cmb.dipole,
            },
        };
        let chain = vec![step.into()];
        let length = Quantity::new(centre, Unit::Megaparsec, uncertainty)?;
        Ok(self.radial(z, None, length, Measure::Comoving, chain))
    }

    pub(super) fn radial(
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
            cosmology: Arc::clone(self.table().shared_cosmology()),
            chain,
            model_version: MODEL_VERSION,
        }
    }
}

/// The probability of a one-sigma interval, within the rounding sources give it.
fn is_one_sigma(probability: f64) -> bool {
    (probability - 0.6827).abs() < 0.01
}

/// The redshift `(1 + z) v / c` of a line-of-sight peculiar velocity of `v` km/s at `z`.
#[must_use]
pub fn peculiar_velocity_redshift(z: f64, peculiar_velocity_km_s: f64) -> f64 {
    (1.0 + z) * peculiar_velocity_km_s / SPEED_OF_LIGHT
}

/// The error in z of a redshift distance, `sqrt(σz² + ((1 + z) v / c)²)`: the redshift's own
/// error and the peculiar velocity `v`, in quadrature. A distance's error is `D′(z)` times it;
/// the ranking and the derived positions both take it from here.
#[must_use]
pub fn redshift_sigma(z: f64, sigma_z: f64, peculiar_velocity_km_s: f64) -> f64 {
    sigma_z.hypot(peculiar_velocity_redshift(z, peculiar_velocity_km_s))
}

/// `D_C = D_L / (1 + z)`. The error of `D_L` scales by 1 / (1 + z), and the redshift's error
/// adds `D_C σz / (1 + z)` in quadrature, its upper side to the lower side of `D_C` and its lower
/// side to the upper; a credible interval of one sigma counts as its sides. A distance with no
/// reported error keeps none: the redshift's alone would understate it.
///
/// # Errors
///
/// Fails on a blueshift, `z < 0`, as [`Release::radial_from_redshift`] does, and on a redshift
/// error, or a distance interval, that cannot be added in quadrature: a redshift limit, or a
/// credible interval of another probability than one sigma.
fn luminosity_to_comoving(luminosity: &Quantity, z: &Quantity) -> Result<Quantity, DeriveError> {
    if z.value.is_nan() || z.value < 0.0 {
        return Err(DeriveError::Blueshift(z.value));
    }
    let factor = 1.0 / (1.0 + z.value);
    let comoving = luminosity.value * factor;
    let symmetric = matches!(
        z.uncertainty,
        Uncertainty::NoneReported | Uncertainty::Symmetric { .. }
    );
    let (z_minus, z_plus) = match z.uncertainty {
        Uncertainty::NoneReported => (0.0, 0.0),
        Uncertainty::Symmetric { sigma } => (sigma, sigma),
        Uncertainty::Asymmetric { minus, plus } => (minus, plus),
        Uncertainty::CredibleInterval {
            minus,
            plus,
            probability,
        } if is_one_sigma(probability) => (minus, plus),
        other => return Err(DeriveError::Uncombinable(other)),
    };
    let (below, above) = (comoving * z_plus * factor, comoving * z_minus * factor);
    let uncertainty = match luminosity.uncertainty.scaled(factor) {
        Uncertainty::Symmetric { sigma } if symmetric => Uncertainty::Symmetric {
            sigma: sigma.hypot(below),
        },
        Uncertainty::Symmetric { sigma } => Uncertainty::Asymmetric {
            minus: sigma.hypot(below),
            plus: sigma.hypot(above),
        },
        Uncertainty::Asymmetric { minus, plus } => Uncertainty::Asymmetric {
            minus: minus.hypot(below),
            plus: plus.hypot(above),
        },
        Uncertainty::CredibleInterval {
            minus,
            plus,
            probability,
        } if is_one_sigma(probability) => Uncertainty::CredibleInterval {
            minus: minus.hypot(below),
            plus: plus.hypot(above),
            probability,
        },
        other @ (Uncertainty::NoneReported | Uncertainty::UpperLimit | Uncertainty::LowerLimit) => {
            other
        }
        other if z_minus.max(z_plus) <= 0.0 => other,
        other => return Err(DeriveError::Uncombinable(other)),
    };
    Ok(Quantity::new(comoving, luminosity.unit, uncertainty)?)
}
