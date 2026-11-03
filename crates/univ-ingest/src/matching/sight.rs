//! The line of sight of an entry as a pair rule compares it: its heliocentric cz, or its first
//! distance; and the wider angular tolerance a pair may give two nearby entries.

use serde::{Deserialize, Serialize};
use univ_model::cosmology::SPEED_OF_LIGHT;
use univ_model::entry::CatalogueEntry;
use univ_model::units::Unit;

use super::rules::{MatchingRules, PairRule};

/// A wider angular tolerance for two entries whose heliocentric cz both lie below `below_km_s`:
/// nearby galaxies are large on the sky, and catalogues adopt centres farther apart.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NearbyTolerance {
    pub below_km_s: f64,
    pub angular_tolerance_arcsec: f64,
    pub basis: String,
}

impl NearbyTolerance {
    /// Whether it can stand beside a pair's `tolerance`: a positive velocity, a finite tolerance
    /// wider than the pair's, and a basis.
    #[must_use]
    pub fn widens(&self, tolerance: f64) -> bool {
        self.below_km_s.is_finite()
            && self.below_km_s > 0.0
            && self.angular_tolerance_arcsec.is_finite()
            && self.angular_tolerance_arcsec > tolerance
            && !self.basis.trim().is_empty()
    }
}

impl MatchingRules {
    /// The entry's heliocentric cz in km/s: its first spectroscopic redshift, else its first
    /// redshift, else its published velocity. A redshift published in the CMB frame, such as
    /// Cosmicflows-4's Vcmb, compares by the heliocentric redshift the normalizer derived back
    /// from it with the run's dipole.
    #[must_use]
    pub fn velocity(&self, entry: &CatalogueEntry) -> Option<f64> {
        if let Some(r) = entry.preferred_redshift() {
            return Some(r.cmb.heliocentric.value * SPEED_OF_LIGHT);
        }
        entry.velocity.as_ref().map(|v| v.normalized.value)
    }

    /// The angular tolerance of `pair` for `a` and `b`, arcseconds: its nearby tolerance when
    /// it gives one and both entries' cz lie below its velocity, else its own.
    #[must_use]
    pub fn angular_tolerance(
        &self,
        pair: &PairRule,
        a: &CatalogueEntry,
        b: &CatalogueEntry,
    ) -> f64 {
        match &pair.nearby {
            Some(n)
                if [a, b]
                    .iter()
                    .all(|e| self.velocity(e).is_some_and(|v| v < n.below_km_s)) =>
            {
                n.angular_tolerance_arcsec
            }
            _ => pair.angular_tolerance_arcsec,
        }
    }
}

/// The entry's first distance in Mpc.
#[must_use]
pub fn distance(entry: &CatalogueEntry) -> Option<f64> {
    let d = entry.distances.first()?;
    d.normalized
        .to(Unit::Megaparsec)
        .ok()
        .map(|q| q.value)
        .filter(|v| *v > 0.0)
}
