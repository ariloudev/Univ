//! The line of sight of an entry as a pair rule compares it: its heliocentric cz, moved back
//! from the CMB frame for a source that publishes only that, or its first distance.

use std::sync::LazyLock;

use univ_model::cosmology::SPEED_OF_LIGHT;
use univ_model::entry::CatalogueEntry;
use univ_model::redshift::{CmbCorrection, DipoleModel};
use univ_model::units::Unit;

use super::rules::MatchingRules;

/// The release's dipole, the one every redshift is moved to the CMB frame with.
static PLANCK: LazyLock<CmbCorrection> =
    LazyLock::new(|| CmbCorrection::new(DipoleModel::Planck2018));

impl MatchingRules {
    /// The entry's heliocentric cz in km/s: its first spectroscopic redshift, else its first
    /// redshift, else its published velocity, which for a source the rules name as publishing
    /// CMB-frame velocities is moved back to the heliocentric frame along its direction.
    #[must_use]
    pub fn velocity(&self, entry: &CatalogueEntry) -> Option<f64> {
        if let Some(r) = entry.preferred_redshift() {
            return Some(r.cmb.heliocentric.value * SPEED_OF_LIGHT);
        }
        let v = entry.velocity.as_ref()?.normalized.value;
        let source = &entry.designation.source;
        if !self
            .cmb_frame_velocities
            .iter()
            .any(|f| &f.source == source)
        {
            return Some(v);
        }
        let direction = entry.position.as_ref()?.icrs.unit_vector();
        Some(PLANCK.heliocentric_of(v / SPEED_OF_LIGHT, direction) * SPEED_OF_LIGHT)
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
