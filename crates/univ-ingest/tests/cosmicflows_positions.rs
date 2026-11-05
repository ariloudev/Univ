//! The Cosmicflows-4 groups whose published positions are wrong (a published-catalogue error):
//! on real recorded rows, the 148 that Kourkchi & Tully 2017 also list take its group centre
//! and lose their velocity-space Cartesian coordinates, and the 9 it does not list are left out.

mod common;

use common::{definition_of, entry};
use univ_ingest::source::{FixAction, LeftOutReason};
use univ_model::position::separation_degrees;
use univ_model::position::unit_vector;

fn recorded() -> univ_ingest::normalize::Normalized {
    common::normalized_file("cosmicflows-4", "cosmicflows-4-wrong-positions.tsv")
}

fn at(e: &univ_model::entry::CatalogueEntry) -> (f64, f64) {
    let p = e.position.as_ref().map(|p| p.icrs);
    p.map_or((f64::NAN, f64::NAN), |p| (p.ra, p.dec))
}

#[test]
fn the_definition_cites_148_group_centres_and_leaves_9_groups_out() {
    let d = definition_of("cosmicflows-4");
    let mut replaced = 0;
    let mut left_out = 0;
    for fix in &d.fixes {
        match &fix.action {
            Some(FixAction::ReplaceByKey {
                key,
                columns,
                drop,
                values,
                ..
            }) => {
                assert_eq!(key, "1PGC");
                assert_eq!(columns, &["_RAJ2000", "_DEJ2000"]);
                assert_eq!(drop, &["SGX", "SGY", "SGZ"]);
                assert!(fix.change.contains("J/ApJ/843/16/table2"));
                replaced += values.len();
            }
            Some(FixAction::LeaveOutWhen { when, reason }) => {
                assert_eq!(*reason, LeftOutReason::WrongPosition);
                assert_eq!(when.column, "1PGC");
                left_out += when.is.as_ref().map_or(0, Vec::len);
            }
            _ => {}
        }
    }
    assert_eq!((replaced, left_out), (148, 9));
}

#[test]
fn virgo_takes_the_kourkchi_tully_centre_and_its_wrong_cartesian_is_dropped() {
    let n = recorded();
    // Published: 1PGC 41220 at 220.8243, -23.9008, 32 degrees from M87.
    let virgo = entry(&n, "41220");
    let (ra, dec) = at(virgo);
    assert!(
        (ra - 187.756).abs() < 0.01 && (dec - 12.406).abs() < 0.01,
        "{ra} {dec}"
    );
    let m87 = unit_vector(187.7058, 12.3912);
    assert!(separation_degrees(unit_vector(ra, dec), m87) < 0.1);
    assert!(virgo.supergalactic_velocity.is_none());
    // Its distance and velocity (V3k, in the CMB frame) are kept as published.
    assert_eq!(virgo.distances[0].native.value, 31.048);
    assert_eq!(virgo.velocity.as_ref().unwrap().native.value, 1479.0);
    // 1PGC 1816 had KT17's galactic angles as its supergalactic ones; it takes KT17's centre,
    // where its one table2 member lies (7.4238, -51.5207).
    let (ra, dec) = at(entry(&n, "1816"));
    assert!(
        (ra - 7.4237).abs() < 0.001 && (dec + 51.5207).abs() < 0.001,
        "{ra} {dec}"
    );
    // Centaurus, 1PGC 43296.
    let (ra, dec) = at(entry(&n, "43296"));
    assert!((ra - 191.98).abs() < 0.01 && (dec + 41.22).abs() < 0.01);
}

#[test]
fn a_group_kourkchi_tully_does_not_list_is_left_out_for_its_wrong_position() {
    let n = recorded();
    assert_eq!(n.entries.len(), 3);
    assert_eq!(
        n.report.left_out.get(&LeftOutReason::WrongPosition),
        Some(&1)
    );
    assert!(
        n.entries
            .iter()
            .all(|e| &*e.designation.identifier != "43423")
    );
    // Provenance counts the entries each fix changed: three replaced, one left out, none read
    // SGX, SGY and SGZ as km/s, as the replacement dropped them, and none is the Milky Way's
    // group, whose distance the last fix drops.
    let changed: Vec<u64> = n.report.fixes.iter().map(|f| f.entries_changed).collect();
    assert_eq!(changed, [0, 3, 1, 0], "{:?}", n.report.fixes);
}

#[test]
fn a_replaced_group_position_names_kourkchi_and_tully_and_the_fix() {
    // B5: the look-up reader sees, on the entry itself, where its position comes from; a group
    // whose position is as published carries no such flag.
    let n = recorded();
    let virgo = entry(&n, "41220");
    let flag = &virgo.flags.get("position_source").unwrap();
    for text in [
        "Kourkchi & Tully 2017",
        "ApJ 843, 16",
        "J/ApJ/843/16/table2",
        "fixes[1] of the cosmicflows-4 definition",
    ] {
        assert!(flag.contains(text), "{text} in {flag}");
    }
    let published = common::normalized_fixture("cosmicflows-4");
    assert!(
        published
            .entries
            .iter()
            .all(|e| !e.flags.contains_key("position_source"))
    );
    let d = definition_of("cosmicflows-4");
    assert!(d.citation.contains("J/ApJ/843/16"), "{}", d.citation);
    assert!(d.terms.credit.contains("Kourkchi & Tully 2017"));
    let Some(FixAction::ReplaceByKey { mark, .. }) = &d.fixes[1].action else {
        panic!("{:?}", d.fixes[1].action)
    };
    assert_eq!(mark.flag, "position_source");
}

#[test]
#[should_panic(expected = "no-such-answer.tsv")]
fn a_missing_recorded_answer_fails_rather_than_reading_as_empty() {
    common::normalized_file("cosmicflows-4", "no-such-answer.tsv");
}
