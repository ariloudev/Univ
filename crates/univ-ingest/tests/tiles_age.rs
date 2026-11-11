//! The age a Galaxy tracers point carries (build-octree-tiles): an open cluster's log age in
//! years, in the place a star's colour index takes, within 0.01 of the value it was built from;
//! a tracer without an age carries none, and no stars point carries one. The clusters and
//! Cepheids are the recorded answers of Hunt & Reffert 2023 and Skowron et al. 2025.

#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod release_fixture;

use std::collections::BTreeMap;

use release_fixture::{cross, fixture_with, tiles};
use univ_ingest::source::ScaleDomain;
use univ_ingest::tiles::{assign, log_age_of};
use univ_model::tile::{PointValue, decode};

#[test]
fn an_open_cluster_point_carries_its_log_age_and_an_ageless_tracer_none() {
    let fixture = fixture_with(&["hunt-reffert-2023", "skowron-2025-cepheids"]);
    let crossed = cross(&fixture);
    let built = tiles(&fixture, &crossed);
    let assigned = assign(&crossed.objects, &fixture.definitions, &crossed.cosmology).unwrap();
    let ages: BTreeMap<u64, Option<f64>> = crossed
        .objects
        .iter()
        .map(|o| (o.id.get(), log_age_of(&o.object)))
        .collect();

    let tracers = &assigned.domains[&ScaleDomain::GalaxyTracers].points;
    let (mut with_age, mut without_age) = (0, 0);
    for p in tracers {
        if let Some(log_age) = ages[&p.id.get()] {
            // An age in the place of its colour.
            assert_eq!(p.value, Some(PointValue::LogAge(log_age)));
            with_age += 1;
        } else {
            assert_eq!(p.value.and_then(PointValue::log_age), None);
            without_age += 1;
        }
    }
    assert!(
        with_age >= 5,
        "the recorded open clusters are placed: {with_age}"
    );
    assert!(without_age > 0, "the recorded Cepheids carry no age");
    for (domain, points) in &assigned.domains {
        if *domain != ScaleDomain::GalaxyTracers {
            assert!(
                points
                    .points
                    .iter()
                    .all(|p| p.value.and_then(PointValue::log_age).is_none()),
                "{domain:?}"
            );
        }
    }

    // Each tracer tile decodes each cluster's log age within 0.01 of the published one.
    let domain = built
        .domains
        .iter()
        .find(|d| d.domain == ScaleDomain::GalaxyTracers)
        .unwrap();
    let mut decoded = 0;
    for node in &domain.nodes {
        for p in decode(&node.tile).unwrap().points {
            let built = ages[&p.id.get()];
            match (built, p.value.and_then(PointValue::log_age)) {
                (Some(b), Some(d)) => {
                    assert!((b - d).abs() < 0.01, "{b} decoded as {d}");
                    decoded += 1;
                }
                (None, None) => {}
                other => panic!("object {}: {other:?}", p.id.get()),
            }
        }
    }
    assert_eq!(decoded, with_age);
}
