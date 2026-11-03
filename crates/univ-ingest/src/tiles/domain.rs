//! Which scale domain each placed object belongs to, and its point there: the source definition
//! of the catalogue entry holding its preferred distance names the domain, and its derived
//! ICRS Cartesian position is taken into the domain's unit.

use std::collections::BTreeMap;

use univ_model::cosmology::Cosmology;
use univ_model::ids::{ObjectId, SourceId};
use univ_model::object::{MeasurementRef, Object};
use univ_model::position::{Frame, Vec3};
use univ_model::units::Unit;
use univ_model::value::KindOfValue;

use super::TilesError;
use crate::crossmatch::ReleaseObject;
use crate::source::{ScaleDomain, SourceDefinition};

/// Every scale domain, in the order of their manifests and RNG streams.
pub const DOMAINS: [ScaleDomain; 3] = [
    ScaleDomain::Stars,
    ScaleDomain::NearbyGalaxies,
    ScaleDomain::CosmicWeb,
];

/// The unit of a scale domain's positions: parsecs for stars, megaparsecs otherwise.
#[must_use]
pub fn unit_of(domain: ScaleDomain) -> Unit {
    match domain {
        ScaleDomain::Stars => Unit::Parsec,
        ScaleDomain::NearbyGalaxies | ScaleDomain::CosmicWeb => Unit::Megaparsec,
    }
}

/// The symbol of a scale domain's unit, as its manifest gives it.
#[must_use]
pub fn unit_symbol(domain: ScaleDomain) -> &'static str {
    match domain {
        ScaleDomain::Stars => "pc",
        ScaleDomain::NearbyGalaxies | ScaleDomain::CosmicWeb => "Mpc",
    }
}

/// The name of a scale domain, as its manifest, its directory and its counts give it.
#[must_use]
pub fn name_of(domain: ScaleDomain) -> &'static str {
    match domain {
        ScaleDomain::Stars => "stars",
        ScaleDomain::NearbyGalaxies => "nearby_galaxies",
        ScaleDomain::CosmicWeb => "cosmic_web",
    }
}

/// An object's point in its scale domain.
#[derive(Clone, Debug, PartialEq)]
pub struct DomainPoint {
    pub id: ObjectId,
    /// Its derived ICRS Cartesian position in the domain's unit, the barycentre as origin.
    pub position: Vec3,
    /// The source of the entry holding its preferred distance: the source its tiles give it.
    pub source: SourceId,
    /// The source of the entry whose sky position it takes, credited in the manifest too.
    pub sky_source: SourceId,
    /// The kind of value of its preferred distance measurement.
    pub kind: KindOfValue,
}

/// The release's objects sorted into scale domains.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Assigned {
    pub domains: BTreeMap<ScaleDomain, Vec<DomainPoint>>,
    /// Objects with no derived position, which are in no tile.
    pub unplaced: usize,
}

/// The kind of value of the measurement `m` of `object`.
fn kind_of(object: &Object, m: &MeasurementRef) -> Option<KindOfValue> {
    let entry = &object.entry(m.entry())?.entry;
    match m {
        MeasurementRef::Distance { index, .. } => {
            entry.distances.get(*index).map(|d| d.kind_of_value)
        }
        MeasurementRef::Redshift { index, .. } => {
            entry.redshifts.get(*index).map(|z| z.kind_of_value)
        }
        MeasurementRef::Parallax { .. } => entry.parallax.as_ref().map(|p| p.kind),
    }
}

/// Sorts the placed `objects` into the scale domains `definitions` name, each domain's points
/// in order of id, under `cosmology`, the release's.
///
/// # Errors
///
/// Fails on a placed object whose preferred distance is missing or names a source with no
/// definition, whose position was derived under another cosmology, or is not finite.
pub fn assign(
    objects: &[ReleaseObject],
    definitions: &[SourceDefinition],
    cosmology: &Cosmology,
) -> Result<Assigned, TilesError> {
    let domain_of: BTreeMap<&SourceId, ScaleDomain> = definitions
        .iter()
        .map(|d| (&d.id, d.scale_domain))
        .collect();
    let mut assigned = Assigned::default();
    for o in objects {
        let Some(position) = &o.position else {
            assigned.unplaced += 1;
            continue;
        };
        let id = o.id;
        let preferred = o
            .object
            .preferred_distance
            .as_ref()
            .ok_or(TilesError::NoPreferred(id))?;
        let source = preferred.measurement.entry().source.clone();
        let domain = *domain_of
            .get(&source)
            .ok_or_else(|| TilesError::UnknownSource {
                id,
                source_id: source.clone(),
            })?;
        if position.cosmology() != cosmology {
            return Err(TilesError::Cosmology {
                id,
                found: position.cosmology().name.to_string(),
            });
        }
        let kind = kind_of(&o.object, &preferred.measurement).ok_or(TilesError::NoPreferred(id))?;
        let sky_source = o
            .object
            .preferred_position
            .as_ref()
            .map_or_else(|| source.clone(), |p| p.entry.source.clone());
        let factor = Unit::Megaparsec
            .factor_to(unit_of(domain))
            .map_err(|_| TilesError::NotFinite(id))?;
        let mpc = position
            .cartesian(Frame::Icrs)
            .ok_or(TilesError::NotFinite(id))?;
        let at = mpc.map(|c| c * factor);
        if !at.iter().all(|c| c.is_finite()) {
            return Err(TilesError::NotFinite(id));
        }
        assigned
            .domains
            .entry(domain)
            .or_default()
            .push(DomainPoint {
                id,
                position: at,
                source,
                sky_source,
                kind,
            });
    }
    for points in assigned.domains.values_mut() {
        points.sort_by_key(|p| p.id);
    }
    Ok(assigned)
}
