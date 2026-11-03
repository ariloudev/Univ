//! An object's derived position: its preferred distance along the direction of its preferred
//! sky position, under the release cosmology.

use univ_model::derive::{DerivedPosition, Release};
use univ_model::distance::{Distance, DistanceKind, Measure, Method};
use univ_model::entry::{CatalogueEntry, RedshiftKind};
use univ_model::object::{MeasurementRef, NotPlacedReason, Object, Placement};
use univ_model::redshift::CmbRedshift;
use univ_model::units::Unit;
use univ_model::value::{KindOfValue, Quantity, Uncertainty};

use crate::ranking::fraction;

/// Places a ranked object under `release`, giving its derived position. An object that has a
/// preferred distance but no sky position, or whose distance cannot be derived, is marked not
/// placed with the reason; one without a preferred distance stays as ranking left it.
pub fn place(object: &mut Object, release: &Release) -> Option<DerivedPosition> {
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
        .and_then(|e| derive(release, position, &e.entry, &preferred.measurement));
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
    position: &univ_model::entry::Position,
    entry: &CatalogueEntry,
    measurement: &MeasurementRef,
) -> Option<DerivedPosition> {
    let placed = match measurement {
        MeasurementRef::Distance { index, .. } => {
            release.place(Some(position), entry.distances.get(*index), cmb_of(entry))
        }
        MeasurementRef::Redshift { index, .. } => {
            let z = entry.redshifts.get(*index)?;
            release.place(Some(position), None, Some(&z.cmb))
        }
        MeasurementRef::Parallax { .. } => {
            release.place(Some(position), Some(&inverted_parallax(entry)?), None)
        }
    };
    placed.ok().flatten()
}

/// The CMB-frame redshift that makes a luminosity distance of `entry` comoving: its first
/// spectroscopic redshift, else its first.
fn cmb_of(entry: &CatalogueEntry) -> Option<&CmbRedshift> {
    entry
        .redshifts
        .iter()
        .find(|z| z.kind == RedshiftKind::Spectroscopic)
        .or_else(|| entry.redshifts.first())
        .map(|z| &z.cmb)
}

/// The distance of a parallax the ranking found fine enough to invert, `1 / ϖ`, inferred from
/// the measured parallax: its error carried to first order, `σ_d = d σ_ϖ / ϖ`, a Euclidean
/// distance. The parallax itself stays in the entry as published.
fn inverted_parallax(entry: &CatalogueEntry) -> Option<Distance> {
    let parallax = &entry.parallax.as_ref()?.normalized;
    let arcsec = parallax.to(Unit::Arcsecond).ok()?.value;
    if arcsec <= 0.0 {
        return None;
    }
    let parsecs = 1.0 / arcsec;
    let uncertainty = fraction(parallax).map_or(Uncertainty::NoneReported, |f| {
        Uncertainty::Symmetric { sigma: parsecs * f }
    });
    let native = Quantity::new(parsecs, Unit::Parsec, uncertainty).ok()?;
    let how = (DistanceKind::Parallax, Method::TrigonometricParallax);
    let (kind, measure) = (KindOfValue::Inferred, Measure::Euclidean);
    Distance::new(native, None, Unit::Parsec, kind, how, measure, None).ok()
}
