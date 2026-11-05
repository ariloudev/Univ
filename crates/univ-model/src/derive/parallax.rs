//! A parallax made a distance: its zero point subtracted, where its source has one, then
//! inverted. The one inversion the ranking and the derived positions share.

use super::chain::{Step, Transformation};
use super::{DeriveError, DerivedPosition, Direction, Release};
use crate::cosmology::SPEED_OF_LIGHT;
use crate::distance::Measure;
use crate::entry::Position;
use crate::units::Unit;
use crate::value::{Quantity, Uncertainty, Value};

/// A parallax inverted: the parallax after its zero point, its distance `1 / ϖ`, and the exact
/// interval `[1 / (ϖ + σ), 1 / (ϖ − σ)]` where the parallax has a symmetric error smaller than
/// itself.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InvertedParallax {
    /// The parallax inverted, mas: the published one less its zero point.
    pub parallax_mas: f64,
    /// Its symmetric error, mas, where one was published.
    pub sigma_mas: Option<f64>,
    /// The distance, pc.
    pub distance_pc: f64,
    /// The near and far ends of the distance interval, pc.
    pub interval_pc: Option<(f64, f64)>,
}

impl InvertedParallax {
    /// The parallax's error over the parallax inverted, where it has one.
    #[must_use]
    pub fn fraction(&self) -> Option<f64> {
        self.sigma_mas.map(|s| s / self.parallax_mas)
    }
}

/// The parallax `published_mas`, of symmetric error `sigma_mas`, less `zero_point_mas` and
/// inverted; none when the corrected parallax is not positive.
#[must_use]
pub fn inverted_parallax(
    published_mas: f64,
    sigma_mas: Option<f64>,
    zero_point_mas: Option<f64>,
) -> Option<InvertedParallax> {
    let parallax_mas = published_mas - zero_point_mas.unwrap_or(0.0);
    if !(parallax_mas.is_finite() && parallax_mas > 0.0) {
        return None;
    }
    let pc = |mas: f64| 1_000.0 / mas;
    let interval_pc = sigma_mas
        .filter(|s| *s < parallax_mas)
        .map(|s| (pc(parallax_mas + s), pc(parallax_mas - s)));
    Some(InvertedParallax {
        parallax_mas,
        sigma_mas,
        distance_pc: pc(parallax_mas),
        interval_pc,
    })
}

/// The symmetric error of a parallax, mas.
fn sigma_mas(parallax: &Quantity) -> Option<f64> {
    match parallax.uncertainty {
        Uncertainty::Symmetric { sigma } => Some(sigma),
        _ => None,
    }
}

impl Release {
    /// The radial distance of a published parallax: less `zero_point_mas` where its source has
    /// one, then inverted, a Euclidean distance inferred from it. Its chain starts from the
    /// parallax as published, and its native value is that parallax.
    ///
    /// # Errors
    ///
    /// Fails when the parallax is not an angle, or after its zero point is not positive or not
    /// larger than its error, which leaves the far end of the interval unbounded.
    pub fn radial_from_parallax(
        &self,
        parallax: &Value,
        zero_point_mas: Option<f64>,
    ) -> Result<super::Radial, DeriveError> {
        let mas = parallax.normalized.to(Unit::Milliarcsecond)?;
        let sigma = sigma_mas(&mas);
        let inverted = inverted_parallax(mas.value, sigma, zero_point_mas)
            .ok_or(DeriveError::Parallax(mas.value, "not positive"))?;
        let mut chain: Vec<Step> = vec![
            Transformation::PublishedParallax {
                native: parallax.native,
            }
            .into(),
        ];
        if let Some(zero_point_mas) = zero_point_mas {
            let corrected_mas = inverted.parallax_mas;
            chain.push(
                Transformation::ParallaxZeroPoint {
                    zero_point_mas,
                    corrected_mas,
                }
                .into(),
            );
        }
        let parallax_mas = inverted.parallax_mas;
        chain.push(Transformation::InvertedParallax { parallax_mas }.into());
        let mpc = inverted.distance_pc * 1e-6;
        let uncertainty = match (sigma, inverted.interval_pc) {
            (None, _) => Uncertainty::NoneReported,
            (Some(_), Some((near, far))) => Uncertainty::Asymmetric {
                minus: mpc - near * 1e-6,
                plus: far * 1e-6 - mpc,
            },
            (Some(_), None) => {
                return Err(DeriveError::Parallax(
                    parallax_mas,
                    "not larger than its error",
                ));
            }
        };
        let largest_difference = self.cosmology().hubble_constant * mpc / SPEED_OF_LIGHT;
        chain.push(Transformation::Euclidean { largest_difference }.into());
        let distance = Quantity::new(mpc, Unit::Megaparsec, uncertainty)?;
        Ok(self.radial(parallax.native, None, distance, Measure::Euclidean, chain))
    }

    /// Places an object from its parallax, as [`Release::radial_from_parallax`] gives it, toward
    /// `position`.
    ///
    /// # Errors
    ///
    /// Fails as [`Release::radial_from_parallax`] does.
    pub fn place_parallax(
        &self,
        position: &Position,
        parallax: &Value,
        zero_point_mas: Option<f64>,
    ) -> Result<DerivedPosition, DeriveError> {
        let radial = self.radial_from_parallax(parallax, zero_point_mas)?;
        Ok(DerivedPosition::new(Direction::of(position), radial))
    }
}
