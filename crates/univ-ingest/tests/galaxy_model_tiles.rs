//! The Galaxy model's scale domain (build-the-galaxy-model): its model points sampled along the
//! release's arms, the covered ones marked, and tiled in encoding version 5.
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod release_fixture;

use common::repository::{MODEL_POINTS, repository_model};
use common::workspace;
use release_fixture::{build_next, cross, fixture, loaded, tiles};
use univ_ingest::source::ScaleDomain;
use univ_ingest::structures::{Structure, StructureGeometry, read_all_named};
use univ_ingest::tiles::{
    BuiltDomain, Context, DomainPoint, DomainPoints, Galaxy, TileRules, build_model_domain, counts,
};
use univ_model::cosmology::Cosmology;
use univ_model::galaxy_model::{Counterpart, Counts, Model, cube_of};
use univ_model::ids::ObjectId;
use univ_model::object::ObjectKind;
use univ_model::tile::{MODEL_VERSION, TileError, decode, decode_model};
use univ_model::value::KindOfValue;

fn model() -> Model {
    repository_model()
}

fn structures() -> Vec<(String, Structure)> {
    read_all_named(&workspace().join("structures")).unwrap()
}

/// The model's domain built from `structures` with `counts`, under the repository's rules.
fn built(structures: &[(String, Structure)], counts: &Counts) -> Option<BuiltDomain> {
    let rules = TileRules::read(&workspace().join("rules/tiles.yaml")).unwrap();
    let cosmology = Cosmology::planck_2018();
    let model = model();
    let galaxy = Galaxy {
        model: &model,
        structures,
    };
    let context = Context {
        release: "1",
        cosmology: &cosmology,
        rules: &rules,
        definitions: &[],
        galaxy: Some(galaxy),
    };
    build_model_domain(counts, &context)
        .unwrap()
        .map(|d| d.built)
}

/// Every point of `domain`'s tiles, decoded.
fn decoded(domain: &BuiltDomain) -> Vec<univ_model::tile::DecodedModelPoint> {
    domain
        .nodes
        .iter()
        .flat_map(|n| decode_model(&n.tile).unwrap().points)
        .collect()
}

#[test]
fn the_model_domain_is_in_parsecs_with_its_own_octree_and_only_its_tiles_take_version_5() {
    let f = fixture();
    let crossed = cross(&f);
    let built = tiles(&f, &crossed);
    let model = built
        .domains
        .iter()
        .find(|d| d.domain == ScaleDomain::GalaxyModel)
        .unwrap();
    let m = &model.manifest;
    assert_eq!(m.scale_domain, "galaxy_model");
    assert_eq!(m.unit, "pc");
    assert_eq!(m.encoding.version, MODEL_VERSION);
    assert_eq!(m.points, usize::try_from(MODEL_POINTS).unwrap());
    assert!(m.sources.is_empty(), "a model point is no source's");
    // Its own root, reaching the stellar halo's 100 kpc: a half-edge of 131,072 pc, a power of
    // two, where the stars domain's reaches its stars.
    let root = &model.nodes[0];
    assert_eq!(root.address, "r");
    assert!(
        (root.bounds.edge() - 262_144.0).abs() < 1e-9,
        "{:?}",
        root.bounds
    );
    let stars = built
        .domains
        .iter()
        .find(|d| d.domain == ScaleDomain::Stars)
        .unwrap();
    assert!(stars.nodes[0].bounds.edge() < root.bounds.edge());
    for n in &model.nodes {
        assert_eq!(
            u16::from_le_bytes([n.tile[4], n.tile[5]]),
            MODEL_VERSION,
            "{}",
            n.address
        );
        assert!(matches!(decode(&n.tile), Err(TileError::ModelVersion(_))));
    }
    for d in built
        .domains
        .iter()
        .filter(|d| d.domain != ScaleDomain::GalaxyModel)
    {
        assert_ne!(d.manifest.encoding.version, MODEL_VERSION);
        for n in &d.nodes {
            decode(&n.tile).unwrap();
        }
    }
}

#[test]
fn the_model_seed_is_in_its_manifest_and_building_again_gives_the_same_tiles() {
    let structures = structures();
    let (a, b) = (
        built(&structures, &Counts::default()).unwrap(),
        built(&structures, &Counts::default()).unwrap(),
    );
    assert_eq!(a.manifest.sampling.model_seed, Some(model().seed));
    assert_eq!(a.json, b.json);
    let digests = |d: &BuiltDomain| d.nodes.iter().map(|n| n.sha256.clone()).collect::<Vec<_>>();
    assert_eq!(digests(&a), digests(&b));
}

#[test]
fn the_manifest_names_each_population_by_the_code_its_points_carry() {
    let d = built(&structures(), &Counts::default()).unwrap();
    let names: Vec<(u8, &str)> = d
        .manifest
        .encoding
        .populations
        .iter()
        .flatten()
        .map(|p| (p.code, p.id.as_str()))
        .collect();
    assert_eq!(names[0], (1, "thin_disc"));
    assert_eq!(names.len(), 6);
    for p in decoded(&d) {
        assert!(names.iter().any(|(code, _)| *code == p.population));
    }
}

#[test]
fn covered_model_points_are_marked_and_stay_in_their_tiles() {
    let structures = structures();
    let bare = decoded(&built(&structures, &Counts::default()).unwrap());
    assert!(bare.iter().all(|p| !p.covered));
    // The Sun's cube, ICRS parsecs about the barycentre: as many stars as model points.
    let mut counts = Counts::default();
    for _ in 0..MODEL_POINTS {
        counts.add([1.0, 1.0, 1.0], Counterpart::Stars);
    }
    let d = built(&structures, &counts).unwrap();
    let points = decoded(&d);
    assert_eq!(
        points.len(),
        bare.len(),
        "a covered point stays in its tile"
    );
    let sun = cube_of([1.0, 1.0, 1.0]);
    let in_cube = |p: &univ_model::tile::DecodedModelPoint| {
        // Quantization moves a point by far less than a cube.
        cube_of(p.position) == sun
    };
    let covered: Vec<_> = points.iter().filter(|p| p.covered).collect();
    assert!(!covered.is_empty(), "the disc reaches the Sun's cube");
    assert!(covered.iter().all(|p| in_cube(p)));
    // The arms' young stars count young tracers, not stars: none of them is covered.
    assert!(covered.iter().all(|p| p.population <= 4), "{covered:?}");
}

/// A point of `kind` at `position`, as a domain gives it.
fn point(position: [f64; 3], object: Option<ObjectKind>) -> DomainPoint {
    DomainPoint {
        id: ObjectId::new(1).unwrap(),
        place: 0,
        position,
        source: 0,
        sky_source: 0,
        kind: KindOfValue::Measured,
        value: None,
        object,
    }
}

#[test]
fn the_young_populations_count_young_tracers_and_the_others_count_stars() {
    let at = [100.0, 100.0, 100.0];
    let stars = DomainPoints {
        points: vec![point(at, None), point(at, None)],
        sources: vec![],
    };
    let tracers = DomainPoints {
        points: [
            ObjectKind::LuminousHotStar,
            ObjectKind::OpenCluster,
            ObjectKind::StarFormingRegion,
            ObjectKind::MolecularCloud,
            ObjectKind::GlobularCluster,
        ]
        .into_iter()
        .map(|k| point(at, Some(k)))
        .collect(),
        sources: vec![],
    };
    let c = counts(Some(&stars), Some(&tracers));
    assert_eq!(c.get(cube_of(at), Counterpart::Stars), 2);
    assert_eq!(c.get(cube_of(at), Counterpart::YoungTracers), 3);
}

#[test]
fn a_release_without_spiral_arms_has_no_galaxy_model() {
    let none: Vec<(String, Structure)> = structures()
        .into_iter()
        .filter(|(_, s)| !matches!(s.geometry, Some(StructureGeometry::LogSpiral(_))))
        .collect();
    assert!(built(&none, &Counts::default()).is_none());
}

#[test]
fn the_release_holds_the_definition_it_sampled_each_population_named_by_its_code() {
    let f = fixture();
    let release = build_next(&f);
    let db = loaded(&release.dir, &release.manifest);
    let (domain, points, seed, definition, followed): (String, i64, i64, String, String) = db
        .query_row(
            "SELECT domain, points, seed, definition, followed FROM galaxy_models \
             WHERE release = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .unwrap();
    assert_eq!(domain, "galaxy_model");
    assert_eq!(points, i64::try_from(MODEL_POINTS).unwrap());
    assert_eq!(seed, i64::try_from(model().seed).unwrap());
    let held: Model = serde_json::from_str(&definition).unwrap();
    assert_eq!(held, model());
    let codes: Vec<(u8, &str)> = held
        .populations
        .iter()
        .map(|p| (p.code, p.id.as_str()))
        .collect();
    assert_eq!(codes[0], (1, "thin_disc"));
    assert_eq!(codes[5], (6, "star_forming_knots"));
    // Every number gives its citation or its reason.
    assert!(definition.contains("\"from\":\"cited\""));
    assert!(definition.contains("\"from\":\"choice\""));
    // The arms it followed: Reid et al. 2019's among them, each a structure of the release.
    let followed: Vec<String> = serde_json::from_str(&followed).unwrap();
    assert!(
        followed.contains(&"reid-2019-perseus-arm".to_owned()),
        "{followed:?}"
    );
    for id in &followed {
        assert!(f.structures.iter().any(|(s, _)| s == id), "{id}");
    }
}
