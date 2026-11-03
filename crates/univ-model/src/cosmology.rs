//! Flat ΛCDM cosmology: the comoving distance of a redshift and its inverse.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::value::positive;

/// The speed of light in km/s.
pub const SPEED_OF_LIGHT: f64 = 299_792.458;

/// Simpson intervals per integration: the error stays below 1e-8 of the result up to
/// [`MAX_REDSHIFT`].
const INTERVALS: u32 = 256;
/// The largest redshift this model answers for. It leaves out radiation, whose share of H(z)
/// grows with z: against the full Planck 2018 model the comoving distance is 1.7e-4 off at
/// z = 3, and the gap keeps widening, so the model stops at z = 10.
pub const MAX_REDSHIFT: f64 = 10.0;

/// A flat ΛCDM cosmology without radiation, as a release names it. Checked when built and when
/// read.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawCosmology")]
pub struct Cosmology {
    /// Its name and citation.
    pub name: Cow<'static, str>,
    /// The Hubble constant in km/s/Mpc.
    pub hubble_constant: f64,
    /// The matter density today; Λ makes up the rest.
    pub omega_matter: f64,
}

/// A cosmology as read, before its checks.
#[derive(Deserialize)]
struct RawCosmology {
    name: String,
    hubble_constant: f64,
    omega_matter: f64,
}

/// A cosmology, distance or redshift outside what the model can answer.
#[derive(Debug, Error, PartialEq)]
pub enum CosmologyError {
    #[error("the Hubble constant {0} km/s/Mpc is not positive")]
    HubbleConstant(f64),
    #[error("the matter density {0} lies outside (0, 1]")]
    MatterDensity(f64),
    #[error("the redshift {0} is negative, not a finite number or beyond z = {MAX_REDSHIFT}")]
    Redshift(f64),
    #[error("the comoving distance {0} Mpc is negative or lies beyond z = {MAX_REDSHIFT}")]
    Distance(f64),
}

impl Cosmology {
    /// # Errors
    ///
    /// Fails on a Hubble constant that is not positive or a matter density outside (0, 1].
    pub fn new(
        name: impl Into<Cow<'static, str>>,
        hubble_constant: f64,
        omega_matter: f64,
    ) -> Result<Self, CosmologyError> {
        if !positive(hubble_constant) {
            return Err(CosmologyError::HubbleConstant(hubble_constant));
        }
        if !(omega_matter > 0.0 && omega_matter <= 1.0) {
            return Err(CosmologyError::MatterDensity(omega_matter));
        }
        Ok(Self {
            name: name.into(),
            hubble_constant,
            omega_matter,
        })
    }

    /// Planck 2018, Table 2, last column (Planck Collaboration 2020, A&A 641, A6): H0 = 67.66
    /// km/s/Mpc and Ωm = 0.3111.
    #[must_use]
    pub fn planck_2018() -> Self {
        Self {
            name: Cow::Borrowed("Planck 2018 (A&A 641, A6, Table 2: TT,TE,EE+lowE+lensing+BAO)"),
            hubble_constant: 67.66,
            omega_matter: 0.3111,
        }
    }

    /// The Hubble distance c/H0 in Mpc.
    #[must_use]
    pub fn hubble_distance(&self) -> f64 {
        SPEED_OF_LIGHT / self.hubble_constant
    }

    /// E(z) = H(z)/H0.
    fn expansion(&self, z: f64) -> f64 {
        let a = 1.0 + z;
        (self.omega_matter * a * a * a + (1.0 - self.omega_matter)).sqrt()
    }

    /// The line-of-sight comoving distance to redshift `z`, in Mpc.
    ///
    /// # Errors
    ///
    /// Fails on a negative or non-finite redshift, or one beyond [`MAX_REDSHIFT`].
    pub fn comoving_distance(&self, z: f64) -> Result<f64, CosmologyError> {
        if !(0.0..=MAX_REDSHIFT).contains(&z) {
            return Err(CosmologyError::Redshift(z));
        }
        let h = z / f64::from(INTERVALS);
        let f = |i: u32| 1.0 / self.expansion(h * f64::from(i));
        let inner: f64 = (1..INTERVALS)
            .map(|i| if i % 2 == 1 { 4.0 * f(i) } else { 2.0 * f(i) })
            .sum();
        Ok(self.hubble_distance() * h / 3.0 * (f(0) + inner + f(INTERVALS)))
    }

    /// The redshift whose comoving distance is `distance` Mpc, found by Newton steps kept
    /// inside a bracket that bisection narrows.
    ///
    /// # Errors
    ///
    /// Fails on a negative distance, or one beyond the distance of [`MAX_REDSHIFT`], where the
    /// search does not converge.
    pub fn redshift_at(&self, distance: f64) -> Result<f64, CosmologyError> {
        if !(distance.is_finite() && distance >= 0.0) {
            return Err(CosmologyError::Distance(distance));
        }
        let (mut low, mut high) = (0.0, MAX_REDSHIFT);
        let mut z = (distance / self.hubble_distance()).min(MAX_REDSHIFT);
        for _ in 0..100 {
            let error = self.comoving_distance(z)? - distance;
            let step = error * self.expansion(z) / self.hubble_distance();
            if step.abs() < 1e-13 * z.max(1.0) {
                return Ok(z);
            }
            if error > 0.0 {
                high = z;
            } else {
                low = z;
            }
            let next = z - step;
            z = if next > low && next < high {
                next
            } else {
                f64::midpoint(low, high)
            };
        }
        Err(CosmologyError::Distance(distance))
    }
}

impl TryFrom<RawCosmology> for Cosmology {
    type Error = CosmologyError;
    fn try_from(raw: RawCosmology) -> Result<Self, CosmologyError> {
        Self::new(raw.name, raw.hubble_constant, raw.omega_matter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn relative(actual: f64, expected: f64) -> f64 {
        ((actual - expected) / expected).abs()
    }

    #[test]
    fn comoving_distances_match_astropy_for_the_same_model() {
        // astropy 8.0.2: FlatLambdaCDM(H0=67.66, Om0=0.3111, Tcmb0=0).comoving_distance(z).
        let planck = Cosmology::planck_2018();
        for (z, expected) in [
            (0.01, 44.205_082),
            (0.1, 432.568_394),
            (0.5, 1_946.482_541),
            (1.0, 3_395.867_124),
            (3.0, 6_505.130_213),
        ] {
            let d = planck.comoving_distance(z).unwrap();
            assert!(
                relative(d, expected) < 1e-6,
                "z = {z}: {d} Mpc, astropy {expected}"
            );
        }
    }

    #[test]
    fn comoving_distances_stay_within_0_1_percent_of_the_full_planck_2018_model() {
        // astropy 8.0.2 Planck18, with radiation and massive neutrinos.
        let planck = Cosmology::planck_2018();
        for (z, expected) in [
            (0.1, 432.565_698),
            (0.5, 1_946.416_635),
            (1.0, 3_395.634_471),
        ] {
            assert!(relative(planck.comoving_distance(z).unwrap(), expected) < 1e-3);
        }
    }

    #[test]
    fn a_redshift_round_trips_through_its_comoving_distance() {
        let planck = Cosmology::planck_2018();
        for z in [0.0, 0.001, 0.023, 0.5, 2.0, 7.5] {
            let back = planck
                .redshift_at(planck.comoving_distance(z).unwrap())
                .unwrap();
            assert!((back - z).abs() < 1e-9, "{z} came back as {back}");
        }
    }

    #[test]
    fn redshifts_and_distances_outside_the_model_are_refused() {
        let planck = Cosmology::planck_2018();
        assert_eq!(
            planck.comoving_distance(-0.1),
            Err(CosmologyError::Redshift(-0.1))
        );
        assert_eq!(
            planck.comoving_distance(10.5),
            Err(CosmologyError::Redshift(10.5))
        );
        assert!(planck.redshift_at(-1.0).is_err());
        let at_limit = planck.comoving_distance(MAX_REDSHIFT).unwrap();
        assert_eq!(
            planck.redshift_at(at_limit + 1.0),
            Err(CosmologyError::Distance(at_limit + 1.0))
        );
    }

    #[test]
    fn a_cosmology_that_cannot_stand_is_refused() {
        assert_eq!(
            Cosmology::new("x", 0.0, 0.3),
            Err(CosmologyError::HubbleConstant(0.0))
        );
        assert_eq!(
            Cosmology::new("x", 70.0, -0.1),
            Err(CosmologyError::MatterDensity(-0.1))
        );
        assert!(Cosmology::new("x", 70.0, 0.3).is_ok());
    }
}
