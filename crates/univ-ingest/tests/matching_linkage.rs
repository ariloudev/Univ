//! Cross-matching where a chain of close pairs, a contested cross-identification or a galaxy
//! group could join what is not one body, on SIMULATED entries under the repository's
//! `rules/matching.yaml`.

mod common;
mod simulated;

use simulated::{
    bare, designation, entry, matching_rules, named, offset, ranking_rules, source, with_cz,
    with_modulus,
};
use univ_ingest::matching::match_entries;
use univ_ingest::matching::rules::{EntryField, SchemeMember};
use univ_ingest::ranking::rank_all;
use univ_model::derive::Release;
use univ_model::distance::{DistanceKind, Method};
use univ_model::entry::CatalogueEntry;
use univ_model::object::{MeasurementRef, ObjectKind, RelationKind};

#[test]
fn a_chain_of_close_pairs_does_not_join_entries_too_far_apart() {
    // 2MRS A; 6dFGS B 4.5" from A at +150 km/s (within the 5" and 200 km/s of 2MRS and 6dFGS);
    // SDSS C 9" from A at +300 km/s, which is 4.5" and 150 km/s from B but beyond the 5" of
    // 2MRS and SDSS from A. B may join A, but C may not then join them through B.
    let (ra, dec) = (200.0, -10.0);
    let b = offset(ra, dec, 4.5, 0.0);
    let c = offset(ra, dec, 9.0, 0.0);
    let m = match_entries(
        &matching_rules(),
        vec![
            with_cz(entry("twomrs", "A", ra, dec), 8_000.0, 20.0),
            with_cz(entry("sixdfgs-dr3", "B", b.0, b.1), 8_150.0, 20.0),
            with_cz(entry("sdss-dr17-galaxies", "C", c.0, c.1), 8_300.0, 20.0),
        ],
    );
    let at = |s: &str, id: &str| m.position_of(&designation(s, id)).unwrap();
    assert_ne!(at("twomrs", "A"), at("sdss-dr17-galaxies", "C"));
    assert_eq!(m.objects.len(), 2);
    // C stays apart, linked to the entry it was a candidate of, with the rule's confidence.
    let apart = &m.objects[at("sdss-dr17-galaxies", "C")];
    assert_eq!(apart.links.len(), 1);
    let link = &apart.links[0];
    assert_eq!(link.candidate, designation("sixdfgs-dr3", "B"));
    let expected = (1.0 - 0.9_f64.powi(2)) * (1.0 - 0.75_f64.powi(2));
    assert!((link.confidence.get() - expected).abs() < 1e-3, "{link:?}");
}

/// HIP 10768 and two Gaia sources 0.8" either side, each named by a row of the Gaia DR3 to
/// Hipparcos-2 table as HIP 10768 (all simulated).
fn contested() -> Vec<CatalogueEntry> {
    let (ra, dec) = (45.0, 30.0);
    let g1 = offset(ra, dec, 0.8, 0.0);
    let g2 = offset(ra, dec, -0.8, 0.0);
    vec![
        entry("gaia-dr3-bright", "111", g1.0, g1.1),
        entry("gaia-dr3-bright", "222", g2.0, g2.1),
        named(bare("gaia-dr3-hipparcos2", "111"), "10768"),
        named(bare("gaia-dr3-hipparcos2", "222"), "10768"),
        entry("hipparcos-2007", "10768", ra, dec),
    ]
}

#[test]
fn a_cross_identification_two_entries_of_one_source_claim_joins_neither() {
    for entries in [contested(), contested().into_iter().rev().collect()] {
        let m = match_entries(&matching_rules(), entries);
        let at = |s: &str, id: &str| m.position_of(&designation(s, id)).unwrap();
        let hip = at("hipparcos-2007", "10768");
        let (g1, g2) = (at("gaia-dr3-bright", "111"), at("gaia-dr3-bright", "222"));
        assert!(hip != g1 && hip != g2 && g1 != g2);
        // Each Gaia source keeps its own table row, by the Gaia source id.
        assert_eq!(g1, at("gaia-dr3-hipparcos2", "111"));
        assert_eq!(g2, at("gaia-dr3-hipparcos2", "222"));
        // Both claimants are linked to the HIP star at the conflict confidence.
        let conflict = matching_rules().confidence.cross_identification_conflict;
        for row in ["111", "222"] {
            let claimant = designation("gaia-dr3-hipparcos2", row);
            let link = m.objects[hip]
                .links
                .iter()
                .find(|l| l.candidate == claimant)
                .unwrap();
            assert_eq!(link.confidence.get(), conflict);
        }
    }
}

const TF: (DistanceKind, Method) = (DistanceKind::SecondaryIndicator, Method::TullyFisher);

#[test]
fn a_galaxy_group_stays_apart_from_its_dominant_galaxy_and_lends_it_nothing() {
    // A Cosmicflows-4 group row 2" from a 2MRS galaxy at the group's velocity, its 1PGC naming
    // that galaxy's PGC number. No source defined so far gives a galaxy's PGC number, so here
    // 2MRS is supposed, for the test, to give it as the entry's name.
    let mut rules = matching_rules();
    let pgc = rules
        .cross_identifications
        .iter_mut()
        .find(|s| s.scheme == "pgc")
        .unwrap();
    pgc.members.push(SchemeMember {
        source: source("twomrs"),
        field: EntryField::Name,
    });
    let (ra, dec) = (150.0, 2.0);
    let near = offset(ra, dec, 2.0, 0.0);
    let group = with_modulus(entry("cosmicflows-4", "42", ra, dec), 34.0, 0.05, TF);
    let galaxy = named(
        with_cz(entry("twomrs", "g", near.0, near.1), 5_000.0, 20.0),
        "42",
    );
    let m = match_entries(&rules, vec![with_cz(group, 5_000.0, 10.0), galaxy]);
    assert_eq!(m.objects.len(), 2);
    let at = |s: &str, id: &str| m.position_of(&designation(s, id)).unwrap();
    let (g, x) = (at("cosmicflows-4", "42"), at("twomrs", "g"));
    assert_ne!(g, x);
    assert_eq!(m.objects[g].kind, ObjectKind::GalaxyGroup);
    assert_eq!(m.objects[x].kind, ObjectKind::Body);
    let to = &m.objects[g].relations;
    let from = &m.objects[x].relations;
    assert_eq!(to.len(), 1);
    assert_eq!(from.len(), 1);
    assert_eq!(
        (to[0].kind, &to[0].other),
        (RelationKind::DominantMember, &designation("twomrs", "g"))
    );
    assert_eq!(
        (from[0].kind, &from[0].other),
        (
            RelationKind::DominantMemberOf,
            &designation("cosmicflows-4", "42")
        )
    );
    assert!(m.objects.iter().all(|o| o.links.is_empty()));
    assert_eq!(m.report.group_relations, 1);
    // The galaxy is placed by its own redshift, never by the group's distance.
    let mut objects = m.objects;
    rank_all(&mut objects, &ranking_rules(), &Release::planck_2018());
    let preferred = objects[x].preferred_distance.as_ref().unwrap();
    assert!(matches!(
        &preferred.measurement,
        MeasurementRef::Redshift { entry, .. } if entry == &designation("twomrs", "g")
    ));
    // The repository's rules match no group by position.
    let rules = matching_rules();
    assert!(
        rules
            .pairs
            .iter()
            .all(|p| !p.sources.contains(&source("cosmicflows-4")))
    );
}
