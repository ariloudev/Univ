//! The encoding of each tile and its manifest (build-octree-tiles, RQ-37 and RQ-38), and the
//! 1 MiB bound on every tile (QA-1). Every point here is SIMULATED.
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod simulated;

use std::collections::BTreeMap;

use simulated::tiles::{Lcg, context, gaia, point, repository_rules, rules, stars, uniform};
use univ_ingest::source::ScaleDomain;
use univ_ingest::tiles::{
    Built, MANIFEST_FILE, TilesError, build_domain, sha256, tile_file, write_all,
};
use univ_model::cosmology::Cosmology;
use univ_model::position::{Frame, from_icrs};
use univ_model::tile::{MAX_POINTS, MAX_TILE_BYTES, STEPS, decode, tile_bytes};

#[test]
fn positions_are_quantized_within_a_step_of_one_65536th_of_their_node() {
    let points = uniform(3_000, 700.0, 4);
    let built = stars(&points, &rules(500, 250, 5, 1));
    let steps: BTreeMap<u8, f64> = built
        .manifest
        .levels
        .iter()
        .map(|l| (l.level, l.step))
        .collect();
    let mut checked = 0;
    for n in &built.nodes {
        let step = steps[&n.level];
        assert!(
            step <= n.bounds.edge() / f64::from(STEPS),
            "no coarser than 1/65,536"
        );
        for d in decode(&n.tile).unwrap().points {
            let p = points[usize::try_from(d.id.get()).unwrap() - 1].position;
            for (decoded, built) in d.position.iter().zip(p) {
                assert!((decoded - built).abs() <= step / 2.0 * (1.0 + 1e-9));
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 3_000);
}

#[test]
fn the_manifest_states_the_encoding_version_and_each_levels_step() {
    let built = stars(&uniform(3_000, 700.0, 4), &rules(500, 250, 5, 1));
    let json: serde_json::Value = serde_json::from_slice(&built.json).unwrap();
    assert_eq!(json["format"], "univ-tile-manifest");
    assert_eq!(json["encoding"]["version"], 1);
    assert_eq!(json["encoding"]["point_bytes"], 17);
    assert_eq!(json["encoding"]["steps_per_edge"], 65_536);
    let root_edge = built.nodes[0].bounds.edge();
    let levels = json["levels"].as_array().unwrap();
    let present: std::collections::BTreeSet<u8> = built.nodes.iter().map(|n| n.level).collect();
    assert_eq!(levels.len(), present.len());
    for l in levels {
        let level = l["level"].as_u64().unwrap();
        let edge = root_edge / f64::from(1u32 << level);
        assert_eq!(l["edge"].as_f64().unwrap(), edge);
        assert_eq!(l["step"].as_f64().unwrap(), edge / 65_536.0);
    }
    // What a client needs besides: frame, origin, unit, cosmology, rotations and each node.
    assert_eq!(json["frame"]["name"], "ICRS equatorial Cartesian");
    assert_eq!(json["frame"]["origin"], "Solar System barycentre");
    assert_eq!(json["unit"], "pc");
    assert_eq!(json["cosmology"]["hubble_constant"], 67.66);
    let galactic = from_icrs(Frame::Galactic).rotation().unwrap().0;
    assert_eq!(json["rotations"]["to_galactic"][2][1], galactic[2][1]);
    assert!(json["rotations"]["to_supergalactic"].is_array());
    assert_eq!(json["sources"][0]["id"], "gaia-dr3-bright");
    assert!(
        json["sources"][0]["credit"]
            .as_str()
            .unwrap()
            .contains("Gaia")
    );
    let node = &json["nodes"][0];
    for field in [
        "address",
        "level",
        "bounds",
        "points",
        "sampled_from",
        "bytes",
        "sha256",
    ] {
        assert!(!node[field].is_null(), "{field}");
    }
}

/// SIMULATED: a cosmic web domain of points spread over 1 Gpc, in `order`.
fn cosmic_web(order: &[usize]) -> (Vec<u8>, Vec<Vec<u8>>) {
    let mut points = uniform(4_000, 1_000.0, 8);
    for p in &mut points {
        (p.source, p.sky_source) = (0, 1);
    }
    let sources = ["cosmicflows-4", "twomrs"].map(simulated::source);
    let mut shuffled: Vec<_> = order.iter().map(|&i| points[i].clone()).collect();
    shuffled.sort_by_key(|p| p.id);
    let (rules, cosmology, definitions) = (
        rules(700, 300, 7, 99),
        Cosmology::planck_2018(),
        common::read_sources(),
    );
    let built = build_domain(
        ScaleDomain::CosmicWeb,
        &shuffled,
        &sources,
        &context(&rules, &cosmology, &definitions),
    )
    .unwrap();
    let tiles = built.nodes.iter().map(|n| n.tile.clone()).collect();
    (built.json, tiles)
}

#[test]
fn two_builds_from_the_same_inputs_and_seed_give_the_same_bytes() {
    let forward: Vec<usize> = (0..4_000).collect();
    let backward: Vec<usize> = (0..4_000).rev().collect();
    let (json_a, tiles_a) = cosmic_web(&forward);
    let (json_b, tiles_b) = cosmic_web(&backward);
    assert_eq!(json_a, json_b, "the manifests are byte-identical");
    assert_eq!(tiles_a, tiles_b, "the tiles are byte-identical");
    let manifest: serde_json::Value = serde_json::from_slice(&json_a).unwrap();
    for (node, tile) in manifest["nodes"].as_array().unwrap().iter().zip(&tiles_a) {
        assert_eq!(node["sha256"], sha256(tile).as_str());
        assert_eq!(node["bytes"], tile.len());
    }

    // Written twice, the files are the same too.
    let points = uniform(2_000, 50.0, 6);
    let write = |dir: &std::path::Path| {
        let built = Built {
            domains: vec![stars(&points, &rules(300, 100, 6, 5))],
            unplaced: 0,
        };
        write_all(dir, &built).unwrap();
        built
    };
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let built = write(a.path());
    write(b.path());
    for name in built.domains[0]
        .nodes
        .iter()
        .map(|n| tile_file(&n.address))
        .chain([MANIFEST_FILE.to_owned()])
    {
        let read = |d: &std::path::Path| std::fs::read(d.join("stars").join(&name)).unwrap();
        assert_eq!(read(a.path()), read(b.path()), "{name}");
    }
}

#[test]
fn every_tile_stays_within_one_mebibyte_under_the_repository_rules() {
    // SIMULATED stress: 300,000 points, half spread over 10 kpc and half packed into a few
    // star-cluster-sized clumps, under the repository's rules.
    let rules = repository_rules();
    assert!(tile_bytes(rules.split_threshold) <= MAX_TILE_BYTES);
    assert!(tile_bytes(rules.level_points) <= MAX_TILE_BYTES);
    let mut points = uniform(150_000, 10_000.0, 12);
    let mut lcg = Lcg(13);
    for id in 150_001..=300_000 {
        let clump = f64::from(u32::try_from(id % 5).unwrap()) * 1_000.0;
        points.push(point(id, [0, 1, 2].map(|_| clump + 5.0 * lcg.next())));
    }
    let built = stars(&points, &rules);
    let largest = built.largest_tile();
    assert!(largest <= MAX_TILE_BYTES, "{largest}");
    for n in &built.nodes {
        assert!(
            n.tile.len() <= MAX_TILE_BYTES,
            "{} is {} bytes",
            n.address,
            n.tile.len()
        );
        assert_eq!(decode(&n.tile).unwrap().points.len(), n.points);
    }
    assert_eq!(built.points(), 300_000);
}

#[test]
fn a_cell_at_the_maximum_depth_too_full_for_one_tile_refuses_the_build_naming_it() {
    // SIMULATED stress: more points than a tile holds, all within one cell at the maximum
    // depth. The build stops before writing, rather than writing an oversized tile or
    // dropping objects; a cell of exactly the most a tile holds is built, at 1 MiB or less.
    let rules = rules(1_000, 500, 4, 1);
    let (cosmology, definitions) = (Cosmology::planck_2018(), common::read_sources());
    let context = context(&rules, &cosmology, &definitions);
    let crowded = |n: u64| -> Vec<_> {
        let mut lcg = Lcg(17);
        (1..=n)
            .map(|id| point(id, [0, 1, 2].map(|_| 3.0 + 0.001 * lcg.next())))
            .collect()
    };
    // The four nodes above the cell, at depths 0 to 3, keep 500 points each.
    let above = 4 * 500;
    let too_many = u64::try_from(MAX_POINTS + above).unwrap() + 1;
    let Err(refused) = build_domain(ScaleDomain::Stars, &crowded(too_many), &gaia(), &context)
    else {
        panic!("built")
    };
    let TilesError::TileTooLarge {
        domain,
        address,
        points,
        bytes,
    } = &refused
    else {
        panic!("{refused}")
    };
    assert_eq!(domain, "stars");
    assert_eq!(address.len(), 5, "a node at depth 4: {address}");
    assert_eq!(*points, MAX_POINTS + 1, "less what the nodes above it keep");
    assert!(*bytes > MAX_TILE_BYTES);
    assert!(refused.to_string().contains(address.as_str()), "{refused}");

    let full = u64::try_from(MAX_POINTS + above).unwrap();
    let built = build_domain(ScaleDomain::Stars, &crowded(full), &gaia(), &context).unwrap();
    assert_eq!(built.largest_tile(), tile_bytes(MAX_POINTS));
    assert!(built.largest_tile() <= MAX_TILE_BYTES);
}

#[test]
fn rules_that_would_allow_a_tile_beyond_one_mebibyte_are_refused() {
    for (rules, field) in [
        (rules(MAX_POINTS + 1, 10, 8, 1), "split_threshold"),
        (rules(0, 0, 8, 1), "split_threshold"),
        (rules(1_000, 1_001, 8, 1), "level_points"),
        (rules(1_000, 500, 31, 1), "max_depth"),
        (rules(1_000, 500, 8, 1 << 53), "seed"),
    ] {
        assert_eq!(rules.check().unwrap_err().0, field);
    }
    assert!(
        rules(MAX_POINTS, MAX_POINTS, 30, (1 << 53) - 1)
            .check()
            .is_ok()
    );
    assert!(repository_rules().check().is_ok());
}
