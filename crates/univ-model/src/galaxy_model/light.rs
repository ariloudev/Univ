//! Sharing the model's points among its populations by their light in the V band.

use super::ModelError;
use super::model::{Light, Model};

/// Each population's light in the V band, solar luminosities, in the model's order: its mass
/// over its mass-to-light ratio, or its share of the Galaxy's light, the light of the others
/// being the rest.
///
/// # Errors
///
/// Fails on a mass or ratio that is not positive, shares that leave no light to the others, or
/// a model with no light.
pub fn lights(model: &Model) -> Result<Vec<f64>, ModelError> {
    let positive = |x: f64| x.partial_cmp(&0.0) == Some(std::cmp::Ordering::Greater);
    let mut massive = 0.0;
    let mut shared = 0.0;
    // Each population's light, or its share where its light waits on the others'.
    let mut each = Vec::with_capacity(model.populations.len());
    for p in &model.populations {
        match &p.light {
            Light::Mass {
                mass,
                mass_to_light,
            } => {
                if !positive(mass.value) || !positive(mass_to_light.value) {
                    return Err(ModelError::Light(p.id.clone()));
                }
                let light = mass.value / mass_to_light.value;
                massive += light;
                each.push(Ok(light));
            }
            Light::Share { share } => {
                if share.value.is_nan() || share.value < 0.0 {
                    return Err(ModelError::Light(p.id.clone()));
                }
                shared += share.value;
                each.push(Err(share.value));
            }
        }
    }
    if shared >= 1.0 || !positive(massive) {
        return Err(ModelError::NoLight);
    }
    let total = massive / (1.0 - shared);
    Ok(each
        .into_iter()
        .map(|light| light.unwrap_or_else(|share| share * total))
        .collect())
}

/// The model points of each population, in the model's order: `points` shared in proportion to
/// `lights`, each rounded down and the points left given to the largest remainders, so that
/// they add up to `points` exactly.
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "counts of at most a few hundred million points"
)]
pub fn shares(points: u64, lights: &[f64]) -> Vec<u64> {
    let total: f64 = lights.iter().sum();
    let exact: Vec<f64> = lights.iter().map(|l| l / total * points as f64).collect();
    let mut out: Vec<u64> = exact.iter().map(|e| e.floor() as u64).collect();
    let mut left = points.saturating_sub(out.iter().sum());
    let mut order: Vec<usize> = (0..exact.len()).collect();
    order.sort_by(|&a, &b| {
        (exact[b] - exact[b].floor())
            .total_cmp(&(exact[a] - exact[a].floor()))
            .then(a.cmp(&b))
    });
    for i in order {
        if left == 0 {
            break;
        }
        out[i] += 1;
        left -= 1;
    }
    out
}
