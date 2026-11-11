//! The Galaxy's tracers in a release: each is an object of the kind its source names, tiled in
//! the Galaxy tracers domain once placed, and a Cepheid or RR Lyrae star joins the Gaia DR3 star
//! its source names (normalize-records; build-octree-tiles; define-a-source).

#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod release_fixture;
mod simulated;

use release_fixture::{build_next, cross, fixture_with, loaded, terms};
use simulated::{bare, designation, entry, matching_rules};
use univ_ingest::matching::match_entries;
use univ_model::object::ObjectKind;

/// The recorded tracer sources and the kind of object each names.
const TRACERS: [(&str, &str); 5] = [
    ("hunt-reffert-2023", "open_cluster"),
    ("baumgardt-vasiliev-2021", "globular_cluster"),
    ("skowron-2025-cepheids", "classical_cepheid"),
    ("li-2023-rr-lyrae", "rr_lyrae_star"),
    ("reid-2019-masers", "star_forming_region"),
];

#[test]
fn each_tracer_is_an_object_of_its_kind_in_the_galaxy_tracers_domain_once_placed() {
    let fixture = fixture_with(&TRACERS.map(|(id, _)| id));
    let crossed = cross(&fixture);
    let release = build_next(&fixture);
    let db = loaded(&release.dir, &release.manifest);
    let terms = terms(&db);
    let term = |id: Option<i64>| id.map(|id| terms[&id].clone());
    for (source, kind) in TRACERS {
        let mut placed = 0;
        for o in crossed
            .objects
            .iter()
            .filter(|o| o.object.sources().any(|s| s.as_str() == source))
        {
            let id = i64::try_from(o.id.get()).unwrap();
            let (k, domain): (i64, Option<i64>) = db
                .query_row(
                    "SELECT kind, scale_domain FROM objects WHERE release = 1 AND id = ?1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            assert_eq!(term(Some(k)).as_deref(), Some(kind), "{source} {id}");
            match (&o.position, term(domain)) {
                (Some(_), Some(d)) => {
                    assert_eq!(d, "galaxy_tracers", "{source} {id}");
                    placed += 1;
                }
                (None, None) => {}
                (p, d) => panic!("{source} {id}: placed {} in {d:?}", p.is_some()),
            }
        }
        // The masers place only where the parallax rule lets them: 2 of the 5 recorded are
        // within its 10%; every cluster and pulsating star has its published distance.
        let expected = if source == "reid-2019-masers" { 2 } else { 5 };
        assert_eq!(placed, expected, "{source}");
    }
}

#[test]
fn a_cepheid_and_an_rr_lyrae_star_join_the_gaia_star_their_source_names() {
    // SIMULATED positions; CG Cas's Gaia DR3 source_id as Skowron et al. 2025 give it, and an
    // RR Lyrae star's as Li et al. 2023 give it, each also a Gaia DR3 star below G = 14.
    let (cg_cas, rr) = ("429385923752386944", "507222753405440");
    let entries = vec![
        entry("gaia-dr3-bright", cg_cas, 0.2468, 60.959),
        bare("skowron-2025-cepheids", cg_cas),
        entry("gaia-dr3-bright", rr, 44.6653, 1.7894),
        entry("li-2023-rr-lyrae", rr, 44.6653, 1.7894),
    ];
    let rules = matching_rules().with_object_kinds(&common::read_sources());
    let m = match_entries(&rules, entries);
    assert_eq!(m.objects.len(), 2);
    for (tracer, id, kind) in [
        (
            "skowron-2025-cepheids",
            cg_cas,
            ObjectKind::ClassicalCepheid,
        ),
        ("li-2023-rr-lyrae", rr, ObjectKind::RrLyraeStar),
    ] {
        let at = m.position_of(&designation(tracer, id)).unwrap();
        assert_eq!(
            Some(at),
            m.position_of(&designation("gaia-dr3-bright", id)),
            "{tracer}"
        );
        // Matching makes the joined object the kind its tracer source names.
        assert_eq!(m.objects[at].kind, kind, "{tracer}");
    }
    assert_eq!(m.report.conflicting_kinds, 0);
}

#[test]
fn a_star_two_tracer_sources_name_differently_is_counted_as_a_conflict() {
    // SIMULATED: one Gaia DR3 source_id listed as a Cepheid by Skowron et al. and as an RR
    // Lyrae star by Li et al.: one object, of its first tracer entry's kind, counted.
    let id = "429385923752386944";
    let entries = vec![
        entry("gaia-dr3-bright", id, 0.2468, 60.959),
        bare("skowron-2025-cepheids", id),
        bare("li-2023-rr-lyrae", id),
    ];
    let rules = matching_rules().with_object_kinds(&common::read_sources());
    let m = match_entries(&rules, entries);
    assert_eq!(m.objects.len(), 1);
    assert!(m.objects[0].kind.is_tracer());
    assert_eq!(m.report.conflicting_kinds, 1);
}

#[test]
fn a_globular_cluster_joins_the_openngc_object_of_the_same_cluster_by_position() {
    // Positions as recorded on 2026-11-10: 47 Tuc in Baumgardt's table, NGC_104 at (6.023792,
    // -72.081306), and OpenNGC's NGC 104 3 arcsec away (SIMULATED offset of that size).
    let cluster = entry("baumgardt-vasiliev-2021", "NGC_104", 6.023_792, -72.081_306);
    let named = entry("openngc", "NGC 104", 6.023_792, -72.081_306 + 3.0 / 3600.0);
    let rules = matching_rules().with_object_kinds(&common::read_sources());
    let m = match_entries(&rules, vec![cluster, named]);
    assert_eq!(m.objects.len(), 1, "one cluster");
    assert_eq!(m.objects[0].kind, ObjectKind::GlobularCluster);
}
