//! `McConnachie` 2012 and UNGC entries of one Local Group galaxy are one object: joined by the
//! common name both publish, or by position and distance within the pair's tolerance, on real
//! recorded rows of Fornax, M32, NGC 55 and the Sagittarius dwarf spheroidal from each source.
#![expect(clippy::unwrap_used, reason = "a failed unwrap is the test failing")]

mod common;
mod simulated;

use simulated::{designation, matching_rules, ranking_rules};
use univ_ingest::matching::{Matched, match_entries};
use univ_ingest::ranking::rank_all;
use univ_model::derive::Release;
use univ_model::object::{MatchMethod, Object, Resolution};

fn recorded(id: &str, file: &str) -> Vec<univ_model::entry::CatalogueEntry> {
    common::normalized_file(id, file).entries
}

fn objects() -> Matched {
    let mut entries = recorded("mcconnachie-2012", "mcconnachie-2012-local-group.tsv");
    entries.extend(recorded(
        "updated-nearby-galaxies",
        "updated-nearby-galaxies-local-group.tsv",
    ));
    assert_eq!(entries.len(), 8);
    let mut m = match_entries(&matching_rules(), entries);
    rank_all(&mut m.objects, &ranking_rules(), &Release::planck_2018());
    m
}

fn of<'a>(objects: &'a [Box<Object>], mcconnachie: &str) -> &'a Object {
    objects
        .iter()
        .find(|o| {
            o.entry(&designation("mcconnachie-2012", mcconnachie))
                .is_some()
        })
        .unwrap()
}

fn ungc_resolution<'a>(o: &'a Object, ungc: &str) -> &'a Resolution {
    &o.entry(&designation("updated-nearby-galaxies", ungc))
        .unwrap()
        .resolved_by
}

#[test]
fn each_galaxy_both_sources_list_is_one_object() {
    let objects = objects().objects;
    assert_eq!(objects.len(), 4, "{objects:#?}");
    // Fornax, 4.7 arcmin apart, and M32, whose distances differ by 49% (0.805 against 0.49
    // Mpc), are joined by the name both publish; NGC 55 by NGC 55 and NGC0055.
    for (mcconnachie, ungc, key) in [
        ("Fornax", "Fornax", "fornax"),
        ("M32", "MESSIER032", "m32"),
        ("NGC 55", "NGC0055", "ngc55"),
    ] {
        let o = of(&objects, mcconnachie);
        let Resolution::CrossIdentification {
            scheme, identifier, ..
        } = ungc_resolution(o, ungc)
        else {
            panic!("{ungc}: {:?}", o.entries)
        };
        assert_eq!((scheme.as_str(), identifier.as_str()), ("common-name", key));
    }
    // The Sagittarius dwarf, Sag dSph in UNGC, 5.3 arcmin apart and 26 against 20 kpc: by
    // position and distance under the 6 arcmin tolerance.
    let sgr = of(&objects, "Sagittarius dSph");
    let Resolution::Match(m) = ungc_resolution(sgr, "Sag dSph") else {
        panic!()
    };
    assert_eq!(m.method, MatchMethod::PositionAndDistance);
    assert!(
        m.separation_arcsec > 300.0 && m.separation_arcsec < 360.0,
        "{m:?}"
    );
}

#[test]
fn a_merge_leaves_the_choice_of_distance_to_the_ranking() {
    // M32: McConnachie's 0.805 Mpc modulus distance with its error beats UNGC's 0.49 Mpc TRGB
    // distance without one, whose method rank is no better: the ranking decides, not the merge.
    let objects = objects().objects;
    let p = of(&objects, "M32").preferred_distance.as_ref().unwrap();
    assert_eq!(p.measurement.entry().source.as_str(), "mcconnachie-2012");
    assert!((p.distance_mpc - 0.805).abs() < 0.01, "{}", p.distance_mpc);
    let p = of(&objects, "Fornax").preferred_distance.as_ref().unwrap();
    assert_eq!(p.measurement.entry().source.as_str(), "mcconnachie-2012");
}

#[test]
fn a_shared_name_beyond_the_angular_tolerance_is_left_apart_as_a_candidate_link() {
    // B6: McConnachie's Fornax, recorded, against a SIMULATED UNGC entry named Fornax 1 degree
    // away, beyond the scheme's 30 arcmin: two objects and a candidate link, not a merge.
    let rules = matching_rules();
    let scheme = rules.scheme("common-name").unwrap();
    assert_eq!(scheme.angular_tolerance_arcmin, Some(30.0));
    assert!(scheme.tolerance_basis.as_deref().unwrap().contains("R2"));
    let fornax = recorded("mcconnachie-2012", "mcconnachie-2012-local-group.tsv")
        .into_iter()
        .find(|e| &*e.designation.identifier == "Fornax")
        .unwrap();
    let p = fornax.position.as_ref().unwrap().icrs;
    let far = simulated::entry("updated-nearby-galaxies", "Fornax", p.ra, p.dec + 1.0);
    let m = match_entries(&rules, vec![fornax.clone(), far]);
    assert_eq!(m.objects.len(), 2, "{:#?}", m.objects);
    let links: Vec<_> = m.objects.iter().flat_map(|o| &o.links).collect();
    assert_ne!(links.len(), 0);
    assert!(links.iter().all(|l| matches!(
        &l.reason,
        Resolution::CrossIdentification { scheme, .. } if scheme == "common-name"
    )));
    // Within the tolerance, 20 arcmin away, the name joins them.
    let near = simulated::entry("updated-nearby-galaxies", "Fornax", p.ra, p.dec + 1.0 / 3.0);
    assert_eq!(match_entries(&rules, vec![fornax, near]).objects.len(), 1);
}

#[test]
fn messier_compares_as_m_by_the_rules_alias_which_names_its_basis() {
    // C10: the alias is the rules' own, not the code's: without it MESSIER032 and M32 are two
    // names, and an alias without its basis is refused.
    let rules = matching_rules();
    let scheme = rules.scheme("common-name").unwrap();
    assert_eq!(scheme.aliases.get("messier").map(String::as_str), Some("m"));
    let mut bare = rules.clone();
    let at = bare
        .cross_identifications
        .iter()
        .position(|s| s.scheme == "common-name")
        .unwrap();
    bare.cross_identifications[at].aliases.clear();
    let mut entries = recorded("mcconnachie-2012", "mcconnachie-2012-local-group.tsv");
    entries.extend(recorded(
        "updated-nearby-galaxies",
        "updated-nearby-galaxies-local-group.tsv",
    ));
    let m = match_entries(&bare, entries);
    let m32 = m
        .objects
        .iter()
        .find(|o| o.entry(&designation("mcconnachie-2012", "M32")).is_some())
        .unwrap();
    assert!(!matches!(
        m32.entry(&designation("updated-nearby-galaxies", "MESSIER032"))
            .map(|e| &*e.resolved_by),
        Some(Resolution::CrossIdentification { .. })
    ));
    let mut unexplained = rules;
    unexplained.cross_identifications[at].aliases_basis = None;
    assert!(unexplained.check().is_err());
}
