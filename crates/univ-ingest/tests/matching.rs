//! Cross-matching (RQ-24..RQ-27, AC-66..AC-74) on SIMULATED catalogues: entries made up under
//! the ids of real sources so that the repository's `rules/matching.yaml` applies, with known
//! pairs, ambiguous triples and far misses. The last tests run the matcher on the real recorded
//! answers in `tests/fixtures` as a smoke test.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a failed unwrap or a panic is the test failing"
)]

mod common;
mod simulated;

use simulated::{
    bare, designation, entry, matching_rules, named, offset, source, with_cz, with_distance,
};
use univ_ingest::matching::{Matched, MatchingRules, match_entries};
use univ_ingest::normalize::reads_rows;
use univ_model::distance::{DistanceKind, Method};
use univ_model::entry::CatalogueEntry;
use univ_model::ids::Designation;
use univ_model::object::{CommonName, LineOfSight, MatchMethod, MatchRecord, Object, Resolution};

/// The place of the object holding `d`.
fn at(m: &Matched, d: &Designation) -> usize {
    m.position_of(d)
        .unwrap_or_else(|| panic!("no object holds {d:?}"))
}

fn record(object: &Object, d: &Designation) -> MatchRecord {
    match &object.entry(d).unwrap().resolved_by {
        Resolution::Match(r) => r.clone(),
        other => panic!("{d:?} resolved by {other:?}"),
    }
}

/// A Hipparcos star, its Gaia counterpart as the published table names it, the table's row,
/// and a second Gaia source closer on the sky than the counterpart (all simulated).
fn gaia_hipparcos() -> Vec<CatalogueEntry> {
    let (ra, dec) = (45.0, 30.0);
    let (g1, g2) = (offset(ra, dec, 0.8, 0.0), offset(ra, dec, 0.1, 0.0));
    vec![
        entry("hipparcos-2007", "10768", ra, dec),
        entry("gaia-dr3-bright", "4612085076726173440", g1.0, g1.1),
        entry("gaia-dr3-bright", "4612085076726173999", g2.0, g2.1),
        named(bare("gaia-dr3-hipparcos2", "4612085076726173440"), "10768"),
        bare("bailer-jones-2021", "4612085076726173440"),
    ]
}

#[test]
fn a_published_cross_identification_is_used_before_a_closer_positional_candidate() {
    // AC-66: the Gaia DR3 to Hipparcos-2 table joins HIP 10768 to the Gaia source 0.8"
    // away, though another Gaia source lies 0.1" away and within the 1.7" tolerance.
    let m = match_entries(&matching_rules(), gaia_hipparcos());
    let hip = designation("hipparcos-2007", "10768");
    let named = designation("gaia-dr3-bright", "4612085076726173440");
    let closer = designation("gaia-dr3-bright", "4612085076726173999");
    let object = &m.objects[at(&m, &hip)];
    assert_eq!(at(&m, &hip), at(&m, &named));
    assert_ne!(at(&m, &hip), at(&m, &closer));
    assert_eq!(
        object.entries.len(),
        4,
        "HIP, Gaia, table row and Bailer-Jones"
    );
    assert_eq!(m.objects[at(&m, &closer)].links.len(), 0);
    assert_eq!(m.report.cross_identified, 3);
    assert_eq!(m.report.merged_by_position, 0);
}

#[test]
fn the_object_records_the_cross_identification_that_resolved_it() {
    // AC-67.
    let m = match_entries(&matching_rules(), gaia_hipparcos());
    let object = &m.objects[at(&m, &designation("hipparcos-2007", "10768"))];
    let table = designation("gaia-dr3-hipparcos2", "4612085076726173440");
    let schemes: Vec<(&str, &str)> = object
        .entries
        .iter()
        .filter_map(|e| match &e.resolved_by {
            Resolution::CrossIdentification {
                scheme,
                identifier,
                confidence,
                ..
            } => {
                assert_eq!(confidence.get(), 1.0);
                Some((scheme.as_str(), identifier.as_str()))
            }
            _ => None,
        })
        .collect();
    assert!(schemes.contains(&("hip", "10768")), "{schemes:?}");
    assert!(schemes.contains(&("gaia-source-id", "4612085076726173440")));
    assert!(object.entry(&table).is_some());
}

#[test]
fn a_shared_2masx_name_joins_entries_beyond_the_positional_tolerance() {
    // AC-66: 2MRS and 2M++ name galaxies by their 2MASX designation; 30" apart is beyond the
    // 2" pair tolerance, yet the shared name resolves them into one object.
    let (ra, dec) = (10.6847, 41.2688);
    let far = offset(ra, dec, 30.0, 0.0);
    let m = match_entries(
        &matching_rules(),
        vec![
            with_cz(entry("twomrs", "00424433+4116074", ra, dec), -300.0, 4.0),
            with_cz(
                entry("twompp-catalogue", "00424433+4116074", far.0, far.1),
                -300.0,
                4.0,
            ),
        ],
    );
    assert_eq!(m.objects.len(), 1);
    let e = &m.objects[0].entries[1];
    assert!(matches!(
        &e.resolved_by,
        Resolution::CrossIdentification { scheme, .. } if scheme == "2masx"
    ));
}

#[test]
fn each_pair_of_sources_has_its_own_tolerances() {
    // AC-68: 2MRS and 6dFGS match within 5" and 200 km/s, and no further.
    let rules = matching_rules();
    let pair = |a: &str, b: &str| {
        rules
            .pairs
            .iter()
            .find(|p| p.sources == [source(a), source(b)])
            .unwrap()
    };
    assert_eq!(pair("twomrs", "sixdfgs-dr3").angular_tolerance_arcsec, 5.0);
    assert_eq!(
        pair("updated-nearby-galaxies", "twomrs").angular_tolerance_arcsec,
        30.0
    );
    assert!(rules.pairs.iter().all(|p| !p.basis.trim().is_empty()));
    let (ra, dec) = (150.0, 2.0);
    let cases = [
        (4.9, 15_100.0, true),
        (5.1, 15_100.0, false),
        (1.0, 15_250.0, false),
    ];
    for (sep, cz, merged) in cases {
        let b = offset(ra, dec, sep, 0.0);
        let m = match_entries(
            &rules,
            vec![
                with_cz(entry("twomrs", "a", ra, dec), 15_000.0, 30.0),
                with_cz(entry("sixdfgs-dr3", "b", b.0, b.1), cz, 45.0),
            ],
        );
        assert_eq!(m.objects.len() == 1, merged, "{sep}\" and cz {cz}");
    }
}

#[test]
fn a_merge_records_method_separation_velocity_difference_and_confidence() {
    // AC-69: 3" apart and 50 km/s in cz, under the 5" and 200 km/s of 2MRS and 6dFGS.
    // AC-70: confidence = (1 - (3/5)^2) * (1 - (50/200)^2) = 0.64 * 0.9375 = 0.6.
    let (ra, dec) = (150.0, 0.0);
    let b = offset(ra, dec, 3.0, 0.0);
    let m = match_entries(
        &matching_rules(),
        vec![
            with_cz(entry("twomrs", "a", ra, dec), 15_000.0, 30.0),
            with_cz(entry("sixdfgs-dr3", "b", b.0, b.1), 15_050.0, 45.0),
        ],
    );
    assert_eq!(m.objects.len(), 1);
    let r = record(&m.objects[0], &designation("sixdfgs-dr3", "b"));
    assert_eq!(r.method, MatchMethod::PositionAndRedshift);
    assert_eq!(r.with, designation("twomrs", "a"));
    assert!(
        (r.separation_arcsec - 3.0).abs() < 1e-6,
        "{}",
        r.separation_arcsec
    );
    let Some(LineOfSight::Velocity { delta_km_s }) = r.difference else {
        panic!("{:?}", r.difference)
    };
    assert!((delta_km_s - 50.0).abs() < 1e-6);
    assert!(
        (r.confidence.get() - 0.6).abs() < 1e-6,
        "{}",
        r.confidence.get()
    );
    assert_eq!(m.report.merged_by_position, 1);
}

#[test]
fn the_confidence_rule_is_stated_in_the_rules_and_applied_as_stated() {
    // AC-70: the statement and its parameters; a pair that compares cz, with one entry
    // lacking it, takes the unchecked factor: (1 - (2/5)^2) * 0.5 = 0.42.
    let rules = matching_rules();
    let c = &rules.confidence;
    assert!(c.statement.contains("(1 - (s / theta)^p) * L"));
    assert_eq!((c.exponent, c.unchecked_factor), (2.0, 0.5));
    let b = offset(150.0, 0.0, 2.0, 0.0);
    let m = match_entries(
        &rules,
        vec![
            with_cz(entry("twomrs", "a", 150.0, 0.0), 15_000.0, 30.0),
            entry("sixdfgs-dr3", "b", b.0, b.1),
        ],
    );
    let r = record(&m.objects[0], &designation("sixdfgs-dr3", "b"));
    assert_eq!((r.method, r.difference), (MatchMethod::Position, None));
    assert!(
        (r.confidence.get() - 0.42).abs() < 1e-6,
        "{}",
        r.confidence.get()
    );
    // A distance pair: UNGC and McConnachie, 180" and 10% apart in distance, under 360" and
    // 30%: (1 - 0.25) * (1 - (0.1/0.3)^2) = 0.6667.
    let how = (
        DistanceKind::PrimaryIndicator,
        Method::TipOfTheRedGiantBranch,
    );
    let b = offset(15.0, -30.0, 180.0, 0.0);
    let m = match_entries(
        &rules,
        vec![
            with_distance(
                entry("updated-nearby-galaxies", "u", 15.0, -30.0),
                1.9,
                None,
                how.clone(),
            ),
            with_distance(entry("mcconnachie-2012", "m", b.0, b.1), 2.1, None, how),
        ],
    );
    let r = record(&m.objects[0], &designation("mcconnachie-2012", "m"));
    assert_eq!(r.method, MatchMethod::PositionAndDistance);
    assert!((r.confidence.get() - 0.75 * (1.0 - 1.0 / 9.0)).abs() < 1e-6);
}

/// A 2MRS galaxy with two 6dFGS galaxies 2" either side and within 200 km/s, one 6dFGS galaxy
/// 60" off, and a 2M++ galaxy 1" from the 2MRS one (all simulated).
fn triple() -> Vec<CatalogueEntry> {
    let (ra, dec) = (200.0, -10.0);
    let (e, w, far, cf) = (
        offset(ra, dec, 2.0, 0.0),
        offset(ra, dec, -2.0, 0.0),
        offset(ra, dec, 60.0, 0.0),
        offset(ra, dec, 0.0, 1.0),
    );
    vec![
        with_cz(entry("twomrs", "a", ra, dec), 8_000.0, 20.0),
        with_cz(entry("sixdfgs-dr3", "east", e.0, e.1), 8_040.0, 45.0),
        with_cz(entry("sixdfgs-dr3", "west", w.0, w.1), 7_980.0, 45.0),
        with_cz(entry("sixdfgs-dr3", "far", far.0, far.1), 8_000.0, 45.0),
        with_cz(entry("twompp-catalogue", "99", cf.0, cf.1), 8_010.0, 10.0),
    ]
}

#[test]
fn an_entry_with_two_candidates_is_kept_apart_and_linked_to_each_with_its_confidence() {
    // AC-71.
    let m = match_entries(&matching_rules(), triple());
    let a = designation("twomrs", "a");
    let (east, west) = (
        designation("sixdfgs-dr3", "east"),
        designation("sixdfgs-dr3", "west"),
    );
    let objects: Vec<usize> = [&a, &east, &west].iter().map(|d| at(&m, d)).collect();
    assert!(objects[0] != objects[1] && objects[0] != objects[2] && objects[1] != objects[2]);
    // The object holding the 2MRS galaxy carries its two links, and those of the 2M++ galaxy
    // merged with it (below).
    let links: Vec<_> = m.objects[objects[0]]
        .links
        .iter()
        .filter(|l| l.entry == a)
        .collect();
    assert_eq!(links.len(), 2, "{links:?}");
    for link in links {
        assert!(link.candidate == east || link.candidate == west);
        assert!(link.confidence.get() > 0.0 && link.confidence.get() < 1.0);
    }
    // The far miss is neither merged nor linked.
    let far = &m.objects[at(&m, &designation("sixdfgs-dr3", "far"))];
    assert!(far.links.is_empty() && far.entries.len() == 1);
    // The 2M++ galaxy has one 2MRS candidate but two 6dFGS ones: it merges with the 2MRS
    // galaxy only where neither side has a rival of the same source. Here it has two 6dFGS
    // rivals, so it is linked to both and merged with the 2MRS galaxy alone.
    let cf = designation("twompp-catalogue", "99");
    assert_eq!(at(&m, &cf), objects[0]);
    assert_eq!(m.report.candidate_links, 4);
}

#[test]
fn no_measurement_is_merged_across_a_candidate_link() {
    // AC-72: no object holds entries from both ends of a link.
    let m = match_entries(&matching_rules(), triple());
    for object in &m.objects {
        for link in &object.links {
            assert!(object.entry(&link.entry).is_some());
            assert!(object.entry(&link.candidate).is_none());
            let other = &m.objects[at(&m, &link.candidate)];
            assert!(other.entry(&link.entry).is_none());
        }
    }
}

#[test]
fn designations_and_common_names_keep_their_sources() {
    // AC-73: each designation with its source, and the HIP number the Gaia-Hipparcos table
    // gives beside the Gaia id, kept under the table's source.
    let m = match_entries(&matching_rules(), gaia_hipparcos());
    let object = &m.objects[at(&m, &designation("hipparcos-2007", "10768"))];
    for d in [
        designation("hipparcos-2007", "10768"),
        designation("gaia-dr3-bright", "4612085076726173440"),
        designation("gaia-dr3-hipparcos2", "HIP 10768"),
        designation("bailer-jones-2021", "4612085076726173440"),
    ] {
        assert!(object.designations().any(|o| *o == d), "{d:?}");
    }
    // AC-74: the LMC of McConnachie 2012 and of UNGC, 40" and 2% apart in distance (simulated
    // positions near the real ones): one object, its common name from McConnachie.
    let how = (DistanceKind::PrimaryIndicator, Method::Cepheids);
    let near = offset(80.8938, -69.7561, 40.0, 0.0);
    let m = match_entries(
        &matching_rules(),
        vec![
            with_distance(
                entry("updated-nearby-galaxies", "LMC", 80.8938, -69.7561),
                0.050,
                None,
                how.clone(),
            ),
            with_distance(
                entry("mcconnachie-2012", "LMC", near.0, near.1),
                0.051,
                None,
                how,
            ),
        ],
    );
    assert_eq!(m.objects.len(), 1);
    let object = &m.objects[0];
    assert_eq!(
        object.names,
        [CommonName {
            name: "LMC".to_owned(),
            source: source("mcconnachie-2012"),
        }]
    );
    assert!(
        object
            .designations()
            .any(|o| o == &designation("updated-nearby-galaxies", "LMC"))
    );
    assert!(
        object
            .designations()
            .any(|o| o == &designation("mcconnachie-2012", "LMC"))
    );
}

#[test]
fn the_rules_read_back_as_written_and_refuse_a_pair_twice() {
    let rules = matching_rules();
    let yaml = serde_norway::to_string(&rules).unwrap();
    assert_eq!(
        serde_norway::from_str::<MatchingRules>(&yaml).unwrap(),
        rules
    );
    let mut twice = rules.clone();
    twice.pairs.push(rules.pairs[0].clone());
    assert!(twice.check().is_err());
}

#[test]
fn forty_thousand_simulated_entries_match_in_pairs() {
    // 20,000 simulated 2MRS galaxies on a 0.1° grid, each with a 6dFGS entry 1" away.
    let mut entries = Vec::new();
    for i in 0..20_000u32 {
        let (ra, dec) = (f64::from(i % 400) * 0.1, f64::from(i / 400) * 0.1 - 2.5);
        let b = offset(ra, dec, 1.0, 0.0);
        entries.push(with_cz(
            entry("twomrs", &format!("a{i}"), ra, dec),
            9_000.0,
            20.0,
        ));
        entries.push(with_cz(
            entry("sixdfgs-dr3", &format!("b{i}"), b.0, b.1),
            9_020.0,
            40.0,
        ));
    }
    let m = match_entries(&matching_rules(), entries);
    assert_eq!(
        (m.report.objects, m.report.merged_by_position),
        (20_000, 20_000)
    );
    assert_eq!(m.report.candidate_links, 0);
}

#[test]
#[ignore = "two million simulated entries: run by hand with --release"]
fn two_million_simulated_entries_match_in_pairs() {
    // 1,000,000 simulated 2MRS galaxies on a 0.12° grid over 360° × 120°, each with a 6dFGS
    // entry 1" away: the scale of a full release.
    let mut entries = Vec::with_capacity(2_000_000);
    for i in 0..1_000_000u32 {
        let (ra, dec) = (
            f64::from(i % 3_000) * 0.12,
            f64::from(i / 3_000) * 0.12 - 20.0,
        );
        let b = offset(ra, dec, 1.0, 0.0);
        entries.push(with_cz(
            entry("twomrs", &format!("a{i}"), ra, dec),
            9_000.0,
            20.0,
        ));
        entries.push(with_cz(
            entry("sixdfgs-dr3", &format!("b{i}"), b.0, b.1),
            9_020.0,
            40.0,
        ));
    }
    let started = std::time::Instant::now();
    let m = match_entries(&matching_rules(), entries);
    eprintln!("2,000,000 entries matched in {:?}", started.elapsed());
    assert_eq!(
        (m.report.objects, m.report.merged_by_position),
        (1_000_000, 1_000_000)
    );
}

#[test]
fn the_matcher_runs_on_every_recorded_answer() {
    // Smoke test on the real recorded rows: every entry lands in one object, and the
    // Bailer-Jones distances join their Gaia DR3 sources by source_id.
    let mut entries = Vec::new();
    for definition in common::read_sources() {
        let fixture = common::fixture_of(&definition);
        if reads_rows(&definition) && fixture.exists() {
            entries.extend(common::normalized_fixture(definition.id.as_str()).entries);
        }
    }
    let n = entries.len();
    let bj: Vec<Designation> = entries
        .iter()
        .filter(|e| e.designation.source.as_str() == "bailer-jones-2021")
        .map(|e| e.designation.clone())
        .collect();
    let m = match_entries(&matching_rules(), entries);
    let held: usize = m.objects.iter().map(|o| o.entries.len()).sum();
    assert_eq!((held, m.report.entries), (n, n as u64));
    assert_ne!(bj.len(), 0);
    for d in &bj {
        let object = m.find(d).unwrap();
        let gaia = Designation {
            source: source("gaia-dr3-bright"),
            ..d.clone()
        };
        assert!(object.entry(&gaia).is_some(), "{d:?}");
    }
    assert!(m.report.cross_identified >= bj.len() as u64);
}
