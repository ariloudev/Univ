//! An object's derived position: its preferred distance along the direction of its preferred
//! sky position, under the release cosmology.

use univ_model::derive::{DerivedPosition, Release};
use univ_model::entry::CatalogueEntry;
use univ_model::object::{MeasurementRef, NotPlacedReason, Object, Placement};

use crate::ranking::RankingRules;

/// Places a ranked object under `release`, giving its derived position. An object that has a
/// preferred distance but no sky position, or whose distance cannot be derived, is marked not
/// placed with the reason; one without a preferred distance stays as ranking left it.
pub fn place(
    object: &mut Object,
    release: &Release,
    rules: &RankingRules,
) -> Option<DerivedPosition> {
    let preferred = object.preferred_distance.as_ref()?;
    let position = object
        .preferred_position
        .as_ref()
        .and_then(|p| object.entry(&p.entry))
        .and_then(|e| e.entry.position.as_ref());
    let Some(position) = position else {
        object.placement = not_placed(NotPlacedReason::NoSkyPosition);
        return None;
    };
    let derived = object
        .entry(preferred.measurement.entry())
        .and_then(|e| derive(release, rules, position, &e.entry, &preferred.measurement));
    if derived.is_none() {
        object.placement = not_placed(NotPlacedReason::NotDerivable);
    }
    derived
}

fn not_placed(reason: NotPlacedReason) -> Placement {
    Placement::NotPlaced { reason }
}

fn derive(
    release: &Release,
    rules: &RankingRules,
    position: &univ_model::entry::Position,
    entry: &CatalogueEntry,
    measurement: &MeasurementRef,
) -> Option<DerivedPosition> {
    let placed = match measurement {
        MeasurementRef::Distance { index, .. } => release.place(
            Some(position),
            entry.distances.get(*index),
            entry.preferred_redshift(),
        ),
        MeasurementRef::Redshift { index, .. } => {
            release.place(Some(position), None, entry.redshifts.get(*index))
        }
        MeasurementRef::Parallax { .. } => {
            let zero_point = rules.zero_point_of(&entry.designation.source);
            return release
                .place_parallax(position, entry.parallax.as_ref()?, zero_point)
                .ok();
        }
    };
    placed.ok().flatten()
}
