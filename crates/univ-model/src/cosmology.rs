//! Flat ΛCDM cosmology: the comoving distance of a redshift and its inverse.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The speed of light in km/s.
pub const SPEED_OF_LIGHT: f64 = 299_792.458;

/// Simpson intervals per integration: the error is far below 1e-9 of the result for z ≤ 10.
const INTERVALS: u32 = 256;
/// The largest redshift the inverse searches to.
const MAX_REDSHIFT: f64 = 1_100.0;

/// A flat ΛCDM cosmology without radiation, as a release names it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cosmology {
    /// Its name and citation, such as "Planck 2018 (A&A 641, A6), TT,TE,EE+lowE+lensing+BAO".
    pub name: String,
    /// The Hubble constant in km/s/Mpc.
    pub hubble_constant: f64,
    /// The matter density today; Λ makes up the rest.
    pub omega_matter: f64,
}

/// A distance or redshift outside what the cosmology can answer.
#[derive(Debug, Error, PartialEq)]
pub enum CosmologyError {
    #[error("the redshift {0} is negative or not a finite number")]
    Redshift(f64),
    #[error("the comoving distance {0} Mpc is negative or lies beyond z = {MAX_REDSHIFT}")]
    Distance(f64),
}

impl Cosmology {
    /// Planck 2018, Table 2, last column (Planck Collaboration 2020, A&A 641, A6): H0 = 67.66
    /// km/s/Mpc and Ωm = 0.3111.
    #[must_use]
    pub fn planck_2018() -> Self {
        Self {
            name: "Planck 2018 (A&A 641, A6, Table 2: TT,TE,EE+lowE+lensing+BAO)".to_owned(),
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
    /// Fails on a negative or non-finite redshift.
    pub fn comoving_distance(&self, z: f64) -> Result<f64, CosmologyError> {
        if !(z.is_finite() && z >= 0.0) {
            return Err(CosmologyError::Redshift(z));
        }
        let h = z / f64::from(INTERVALS);
        let f = |i: u32| 1.0 / self.expansion(h * f64::from(i));
        let inner: f64 = (1..INTERVALS)
            .map(|i| if i % 2 == 1 { 4.0 * f(i) } else { 2.0 * f(i) })
            .sum();
        Ok(self.hubble_distance() * h / 3.0 * (f(0) + inner + f(INTERVALS)))
    }

    /// The redshift whose comoving distance is `distance` Mpc, found by safeguarded Newton steps.
    ///
    /// # Errors
    ///
    /// Fails on a negative distance or one beyond the distance of z = 1100.
    pub fn redshift_at(&self, distance: f64) -> Result<f64, CosmologyError> {
        let beyond = || CosmologyError::Distance(distance);
        if !(distance.is_finite() && distance >= 0.0) {
            return Err(beyond());
        }
        if distance > self.comoving_distance(MAX_REDSHIFT)? {
            return Err(beyond());
        }
        let (mut low, mut high) = (0.0, MAX_REDSHIFT);
        let mut z = distance / self.hubble_distance();
        for _ in 0..100 {
            let error = self.comoving_distance(z)? - distance;
            let step = error * self.expansion(z) / self.hubble_distance();
            if step.abs() < 1e-13 * z.max(1.0) {
                break;
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
        Ok(z)
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
        assert!(planck.redshift_at(-1.0).is_err());
        assert!(planck.redshift_at(20_000.0).is_err());
    }
}
