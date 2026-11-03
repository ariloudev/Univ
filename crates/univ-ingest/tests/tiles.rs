//! The octree of each scale domain (build-octree-tiles, RQ-36): its root and unit, the domain
//! of each object, the splits, the levels of detail and their seed. Every object and point here
//! is SIMULATED.
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod simulated;

use std::collections::{BTreeMap, BTreeSet};

use simulated::tiles::{Lcg, context, object, point, rules, stars, uniform};
use simulated::{entry, with_distance, with_parallax};
use univ_ingest::tiles::{BuiltDomain, build};
use univ_model::cosmology::Cosmology;
use univ_model::distance::{DistanceKind, Method};
use univ_model::ids::ObjectId;
use univ_model::object::MeasurementRef;
use univ_model::position::dot;
use univ_model::tile::decode;

const CEPHEIDS: (DistanceKind, Method) = (DistanceKind::PrimaryIndicator, Method::Cepheids);

/// The ids each node's tile holds, decoded from the tile, by address.
fn decoded_ids(domain: &BuiltDomain) -> BTreeMap<String, Vec<ObjectId>> {
    domain
        .nodes
        .iter()
        .map(|n| {
            let tile = decode(&n.tile).unwrap();
            (
                n.address.clone(),
                tile.points.iter().map(|p| p.id).collect(),
            )
        })
        .collect()
}

/// SIMULATED: a Gaia star at 10 mas, a nearby galaxy at 50 kpc, a Cosmicflows galaxy at 80 Mpc
/// and a 2MRS galaxy with no distance, which cannot be placed.
fn release_objects() -> Vec<univ_ingest::crossmatch::ReleaseObject> {
    let star = with_parallax(entry("gaia-dr3-bright", "star", 10.0, 20.0), 10.0, 0.01);
    let lmc = with_distance(
        entry("updated-nearby-galaxies", "lmc", 80.9, -69.8),
        0.050,
        Some(0.002),
        CEPHEIDS,
    );
    let far = with_distance(
        entry("cosmicflows-4", "far", 200.0, -10.0),
        80.0,
        Some(4.0),
        CEPHEIDS,
    );
    let parallax = |e: &univ_model::entry::CatalogueEntry| MeasurementRef::Parallax {
        entry: e.designation.clone(),
    };
    let distance = |e: &univ_model::entry::CatalogueEntry| MeasurementRef::Distance {
        entry: e.designation.clone(),
        index: 0,
    };
    vec![
        object(4, vec![entry("twomrs", "lost", 30.0, 30.0)], None),
        object(1, vec![star.clone()], Some(parallax(&star))),
        object(2, vec![lmc.clone()], Some(distance(&lmc))),
        object(3, vec![far.clone()], Some(distance(&far))),
    ]
}

#[test]
fn each_scale_domain_has_its_own_root_stars_in_parsecs_galaxies_in_megaparsecs() {
    let rules = rules(100, 50, 8, 7);
    let (cosmology, definitions) = (Cosmology::planck_2018(), common::read_sources());
    let built = build(
        &release_objects(),
        &context(&rules, &cosmology, &definitions),
    )
    .unwrap();
    let names: Vec<_> = built
        .domains
        .iter()
        .map(|d| d.manifest.scale_domain)
        .collect();
    assert_eq!(names, ["stars", "nearby_galaxies", "cosmic_web"]);
    for (domain, unit, id, radius) in [
        (0, "pc", 1, 100.0),
        (1, "Mpc", 2, 0.050),
        (2, "Mpc", 3, 80.0),
    ] {
        let d = &built.domains[domain];
        assert_eq!(d.manifest.unit, unit);
        assert_eq!(d.nodes.len(), 1, "one point, one root");
        let root = &d.nodes[0];
        assert_eq!((root.address.as_str(), root.level), ("r", 0));
        assert_eq!(root.ids, [ObjectId::new(id).unwrap()]);
        let r = d.manifest.radius;
        assert!((r - radius).abs() < radius * 1e-3, "{unit}: {r}");
        // A cube centred on the barycentre, its half-edge the power of two just above the
        // largest coordinate.
        let (min, edge) = (root.bounds.min(), root.bounds.edge());
        assert_eq!(min, [-edge / 2.0; 3]);
        assert_eq!((edge / 2.0).log2().fract(), 0.0, "a power of two: {edge}");
        let decoded = decode(&root.tile).unwrap().points[0].position;
        let largest = decoded.iter().fold(0.0_f64, |m, c| m.max(c.abs()));
        assert!(largest <= edge / 2.0 && largest > edge / 4.0 * 0.999);
        assert!((dot(decoded, decoded).sqrt() - r).abs() < root.bounds.step());
    }
    assert_eq!(built.unplaced, 1);
    let every: Vec<ObjectId> = built
        .domains
        .iter()
        .flat_map(|d| decoded_ids(d).into_values().flatten())
        .collect();
    assert!(
        !every.contains(&ObjectId::new(4).unwrap()),
        "an object not placed is in no tile (AC-65)"
    );
}

#[test]
fn an_object_belongs_to_the_domain_of_the_entry_holding_its_preferred_distance() {
    // SIMULATED: one galaxy listed by UNGC (nearby galaxies) and Cosmicflows-4 (cosmic web);
    // object 1 is placed by the UNGC distance, object 2 by the Cosmicflows one.
    let ungc = with_distance(
        entry("updated-nearby-galaxies", "g", 150.0, 5.0),
        3.0,
        Some(0.1),
        CEPHEIDS,
    );
    let cf4 = with_distance(
        entry("cosmicflows-4", "g", 150.0, 5.0),
        3.2,
        Some(0.3),
        CEPHEIDS,
    );
    let by = |e: &univ_model::entry::CatalogueEntry| {
        Some(MeasurementRef::Distance {
            entry: e.designation.clone(),
            index: 0,
        })
    };
    let objects = vec![
        object(1, vec![ungc.clone(), cf4.clone()], by(&ungc)),
        object(2, vec![ungc.clone(), cf4.clone()], by(&cf4)),
    ];
    let rules = rules(100, 50, 8, 7);
    let (cosmology, definitions) = (Cosmology::planck_2018(), common::read_sources());
    let built = build(&objects, &context(&rules, &cosmology, &definitions)).unwrap();
    let ids = |d: usize| -> Vec<u64> {
        decoded_ids(&built.domains[d])
            .into_values()
            .flatten()
            .map(ObjectId::get)
            .collect()
    };
    assert_eq!(ids(0), Vec::<u64>::new());
    assert_eq!(ids(1), [1]);
    assert_eq!(ids(2), [2]);
    let cosmic = &built.domains[2].manifest;
    let point_source = decode(&built.domains[2].nodes[0].tile).unwrap().points[0].source;
    assert_eq!(
        cosmic.sources[usize::from(point_source)].id,
        "cosmicflows-4"
    );
    let credited: Vec<_> = cosmic.sources.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        credited,
        ["cosmicflows-4", "updated-nearby-galaxies"],
        "the sky position's source is credited too"
    );
}

#[test]
fn a_node_splits_above_the_threshold_down_to_the_maximum_depth() {
    // SIMULATED: 1,000 points spread over 512 pc and 600 more within 1 pc of one place, so
    // that some cells at the maximum depth still hold more than the threshold.
    let mut points = uniform(1_000, 256.0, 3);
    let mut lcg = Lcg(9);
    for id in 1_001..=1_600 {
        points.push(point(id, [0, 1, 2].map(|_| 100.0 + lcg.next())));
    }
    let (threshold, kept, depth) = (100, 40, 3);
    let built = stars(&points, &rules(threshold, kept, depth, 1));
    let addresses: BTreeSet<&str> = built.nodes.iter().map(|n| n.address.as_str()).collect();
    let has_children = |a: &str| (0..8).any(|o| addresses.contains(format!("{a}{o}").as_str()));
    let mut deep_and_full = 0;
    for n in &built.nodes {
        assert!(n.level <= depth);
        assert_eq!(n.address.len(), usize::from(n.level) + 1);
        if n.sampled_from > threshold && n.level < depth {
            assert!(has_children(&n.address), "{} is split", n.address);
            assert_eq!(n.ids.len(), kept);
        } else {
            assert!(!has_children(&n.address), "{} is a leaf", n.address);
            assert_eq!(n.ids.len(), n.sampled_from, "a leaf keeps every point");
            if n.sampled_from > threshold {
                deep_and_full += 1;
            }
        }
    }
    assert!(
        deep_and_full > 0,
        "a cell at the maximum depth over the threshold"
    );
    let total: usize = built.nodes.iter().map(|n| n.ids.len()).sum();
    assert_eq!(total, 1_600);
}

#[test]
fn each_level_is_an_even_random_subsample_and_a_finer_level_repeats_no_point() {
    let points = uniform(20_000, 1_000.0, 5);
    let built = stars(&points, &rules(4_000, 2_000, 6, 11));
    let mut seen = BTreeSet::new();
    for (address, ids) in decoded_ids(&built) {
        for id in ids {
            assert!(seen.insert(id), "{id} repeated in {address}");
        }
    }
    assert_eq!(seen.len(), 20_000, "every point in exactly one node");
    for n in &built.nodes {
        let parent = &n.address[..n.address.len() - 1];
        if let Some(p) = built.nodes.iter().find(|p| p.address == parent) {
            assert!(p.ids.iter().all(|id| !n.ids.contains(id)));
        }
    }
    // The root keeps 2,000 of 20,000 uniform points, each equally likely: about 250 in each
    // octant and a mean id near the middle, within four standard deviations.
    let root = &built.nodes[0];
    assert_eq!((root.ids.len(), root.sampled_from), (2_000, 20_000));
    let mut octants = [0_u32; 8];
    for id in &root.ids {
        let p = points[usize::try_from(id.get()).unwrap() - 1].position;
        octants[usize::from(root.bounds.octant_of(p))] += 1;
    }
    for n in octants {
        assert!((190..=310).contains(&n), "{octants:?}");
    }
    let sum: u64 = root.ids.iter().map(|i| i.get()).sum();
    let mean = f64::from(u32::try_from(sum).unwrap()) / 2_000.0;
    assert!((mean - 10_000.5).abs() < 520.0, "{mean}");
}

#[test]
fn the_subsample_is_drawn_with_the_seed_the_manifest_records() {
    let points = uniform(5_000, 300.0, 2);
    let a = stars(&points, &rules(1_000, 500, 6, 42));
    let b = stars(&points, &rules(1_000, 500, 6, 42));
    let c = stars(&points, &rules(1_000, 500, 6, 43));
    assert_eq!(a.manifest.sampling.seed, 42);
    assert_eq!(c.manifest.sampling.seed, 43);
    let json: serde_json::Value = serde_json::from_slice(&a.json).unwrap();
    assert_eq!(json["sampling"]["seed"], 42);
    assert_eq!(
        a.nodes[0].ids, b.nodes[0].ids,
        "the same seed, the same draw"
    );
    assert_ne!(a.nodes[0].ids, c.nodes[0].ids, "another seed, another draw");
    let all = |d: &BuiltDomain| -> BTreeSet<ObjectId> {
        d.nodes.iter().flat_map(|n| n.ids.clone()).collect()
    };
    assert_eq!(all(&a), all(&c), "never other points");
}
