//! SIMULATED objects and points for the tile tests: made-up positions under the ids of real
//! sources, so that the real definitions name their scale domains and credits.

use univ_ingest::crossmatch::ReleaseObject;
use univ_ingest::source::{ScaleDomain, SourceDefinition};
use univ_ingest::tiles::{BuiltDomain, Context, DomainPoint, TileRules, build_domain};
use univ_model::cosmology::Cosmology;
use univ_model::derive::Release;
use univ_model::entry::CatalogueEntry;
use univ_model::ids::ObjectId;
use univ_model::object::{
    Decided, FractionalUncertainty, MeasurementRef, MethodRank, Object, ObjectEntry, Placement,
    PositionClass, PreferredDistance, PreferredPosition, Resolution,
};
use univ_model::position::Vec3;
use univ_model::value::KindOfValue;

use super::source;

/// Tile rules with these parameters and the repository's statement.
pub fn rules(split_threshold: usize, level_points: usize, max_depth: u8, seed: u64) -> TileRules {
    TileRules {
        statement: "test rules".to_owned(),
        split_threshold,
        split_basis: "test".to_owned(),
        level_points,
        level_points_basis: "test".to_owned(),
        max_depth,
        max_depth_basis: "test".to_owned(),
        seed,
        seed_basis: "test".to_owned(),
    }
}

/// The repository's tile rules.
pub fn repository_rules() -> TileRules {
    TileRules::read(&super::rules_dir().join("tiles.yaml")).unwrap_or_else(|e| panic!("{e}"))
}

/// A build of release 1 under Planck 2018 with `rules` and the repository's definitions.
pub fn context<'a>(
    rules: &'a TileRules,
    cosmology: &'a Cosmology,
    definitions: &'a [SourceDefinition],
) -> Context<'a> {
    Context {
        release: "1",
        cosmology,
        rules,
        definitions,
    }
}

/// A made-up number generator, so the simulated points need no seed of the build's own.
pub struct Lcg(pub u64);

impl Lcg {
    /// A number in [0, 1).
    pub fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        // The top 32 bits, exactly an f64.
        f64::from(u32::try_from(self.0 >> 32).unwrap_or_default()) / 4_294_967_296.0
    }
}

/// SIMULATED: a stars point of object `id` at `position`, its distance and sky position from
/// Gaia DR3.
pub fn point(id: u64, position: Vec3) -> DomainPoint {
    DomainPoint {
        id: ObjectId::new(id).unwrap(),
        position,
        source: source("gaia-dr3-bright"),
        sky_source: source("gaia-dr3-bright"),
        kind: KindOfValue::Measured,
    }
}

/// SIMULATED: `n` stars points, ids 1 to `n`, uniform in a cube of half-edge `half` pc.
pub fn uniform(n: u64, half: f64, seed: u64) -> Vec<DomainPoint> {
    let mut lcg = Lcg(seed);
    (1..=n)
        .map(|id| point(id, [0, 1, 2].map(|_| (2.0 * lcg.next() - 1.0) * half)))
        .collect()
}

/// The stars domain of `points` built under `rules`.
pub fn stars(points: &[DomainPoint], rules: &TileRules) -> BuiltDomain {
    let cosmology = Cosmology::planck_2018();
    let definitions = crate::common::read_sources();
    build_domain(
        ScaleDomain::Stars,
        points,
        &context(rules, &cosmology, &definitions),
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

/// SIMULATED: object `id` of `entries`, placed by `preferred` (of the entry it names) under
/// Planck 2018 along the sky position of its first entry; not placed when `preferred` is none.
pub fn object(
    id: u64,
    entries: Vec<CatalogueEntry>,
    preferred: Option<MeasurementRef>,
) -> ReleaseObject {
    let release = Release::planck_2018();
    let first = entries[0].clone();
    let mut object = Object::new(
        entries
            .into_iter()
            .map(|entry| ObjectEntry {
                entry,
                resolved_by: Resolution::Anchor,
            })
            .collect(),
    );
    let Some(m) = preferred else {
        return ReleaseObject {
            id: ObjectId::new(id).unwrap(),
            object,
            position: None,
        };
    };
    let sky = first.position.as_ref().unwrap();
    let holder = object.entry(m.entry()).unwrap().entry.clone();
    let position = match &m {
        MeasurementRef::Distance { index, .. } => release
            .place(Some(sky), holder.distances.get(*index), None)
            .unwrap()
            .unwrap(),
        MeasurementRef::Parallax { .. } => release
            .place_parallax(sky, holder.parallax.as_ref().unwrap(), None)
            .unwrap(),
        MeasurementRef::Redshift { .. } => unimplemented!("not simulated here"),
    };
    object.preferred_position = Some(PreferredPosition {
        entry: first.designation.clone(),
        class: PositionClass::Survey,
        uncertainty_mas: None,
    });
    object.preferred_distance = Some(PreferredDistance {
        measurement: m,
        rank: MethodRank::PrimaryIndicator,
        distance_mpc: position.distance().value,
        uncertainty: FractionalUncertainty::NoneReported,
        decided: Decided::OnlyCandidate,
        runner_up: None,
    });
    object.placement = Placement::Placed;
    ReleaseObject {
        id: ObjectId::new(id).unwrap(),
        object,
        position: Some(position),
    }
}
