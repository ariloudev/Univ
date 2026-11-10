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

use release_fixture::{build_next, cross, fixture_with, loaded};
use simulated::{bare, designation, entry, matching_rules};
use univ_ingest::matching::match_entries;

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
    let term = |id: Option<i64>| -> Option<String> {
        id.map(|id| {
            db.query_row(
                "SELECT term FROM terms WHERE release = 1 AND id = ?1",
                [id],
                |r| r.get(0),
            )
            .unwrap()
        })
    };
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
    let m = match_entries(&matching_rules(), entries);
    assert_eq!(m.objects.len(), 2);
    for (tracer, id) in [("skowron-2025-cepheids", cg_cas), ("li-2023-rr-lyrae", rr)] {
        assert_eq!(
            m.position_of(&designation(tracer, id)),
            m.position_of(&designation("gaia-dr3-bright", id)),
            "{tracer}"
        );
    }
}
