//! Which scale domain each placed object belongs to, and its point there, and its derived ICRS
//! Cartesian position taken into the domain's unit. One of the Galaxy's tracers, the kind its
//! tracer source names (set when its entries are matched), is in Galaxy tracers, whichever
//! entry places it.
//! Otherwise one whose preferred distance comes from a source whose definition names the stars
//! domain is in stars, and any other is in nearby galaxies within [`NEARBY_GALAXIES_DEPTH_MPC`]
//! of the barycentre and in cosmic web beyond, whatever source its distance came from (AC-92,
//! the owner's decisions of 2026-11-03 and 2026-11-10).

use std::collections::BTreeMap;

use univ_model::cosmology::Cosmology;
use univ_model::ids::{ObjectId, SourceId};
use univ_model::object::{MeasurementRef, Object, ObjectKind};
use univ_model::position::{Frame, Vec3};
use univ_model::tile::{ColourBands, PointColour, PointValue};
use univ_model::units::{Unit, scale_domain_unit};
use univ_model::value::KindOfValue;

use super::TilesError;
use crate::crossmatch::ReleaseObject;
use crate::source::{ScaleDomain, SourceDefinition};

/// Every scale domain, in the order of their manifests and RNG streams: Galaxy tracers, added
/// for release 13, last, so that every earlier domain keeps its stream and its tiles.
pub const DOMAINS: [ScaleDomain; 4] = [
    ScaleDomain::Stars,
    ScaleDomain::NearbyGalaxies,
    ScaleDomain::CosmicWeb,
    ScaleDomain::GalaxyTracers,
];

/// The depth of the nearby galaxies domain, Mpc: that of the Updated Nearby Galaxy Catalog
/// (Karachentsev et al. 2013, AJ 145, 101), the deepest `nearby_galaxies` source, the same
/// boundary the structure files follow.
pub const NEARBY_GALAXIES_DEPTH_MPC: f64 = 11.0;

/// The scale domain of an object of `kind` placed `placed_mpc` from the barycentre whose
/// preferred distance's source definition names `source_domain`: one of the Galaxy's tracers,
/// as its tracer source names it, is in Galaxy tracers, and so is any object placed by a tracer
/// source; stars stay stars; any other object is in nearby galaxies within
/// [`NEARBY_GALAXIES_DEPTH_MPC`], and in cosmic web beyond it.
#[must_use]
pub fn domain_of(kind: ObjectKind, source_domain: ScaleDomain, placed_mpc: f64) -> ScaleDomain {
    match source_domain {
        _ if kind.is_tracer() => ScaleDomain::GalaxyTracers,
        ScaleDomain::GalaxyTracers => ScaleDomain::GalaxyTracers,
        ScaleDomain::Stars => ScaleDomain::Stars,
        _ if placed_mpc <= NEARBY_GALAXIES_DEPTH_MPC => ScaleDomain::NearbyGalaxies,
        _ => ScaleDomain::CosmicWeb,
    }
}

/// The unit of a scale domain's positions, univ-model's rule
/// ([`univ_model::units::scale_domain_unit`]): parsecs for stars and Galaxy tracers,
/// megaparsecs otherwise.
#[must_use]
pub fn unit_of(domain: ScaleDomain) -> Unit {
    scale_domain_unit(name_of(domain))
}

/// The symbol of a scale domain's unit, as its manifest gives it.
#[must_use]
pub fn unit_symbol(domain: ScaleDomain) -> &'static str {
    match unit_of(domain) {
        Unit::Parsec => "pc",
        _ => "Mpc",
    }
}

/// The name of a scale domain, as its manifest, its directory and its counts give it.
#[must_use]
pub fn name_of(domain: ScaleDomain) -> &'static str {
    match domain {
        ScaleDomain::Stars => "stars",
        ScaleDomain::NearbyGalaxies => "nearby_galaxies",
        ScaleDomain::CosmicWeb => "cosmic_web",
        ScaleDomain::GalaxyTracers => "galaxy_tracers",
    }
}

/// An object's point in its scale domain.
#[derive(Clone, Debug, PartialEq)]
pub struct DomainPoint {
    pub id: ObjectId,
    /// The object's place among the release's objects.
    pub place: u32,
    /// Its derived ICRS Cartesian position in the domain's unit, the barycentre as origin.
    pub position: Vec3,
    /// The index in its domain's sources of the source of its preferred distance: the source
    /// its tile gives it.
    pub source: u16,
    /// The index in its domain's sources of the source of its sky position.
    pub sky_source: u16,
    /// The kind of value of its preferred distance measurement.
    pub kind: KindOfValue,
    /// The base-10 logarithm of its preferred age in years, for a point of the Galaxy tracers
    /// domain whose entries give one; else the colour index chosen for it, if its entries give
    /// one.
    pub value: Option<PointValue>,
    /// Its object's kind, for a point of the Galaxy tracers domain, whose tiles carry it; none
    /// elsewhere and for one body.
    pub object: Option<ObjectKind>,
}

/// A scale domain's points, in increasing id, and the sources they index.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DomainPoints {
    pub points: Vec<DomainPoint>,
    /// Each source the points name, once, in order of first use by the points in increasing
    /// id, a point's distance source before its sky position's: the manifest's source table.
    pub sources: Vec<SourceId>,
}

/// The release's objects sorted into scale domains.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Assigned {
    pub domains: BTreeMap<ScaleDomain, DomainPoints>,
    /// Objects with no derived position, which are in no tile.
    pub unplaced: usize,
}

/// A placed object before its sources are interned: the indices of its sources' definitions.
struct Placed {
    id: ObjectId,
    place: u32,
    position: Vec3,
    distance: usize,
    sky: usize,
    kind: KindOfValue,
    value: Option<PointValue>,
    object: Option<ObjectKind>,
}

/// The colour index chosen for `object` (build-octree-tiles): a BP−RP its sources publish
/// (Gaia DR3's), else a published B−V (Hipparcos-2's), else a BP−RP derived from two magnitudes
/// (the Gaia Catalogue of Nearby Stars'); of each, the first its entries give.
#[must_use]
pub fn colour_of(object: &Object) -> Option<PointColour> {
    /// The rank of a colour index of `bands`, published or `derived`: lower is preferred.
    fn rank(bands: ColourBands, derived: bool) -> u8 {
        match (bands, derived) {
            (ColourBands::BpRp, false) => 0,
            (ColourBands::BV, false) => 1,
            (ColourBands::BpRp, true) => 2,
            (ColourBands::BV, true) => 3,
        }
    }
    let mut best: Option<(u8, PointColour)> = None;
    for c in object.entries.iter().flat_map(|e| &e.entry.colours) {
        let Some(bands) = ColourBands::of(&c.bands) else {
            continue;
        };
        let r = rank(bands, c.derived_from.is_some());
        if best.is_none_or(|(b, _)| r < b) {
            best = Some((
                r,
                PointColour {
                    index: c.value.normalized.value,
                    bands,
                },
            ));
            if r == 0 {
                break;
            }
        }
    }
    best.map(|(_, c)| c)
}

/// The base-10 logarithm of `object`'s preferred age in years (build-octree-tiles), the first age
/// its entries give that has one: Hunt & Reffert 2023 is the only source of ages, so the first is
/// the preferred. It is taken as published where its source publishes the logarithm; an age in
/// years that is not positive has none.
#[must_use]
pub fn log_age_of(object: &Object) -> Option<f64> {
    object
        .entries
        .iter()
        .flat_map(|e| &e.entry.ages)
        .find_map(|age| {
            let v = &age.value;
            let log = if v.native.unit == Unit::LogYear {
                v.native.value
            } else {
                v.normalized.value.log10()
            };
            log.is_finite().then_some(log)
        })
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
/// in order of id and its sources interned, under `cosmology`, the release's.
///
/// # Errors
///
/// Fails on a placed object whose preferred distance is missing or whose distance or sky
/// position names a source with no definition, whose position was derived under another
/// cosmology, or is not finite; or a domain drawing on more sources than a `u16` can index.
pub fn assign(
    objects: &[ReleaseObject],
    definitions: &[SourceDefinition],
    cosmology: &Cosmology,
) -> Result<Assigned, TilesError> {
    let by_id: BTreeMap<&SourceId, usize> = definitions
        .iter()
        .enumerate()
        .map(|(i, d)| (&d.id, i))
        .collect();
    let definition = |id: ObjectId, source: &SourceId| {
        by_id
            .get(source)
            .copied()
            .ok_or_else(|| TilesError::UnknownSource {
                id,
                source_id: source.clone(),
            })
    };
    let mut placed: BTreeMap<ScaleDomain, Vec<Placed>> = BTreeMap::new();
    let mut unplaced = 0;
    for (place, o) in (0_u32..).zip(objects) {
        let Some(position) = &o.position else {
            unplaced += 1;
            continue;
        };
        let id = o.id;
        let preferred = o
            .object
            .preferred_distance
            .as_ref()
            .ok_or(TilesError::NoPreferred(id))?;
        let distance = definition(id, &preferred.measurement.entry().source)?;
        if position.cosmology() != cosmology {
            return Err(TilesError::Cosmology {
                id,
                found: position.cosmology().name.to_string(),
            });
        }
        let kind = kind_of(&o.object, &preferred.measurement).ok_or(TilesError::NoPreferred(id))?;
        let sky = match &o.object.preferred_position {
            Some(p) => definition(id, &p.entry.source)?,
            None => distance,
        };
        let mpc = position
            .cartesian(Frame::Icrs)
            .ok_or(TilesError::NotFinite(id))?;
        let domain = domain_of(
            o.object.kind,
            definitions[distance].scale_domain,
            position.distance().value,
        );
        // A Galaxy tracer carries its age where it has one, in the place of its colour.
        let age = if domain == ScaleDomain::GalaxyTracers {
            log_age_of(&o.object).map(PointValue::LogAge)
        } else {
            None
        };
        let value = age.or_else(|| colour_of(&o.object).map(PointValue::Colour));
        // A Galaxy tracer carries its kind of object too (build-octree-tiles, version 4).
        let object = (domain == ScaleDomain::GalaxyTracers && o.object.kind != ObjectKind::Body)
            .then_some(o.object.kind);
        let factor = Unit::Megaparsec
            .factor_to(unit_of(domain))
            .map_err(|_| TilesError::NotFinite(id))?;
        let at = mpc.map(|c| c * factor);
        if !at.iter().all(|c| c.is_finite()) {
            return Err(TilesError::NotFinite(id));
        }
        placed.entry(domain).or_default().push(Placed {
            id,
            place,
            position: at,
            distance,
            sky,
            kind,
            value,
            object,
        });
    }
    let mut domains = BTreeMap::new();
    for (domain, mut points) in placed {
        points.sort_by_key(|p| p.id);
        domains.insert(domain, intern(domain, points, definitions)?);
    }
    Ok(Assigned { domains, unplaced })
}

/// The `points` of `domain` with their sources' definitions interned as indices into the
/// domain's sources, in order of first use.
fn intern(
    domain: ScaleDomain,
    points: Vec<Placed>,
    definitions: &[SourceDefinition],
) -> Result<DomainPoints, TilesError> {
    let mut index: Vec<Option<u16>> = vec![None; definitions.len()];
    let mut interned = DomainPoints::default();
    let mut source = |definition: usize, sources: &mut Vec<SourceId>| {
        if let Some(i) = index[definition] {
            return Ok(i);
        }
        let i = u16::try_from(sources.len()).map_err(|_| TilesError::TooManySources {
            domain: name_of(domain).to_owned(),
            sources: sources.len() + 1,
        })?;
        index[definition] = Some(i);
        sources.push(definitions[definition].id.clone());
        Ok(i)
    };
    interned.points.reserve(points.len());
    for p in points {
        let distance = source(p.distance, &mut interned.sources)?;
        let sky = source(p.sky, &mut interned.sources)?;
        interned.points.push(DomainPoint {
            id: p.id,
            place: p.place,
            position: p.position,
            source: distance,
            sky_source: sky,
            kind: p.kind,
            value: p.value,
            object: p.object,
        });
    }
    Ok(interned)
}
