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
use univ_ingest::tiles::{BuiltDomain, NEARBY_GALAXIES_DEPTH_MPC, build};
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
        assert_eq!(decoded_ids(d)["r"], [ObjectId::new(id).unwrap()]);
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
fn an_object_s_scale_domain_follows_its_placed_distance_and_a_star_stays_a_star() {
    // SIMULATED, AC-92 as the owner decided on 2026-11-03: M81 at 3.63 Mpc is in nearby
    // galaxies whether its distance is UNGC's (a nearby-galaxies source) or Cosmicflows-4's (a
    // cosmic-web source); a Cosmicflows-4 galaxy at 40 Mpc is in cosmic web; a Gaia star stays
    // in stars. The boundary is the 11 Mpc depth of the Updated Nearby Galaxy Catalog.
    let (ra, dec) = (148.888, 69.065);
    let ungc = with_distance(
        entry("updated-nearby-galaxies", "MESSIER081", ra, dec),
        3.63,
        None,
        CEPHEIDS,
    );
    let cf4 = with_distance(
        entry("cosmicflows-4-galaxies", "28630", ra, dec),
        3.63,
        Some(0.1),
        CEPHEIDS,
    );
    let far = with_distance(
        entry("cosmicflows-4-galaxies", "g", 150.0, 5.0),
        40.0,
        Some(4.0),
        CEPHEIDS,
    );
    let star = with_parallax(entry("gaia-dr3-bright", "s", 10.0, 10.0), 10.0, 0.01);
    let by = |e: &univ_model::entry::CatalogueEntry| {
        Some(MeasurementRef::Distance {
            entry: e.designation.clone(),
            index: 0,
        })
    };
    let parallax = Some(MeasurementRef::Parallax {
        entry: star.designation.clone(),
    });
    let objects = vec![
        object(1, vec![ungc.clone(), cf4.clone()], by(&ungc)),
        object(2, vec![ungc.clone(), cf4.clone()], by(&cf4)),
        object(3, vec![far.clone()], by(&far)),
        object(4, vec![star], parallax),
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
    assert_eq!(ids(0), [4], "stars");
    assert_eq!(ids(1), [1, 2], "nearby galaxies");
    assert_eq!(ids(2), [3], "cosmic web");
    assert!((NEARBY_GALAXIES_DEPTH_MPC - 11.0).abs() < 1e-12);
    // The point keeps the source of its distance, credited in its domain's manifest.
    let nearby = &built.domains[1].manifest;
    let credited: Vec<_> = nearby.sources.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        credited,
        ["updated-nearby-galaxies", "cosmicflows-4-galaxies"],
        "{credited:?}"
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
            // Its level's fraction of the points reaching it, rounded down: at most `kept`.
            let fraction = built.manifest.levels[usize::from(n.level)].fraction;
            let share = fraction * real(n.sampled_from);
            assert!(n.points <= kept, "{}", n.address);
            assert!(real(n.points) <= share + 1e-6 && real(n.points) > share - 1.0);
        } else {
            assert!(!has_children(&n.address), "{} is a leaf", n.address);
            assert_eq!(n.points, n.sampled_from, "a leaf keeps every point");
            if n.sampled_from > threshold {
                deep_and_full += 1;
            }
        }
    }
    assert!(
        deep_and_full > 0,
        "a cell at the maximum depth over the threshold"
    );
    let total: usize = built.nodes.iter().map(|n| n.points).sum();
    assert_eq!(total, 1_600);
}

#[test]
fn each_level_is_an_even_random_subsample_and_a_finer_level_repeats_no_point() {
    let points = uniform(20_000, 1_000.0, 5);
    let built = stars(&points, &rules(4_000, 2_000, 6, 11));
    let decoded = decoded_ids(&built);
    let mut seen = BTreeSet::new();
    for (address, ids) in &decoded {
        for id in ids {
            assert!(seen.insert(*id), "{id} repeated in {address}");
        }
    }
    assert_eq!(seen.len(), 20_000, "every point in exactly one node");
    for (address, ids) in &decoded {
        if let Some(parent) = decoded.get(&address[..address.len() - 1]) {
            assert!(parent.iter().all(|id| !ids.contains(id)));
        }
    }
    // The root keeps 2,000 of 20,000 uniform points, each equally likely: about 250 in each
    // octant and a mean id near the middle, within four standard deviations.
    let (root, root_ids) = (&built.nodes[0], &decoded["r"]);
    assert_eq!((root_ids.len(), root.sampled_from), (2_000, 20_000));
    let mut octants = [0_u32; 8];
    for id in root_ids {
        let p = points[usize::try_from(id.get()).unwrap() - 1].position;
        octants[usize::from(root.bounds.octant_of(p))] += 1;
    }
    for n in octants {
        assert!((190..=310).contains(&n), "{octants:?}");
    }
    let sum: u64 = root_ids.iter().map(|i| i.get()).sum();
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
    let root = |d: &BuiltDomain| decoded_ids(d).remove("r").unwrap();
    assert_eq!(root(&a), root(&b), "the same seed, the same draw");
    assert_ne!(root(&a), root(&c), "another seed, another draw");
    let all = |d: &BuiltDomain| -> BTreeSet<ObjectId> {
        decoded_ids(d).into_values().flatten().collect()
    };
    assert_eq!(all(&a), all(&c), "never other points");
}

/// `n` as an `f64`, for the ratios below.
fn real(n: usize) -> f64 {
    f64::from(u32::try_from(n).unwrap())
}

#[test]
fn every_inner_node_of_a_level_keeps_one_fraction_so_shown_densities_keep_their_ratio() {
    // SIMULATED: 20,000 stars spread through the root's lowest octant and 2,000, ten times
    // fewer, through its highest, each well over the split threshold.
    let mut lcg = Lcg(21);
    let points: Vec<_> = (1..=22_000)
        .map(|id| {
            let centre = if id <= 20_000 { -50.0 } else { 50.0 };
            point(
                id,
                [0, 1, 2].map(|_| centre + 40.0 * (2.0 * lcg.next() - 1.0)),
            )
        })
        .collect();
    let built = stars(&points, &rules(1_000, 500, 6, 3));
    let json: serde_json::Value = serde_json::from_slice(&built.json).unwrap();
    let node = |address: &str| {
        built
            .manifest
            .nodes
            .iter()
            .find(|n| n.address == address)
            .unwrap()
    };
    let (dense, sparse) = (node("r0"), node("r7"));
    assert!(
        dense.sampled_from > 1_000 && sparse.sampled_from > 1_000,
        "both inner"
    );
    let kept = |n: &univ_ingest::tiles::NodeEntry| real(n.points) / real(n.sampled_from);
    // One fraction for the level, which the manifest records: each node keeps it of the points
    // reaching it, within the rounding of one point.
    let fraction = json["levels"][1]["fraction"].as_f64().unwrap();
    for n in [dense, sparse] {
        assert!(
            (kept(n) - fraction).abs() <= 1.0 / real(n.sampled_from),
            "{} keeps {} of {}, not {fraction}",
            n.address,
            n.points,
            n.sampled_from
        );
        assert!(n.points <= 500);
    }
    assert_eq!(dense.points, 500, "the fullest node keeps the level points");
    // The points shown at level 1 are ten times as dense in the lower octant as in the upper,
    // as the stars are, within the rounding of one point in 50.
    let level_one = real(dense.points) / real(sparse.points);
    assert!(
        (level_one / 10.0 - 1.0).abs() < 0.05,
        "level 1: {level_one}"
    );
    // So are those of levels 0 and 1 together, within the noise of the root's own draw: about
    // 45 of its 500 points fall in the upper octant, give or take 7, and a quarter is 4 sigma.
    let root = decode(&built.nodes[0].tile).unwrap();
    let in_root = |octant: u8| {
        root.points
            .iter()
            .filter(|p| root.node.octant_of(p.position) == octant)
            .count()
    };
    let together = real(in_root(0) + dense.points) / real(in_root(7) + sparse.points);
    assert!(
        (together / 10.0 - 1.0).abs() < 0.25,
        "levels 0 and 1: {together}"
    );
}

#[test]
fn the_source_table_says_whether_each_source_gave_points_their_distance_or_position() {
    // SIMULATED: a galaxy whose sky position comes from 2MRS and its distance from
    // Cosmicflows-4, then another wholly from Cosmicflows-4.
    let twomrs = entry("twomrs", "g", 150.0, 5.0);
    let cf4 = with_distance(
        entry("cosmicflows-4", "g", 150.0, 5.0),
        80.0,
        Some(4.0),
        CEPHEIDS,
    );
    let alone = with_distance(
        entry("cosmicflows-4", "h", 20.0, -5.0),
        60.0,
        Some(3.0),
        CEPHEIDS,
    );
    let by = |e: &univ_model::entry::CatalogueEntry| {
        Some(MeasurementRef::Distance {
            entry: e.designation.clone(),
            index: 0,
        })
    };
    let rules = rules(100, 50, 8, 7);
    let (cosmology, definitions) = (Cosmology::planck_2018(), common::read_sources());
    let source_roles = |objects: &[univ_ingest::crossmatch::ReleaseObject]| {
        let built = build(objects, &context(&rules, &cosmology, &definitions)).unwrap();
        let cosmic = &built.domains[2];
        let json: serde_json::Value = serde_json::from_slice(&cosmic.json).unwrap();
        let sources = json["sources"].as_array().unwrap().clone();
        // Each point's source is the source of its preferred distance.
        for p in decode(&cosmic.nodes[0].tile).unwrap().points {
            assert_eq!(sources[usize::from(p.source)]["id"], "cosmicflows-4");
        }
        sources
            .iter()
            .map(|s| {
                (
                    s["id"].as_str().unwrap().to_owned(),
                    s["role"].as_str().unwrap_or("none").to_owned(),
                )
            })
            .collect::<BTreeMap<_, _>>()
    };
    let galaxy = object(1, vec![twomrs, cf4.clone()], by(&cf4));
    let expected = |cf4: &str| {
        BTreeMap::from([
            ("cosmicflows-4".to_owned(), cf4.to_owned()),
            ("twomrs".to_owned(), "position".to_owned()),
        ])
    };
    assert_eq!(
        source_roles(std::slice::from_ref(&galaxy)),
        expected("distance")
    );
    let other = object(2, vec![alone.clone()], by(&alone));
    assert_eq!(
        source_roles(&[galaxy, other]),
        expected("distance_and_position")
    );
}
