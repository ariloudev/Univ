//! The line of sight of an entry as a pair rule compares it: its heliocentric cz, or its first
//! distance.

use univ_model::cosmology::SPEED_OF_LIGHT;
use univ_model::entry::CatalogueEntry;
use univ_model::units::Unit;

use super::rules::MatchingRules;

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
