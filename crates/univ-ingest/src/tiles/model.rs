//! The Galaxy model's scale domain (build-the-galaxy-model): its model points sampled from the
//! model's definition along the arms the release holds, those that the release's real objects
//! cover marked, and tiled by the octree rule every scale domain's tiles are, in encoding
//! version 5.

use univ_model::galaxy_model::{
    Counterpart, Counts, Fit, Model, Placement, Stretch, counterpart, covered, sample, stretches,
};
use univ_model::object::ObjectKind;
use univ_model::tile::{ModelPoint, NodeBox, encode_model};
use univ_model::units::Unit;

use super::domain::DomainPoints;
use super::manifest::{Laid, PopulationCode};
use super::{BuiltDomain, Context, TilesError, encoding_of, tile_domain};
use crate::source::ScaleDomain;
use crate::structures::{Extent, Structure, StructureGeometry};

/// What the Galaxy model is built from: its definition and the release's researched structures,
/// whose spiral arms it follows.
#[derive(Clone, Copy, Debug)]
pub struct Galaxy<'a> {
    pub model: &'a Model,
    pub structures: &'a [(String, Structure)],
}

/// The Galaxy model a build sampled: its definition, and the ids of the structures of the arms
/// it followed, in the order of its arm rules.
#[derive(Clone, Debug)]
pub struct SampledModel {
    pub model: Model,
    pub followed: Vec<String>,
}

/// The Galaxy model's domain and what it was sampled from.
#[derive(Clone, Debug)]
pub struct ModelDomain {
    pub built: BuiltDomain,
    pub sampled: SampledModel,
}

/// The kinds of object of the Galaxy tracers that are the young populations' counterparts.
const YOUNG: [ObjectKind; 3] = [
    ObjectKind::LuminousHotStar,
    ObjectKind::OpenCluster,
    ObjectKind::StarFormingRegion,
];

/// The arms' fits among `structures`, each path in the plane of `placement`'s frame.
#[must_use]
pub fn fits(structures: &[(String, Structure)], placement: &Placement) -> Vec<Fit> {
    structures
        .iter()
        .filter_map(|(id, s)| match &s.geometry {
            Some(StructureGeometry::LogSpiral(l)) => Some(Fit {
                structure: id.clone(),
                extrapolated: l.extent == Extent::Extrapolated,
                path: l
                    .path_galactic_kpc()
                    .into_iter()
                    .map(|p| placement.in_plane(p))
                    .collect(),
                width: l
                    .width
                    .as_ref()
                    .and_then(|w| w.quantity.to(Unit::Kiloparsec).ok())
                    .map(|q| q.value),
            }),
            _ => None,
        })
        .collect()
}

/// The real objects of each counterpart in each cube: the stars domain's points and the young
/// Galaxy tracers.
#[must_use]
pub fn counts(stars: Option<&DomainPoints>, tracers: Option<&DomainPoints>) -> Counts {
    let mut counts = Counts::default();
    for p in stars.into_iter().flat_map(|d| &d.points) {
        counts.add(p.position, Counterpart::Stars);
    }
    for p in tracers.into_iter().flat_map(|d| &d.points) {
        if p.object.is_some_and(|k| YOUNG.contains(&k)) {
            counts.add(p.position, Counterpart::YoungTracers);
        }
    }
    counts
}

/// The model's points sampled along `stretches`, each covered as `counts` give, in the order
/// sampled.
///
/// # Errors
///
/// Fails when the model cannot be sampled.
fn model_points(
    model: &Model,
    stretches: &[Stretch],
    counts: &Counts,
) -> Result<Vec<ModelPoint>, TilesError> {
    let sampled = sample(model, stretches).map_err(|e| TilesError::Model(e.to_string()))?;
    // Each population's counterpart by its code, looked up once.
    let mut of_code = [Counterpart::Stars; 256];
    for p in &model.populations {
        of_code[usize::from(p.code)] = counterpart(&p.density);
    }
    let covered = covered(
        sampled.iter().map(|s| (s.position, s.population)),
        |code| of_code[usize::from(code)],
        counts,
    );
    Ok(sampled
        .into_iter()
        .zip(covered)
        .map(|(s, covered)| ModelPoint {
            position: s.position,
            population: s.population,
            covered,
        })
        .collect())
}

/// The Galaxy model's domain: the model of `context`'s Galaxy sampled along its arms, each point
/// covered as `counts` give, tiled under `context`'s rules; none where the build was given no
/// model, or no arm of its rules has a stretch among the release's structures.
///
/// # Errors
///
/// Fails when the model cannot be sampled, a leaf would hold a tile beyond 1 MiB, or a tile
/// cannot be encoded.
pub fn build_model_domain(
    counts: &Counts,
    context: &Context<'_>,
) -> Result<Option<ModelDomain>, TilesError> {
    let Some(galaxy) = &context.galaxy else {
        return Ok(None);
    };
    let model = galaxy.model;
    let placement = Placement::new(model.r0.value, model.sun_height.value);
    let stretches = stretches(&model.arms, &fits(galaxy.structures, &placement));
    if stretches.is_empty() {
        return Ok(None);
    }
    let mut followed: Vec<String> = Vec::new();
    for s in &stretches {
        if !followed.contains(&s.structure) {
            followed.push(s.structure.clone());
        }
    }
    let points = model_points(model, &stretches, counts)?;
    let populations = model.populations.iter().map(PopulationCode::from).collect();
    let built = tile_model(&points, populations, model.seed, context)?;
    Ok(Some(ModelDomain {
        built,
        sampled: SampledModel {
            model: model.clone(),
            followed,
        },
    }))
}

/// The Galaxy model's domain of `points`, in the order sampled, whose populations are
/// `populations`, sampled under `model_seed`: tiled by the octree rule every scale domain's tiles
/// are, under `context`'s rules, in encoding version 5.
///
/// # Errors
///
/// Fails when a leaf would hold a tile beyond 1 MiB, or a tile cannot be encoded.
pub fn tile_model(
    points: &[ModelPoint],
    populations: Vec<PopulationCode>,
    model_seed: u64,
    context: &Context<'_>,
) -> Result<BuiltDomain, TilesError> {
    let domain = ScaleDomain::GalaxyModel;
    let laid = Laid {
        sources: Vec::new(),
        encoding: encoding_of(domain).with_populations(populations),
        model_seed: Some(model_seed),
    };
    let encode = |bounds: &NodeBox, kept: &[usize]| {
        let tile_points: Vec<ModelPoint> = kept.iter().map(|&i| points[i]).collect();
        encode_model(bounds, &tile_points)
    };
    tile_domain(domain, points, laid, encode, context)
}
