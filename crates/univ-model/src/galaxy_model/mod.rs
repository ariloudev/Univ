//! The Galaxy model (build-the-galaxy-model): a model of the whole Milky Way's light, from
//! cited parameters, sampled into model points, each standing for an equal share of the
//! Galaxy's light in the V band. Its populations are drawn from their densities in a
//! Galactocentric frame, the arms' populations along the stretches of the spiral arms' fits,
//! and placed in ICRS Cartesian parsecs about the barycentre as the tiles hold positions.
//!
//! The sampling is seeded and pure: the same model and fits give the same points, in the same
//! order, on every target, wasm32 included, so that any client built on univ-model can sample
//! the model as a release did.

mod arms;
mod cover;
mod density;
mod frame;
mod light;
mod model;
mod random;
#[cfg(test)]
mod tests;

use thiserror::Error;

pub use arms::{ArmSampler, Fit, NEARNESS_CELL, NEARNESS_EXTENT, Nearness, Stretch, stretches};
pub use cover::{COVER_CUBE_PC, Counterpart, Counts, covered, cube_of};
pub use frame::Placement;
pub use light::{lights, shares};
pub use model::{
    ArmOrder, ArmRules, Colour, Density, Knots, Light, LongBar, Model, ModelPaper, Parameter,
    ParameterSource, Population,
};
pub use random::Random;

use crate::position::Vec3;

/// A model that cannot be sampled.
#[derive(Debug, Error, PartialEq)]
pub enum ModelError {
    #[error("population {0}'s light cannot stand: its mass, ratio or share is not positive")]
    Light(String),
    #[error("the model has no light: give a population a mass, and shares that leave it some")]
    NoLight,
    #[error("population {0} follows the arms, but no arm has a stretch to follow")]
    NoArms(String),
}

/// A sampled model point: its position, ICRS Cartesian parsecs about the barycentre, and its
/// population's code.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sampled {
    pub position: Vec3,
    pub population: u8,
}

/// The counterparts of the population with `density`: the young tracers for the arms'
/// populations, the stars for every other.
#[must_use]
pub fn counterpart(density: &Density) -> Counterpart {
    match density {
        Density::Arms { .. } => Counterpart::YoungTracers,
        _ => Counterpart::Stars,
    }
}

/// The points a knot of `knots` gathers: at least one.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a small positive count, rounded"
)]
fn per_knot(knots: &Knots) -> u64 {
    knots.per_knot.value.round().max(1.0) as u64
}

/// The model points of `model`, the arms' populations following `stretches`, those of
/// [`stretches`] of the model's arm rules:
/// each population's share of the points, in the model's order, each drawn from its density
/// on its own stream of the model's seed.
///
/// # Errors
///
/// Fails on a model whose light cannot be shared, or one with a population of the arms when no
/// arm has a stretch.
pub fn sample(model: &Model, stretches: &[Stretch]) -> Result<Vec<Sampled>, ModelError> {
    let counts = shares(model.points, &lights(model)?);
    let placement = Placement::new(model.r0.value, model.sun_height.value);
    let nearness = Nearness::new(stretches, model.arms.old_arm_spread.value);
    let mut out = Vec::with_capacity(usize::try_from(model.points).unwrap_or_default());
    for (population, n) in model.populations.iter().zip(counts) {
        // Only the arms' populations draw along the stretches, and their knots gather.
        let (arm_sampler, knots) = match &population.density {
            Density::Arms {
                radial_scale_length,
                knots,
                ..
            } => (
                Some(ArmSampler::new(stretches, radial_scale_length.value)),
                knots.as_ref(),
            ),
            _ => (None, None),
        };
        let context = density::Context {
            arms: arm_sampler.as_ref(),
            nearness: &nearness,
            old_arm_contrast: model.arms.old_arm_contrast.value,
        };
        let mut random = Random::new(model.seed, u64::from(population.code));
        let mut left = n;
        while left > 0 {
            let centre = density::draw(&population.density, &context, &mut random)
                .ok_or_else(|| ModelError::NoArms(population.id.clone()))?;
            let members = knots.map_or(1, per_knot);
            for _ in 0..members.min(left) {
                let p = match knots {
                    Some(k) => {
                        let r = k.knot_radius.value;
                        centre.map(|c| c + random.normal() * r)
                    }
                    None => centre,
                };
                out.push(Sampled {
                    position: placement.to_icrs_pc(p),
                    population: population.code,
                });
                left -= 1;
            }
        }
    }
    Ok(out)
}
