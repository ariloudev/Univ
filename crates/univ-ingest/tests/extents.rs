//! The extent rule on real recorded rows: IC 5052, an edge-on galaxy 8.13 arcmin long, stands in
//! UNGC and Cosmicflows-4 at its centre, in 2MRS and 2M++ as a 2MASX detection 34 arcsec off,
//! and in 6dFGS as four fibres 6 to 28 arcsec off, all within 51 km/s of its 584 km/s; point-
//! source tolerances leave them six objects, the extent of UNGC makes them one. M51 and its
//! companion NGC 5195, 4.6 arcmin apart within M51's extent at 446 and 455 km/s, stay two.
#![expect(clippy::unwrap_used, reason = "a failed unwrap is the test failing")]

mod common;
mod simulated;

use common::normalized_file;
use simulated::{designation, matching_rules, ranking_rules, with_cz};
use univ_ingest::matching::{MatchingRules, match_entries};
use univ_ingest::ranking::rank_all;
use univ_model::derive::Release;
use univ_model::entry::CatalogueEntry;
use univ_model::object::{LineOfSight, MatchMethod, MeasurementRef, Object, Resolution};

const IC5052: [(&str, &str); 8] = [
    ("updated-nearby-galaxies", "IC5052"),
    ("cosmicflows-4-galaxies", "65603"),
    ("twomrs", "20520163-6911359"),
    ("twompp-catalogue", "20520163-6911359"),
    ("sixdfgs-dr3", "g2052024-691150"),
    ("sixdfgs-dr3", "g2052047-691159"),
    ("sixdfgs-dr3", "g2052063-691209"),
    ("sixdfgs-dr3", "g2052078-691229"),
];

fn recorded() -> Vec<CatalogueEntry> {
    let mut entries = Vec::new();
    for (id, file) in [
        (
            "updated-nearby-galaxies",
            "updated-nearby-galaxies-extents.tsv",
        ),
        (
            "cosmicflows-4-galaxies",
            "cosmicflows-4-galaxies-extents.tsv",
        ),
        ("twomrs", "twomrs-extents.tsv"),
        ("twompp-catalogue", "twompp-catalogue-extents.tsv"),
        ("sixdfgs-dr3", "sixdfgs-dr3-ic5052.tsv"),
    ] {
        entries.extend(normalized_file(id, file).entries);
    }
    entries
}

fn holding<'a>(objects: &'a [Object], source: &str, id: &str) -> &'a Object {
    objects
        .iter()
        .find(|o| o.entry(&designation(source, id)).is_some())
        .unwrap()
}

fn without_extents() -> MatchingRules {
    let mut rules = matching_rules();
    rules.extents = None;
    rules
}

#[test]
fn ic_5052_is_one_object_of_its_pieces_within_its_published_extent() {
    // Without the rule, the pieces stay apart: the 2MASX detection lies beyond the 30 arcsec of
    // UNGC against 2MRS, and 6dFGS fibres beyond 10 arcsec of Cosmicflows-4 or one another's.
    let m = match_entries(&without_extents(), recorded());
    let apart = holding(&m.objects, "updated-nearby-galaxies", "IC5052");
    assert!(
        apart
            .entry(&designation("sixdfgs-dr3", "g2052024-691150"))
            .is_none()
    );
    let mut m = match_entries(&matching_rules(), recorded());
    rank_all(&mut m.objects, &ranking_rules(), &Release::planck_2018());
    let ic5052 = holding(&m.objects, "updated-nearby-galaxies", "IC5052");
    for (source, id) in IC5052 {
        assert!(
            ic5052.entry(&designation(source, id)).is_some(),
            "{source} {id}: {:#?}",
            m.objects
        );
    }
    assert_eq!(ic5052.entries.len(), IC5052.len());
    assert!(ic5052.links.is_empty(), "{:?}", ic5052.links);
    // Each group joins through its entry nearest the centre, and records the rule.
    let pieces: Vec<_> = ic5052
        .entries
        .iter()
        .filter_map(|e| match &e.resolved_by {
            Resolution::Match(r) if r.method == MatchMethod::WithinExtent => Some(r),
            _ => None,
        })
        .collect();
    assert_eq!(pieces.len(), 5, "{pieces:#?}");
    for r in pieces {
        assert_eq!(r.with, designation("updated-nearby-galaxies", "IC5052"));
        assert!(r.separation_arcsec < 4.07 * 60.0);
        assert!(r.confidence.get() > 0.0 && r.confidence.get() <= 1.0);
        let Some(LineOfSight::Velocity { delta_km_s }) = r.difference else {
            panic!("{r:?}")
        };
        assert!(delta_km_s <= 51.5, "{delta_km_s}");
    }
    // The sixth piece is M51's: UNGC centres NGC5194 2.35 arcmin north of the 2MASX nucleus
    // that Cosmicflows-4, 2MRS and 2M++ share, within its 7.9 arcmin radius at 446 against 463
    // km/s.
    assert_eq!(m.report.pieces_within_extent, 6);
    let m51 = holding(&m.objects, "updated-nearby-galaxies", "NGC5194");
    for (source, id) in [
        ("cosmicflows-4-galaxies", "47404"),
        ("twomrs", "13295269+4711429"),
    ] {
        assert!(m51.entry(&designation(source, id)).is_some(), "{m51:#?}");
    }
    // The ranking still prefers a measured distance, here the TRGB of UNGC or Cosmicflows-4.
    let p = ic5052.preferred_distance.as_ref().unwrap();
    assert!(
        matches!(&p.measurement, MeasurementRef::Distance { .. }),
        "{p:?}"
    );
    assert!((p.distance_mpc - 6.0).abs() < 0.5, "{}", p.distance_mpc);
}

#[test]
fn a_companion_with_its_own_identity_stays_apart_within_the_extent() {
    // NGC 5195 lies within M51's 15.85 arcmin and agrees in velocity, but UNGC lists it on its
    // own, and Cosmicflows-4 names it by its own PGC number, 47413 against M51's 47404.
    let m = match_entries(&matching_rules(), recorded());
    let m51 = holding(&m.objects, "updated-nearby-galaxies", "NGC5194");
    assert!(
        m51.entry(&designation("updated-nearby-galaxies", "NGC5195"))
            .is_none()
    );
    // Without its UNGC row, NGC 5195's PGC number keeps it apart, and as the M51 nucleus also
    // holds one, 47404, neither is known to be UNGC's NGC5194: both stay apart, linked to it.
    let entries: Vec<_> = recorded()
        .into_iter()
        .filter(|e| e.designation != designation("updated-nearby-galaxies", "NGC5195"))
        .collect();
    let m = match_entries(&matching_rules(), entries);
    let m51 = holding(&m.objects, "updated-nearby-galaxies", "NGC5194");
    assert_eq!(m51.entries.len(), 1, "{m51:#?}");
    for pgc in ["47404", "47413"] {
        let other = holding(&m.objects, "cosmicflows-4-galaxies", pgc);
        assert!(
            other
                .links
                .iter()
                .any(|l| l.candidate == designation("updated-nearby-galaxies", "NGC5194")),
            "{other:#?}"
        );
    }
    let ngc5195 = holding(&m.objects, "cosmicflows-4-galaxies", "47413");
    assert!(
        ngc5195
            .entry(&designation("twomrs", "13295958+4715580"))
            .is_some()
            && ngc5195
                .entry(&designation("cosmicflows-4-galaxies", "47404"))
                .is_none(),
        "{ngc5195:#?}"
    );
}

#[test]
fn a_piece_at_another_velocity_or_without_one_or_a_galaxy_without_a_size_stays_apart() {
    // A fibre 280 km/s beyond the tolerance, SIMULATED from the recorded one, stays apart.
    let entries: Vec<_> = recorded()
        .into_iter()
        .map(|mut e| {
            if e.designation == designation("sixdfgs-dr3", "g2052078-691229") {
                e.redshifts.clear();
                with_cz(e, 584.0 + 580.0, 45.0)
            } else {
                e
            }
        })
        .collect();
    let m = match_entries(&matching_rules(), entries);
    let ic5052 = holding(&m.objects, "updated-nearby-galaxies", "IC5052");
    assert!(
        ic5052
            .entry(&designation("sixdfgs-dr3", "g2052078-691229"))
            .is_none()
    );
    assert!(
        ic5052
            .entry(&designation("sixdfgs-dr3", "g2052063-691209"))
            .is_some()
    );
    // A fibre without a cz, SIMULATED by dropping the recorded one's, stays apart.
    let entries: Vec<_> = recorded()
        .into_iter()
        .map(|mut e| {
            if e.designation == designation("sixdfgs-dr3", "g2052078-691229") {
                e.redshifts.clear();
                e.velocity = None;
            }
            e
        })
        .collect();
    let m = match_entries(&matching_rules(), entries);
    let ic5052 = holding(&m.objects, "updated-nearby-galaxies", "IC5052");
    assert!(
        ic5052
            .entry(&designation("sixdfgs-dr3", "g2052078-691229"))
            .is_none()
    );
    // With the sizes blanked, no size is supplied: the 6dFGS fibres stay apart.
    let entries: Vec<_> = recorded()
        .into_iter()
        .map(|mut e| {
            e.flags.remove("a26");
            e.flags.remove("Riso");
            e
        })
        .collect();
    let m = match_entries(&matching_rules(), entries);
    assert_eq!(m.report.pieces_within_extent, 0);
    let ic5052 = holding(&m.objects, "updated-nearby-galaxies", "IC5052");
    assert!(
        ic5052
            .entry(&designation("sixdfgs-dr3", "g2052024-691150"))
            .is_none()
    );
    // Nor does a galaxy whose size stands without a cz: IC 5052's UNGC row without its HRV,
    // SIMULATED, beside a 2MRS row without its Riso.
    let entries: Vec<_> = recorded()
        .into_iter()
        .map(|mut e| {
            if e.designation == designation("updated-nearby-galaxies", "IC5052") {
                e.velocity = None;
            }
            e.flags.remove("Riso");
            e
        })
        .collect();
    let m = match_entries(&matching_rules(), entries);
    let ic5052 = holding(&m.objects, "updated-nearby-galaxies", "IC5052");
    assert!(
        ic5052
            .entry(&designation("sixdfgs-dr3", "g2052063-691209"))
            .is_none()
    );
}

#[test]
fn an_extent_rule_without_a_basis_or_with_a_length_unit_is_refused() {
    let mut rules = matching_rules();
    rules.extents.as_mut().unwrap().pieces_basis = " ".into();
    assert_eq!(rules.check().unwrap_err().0, "extents.pieces_basis");
    let mut rules = matching_rules();
    rules.extents.as_mut().unwrap().sources[0].unit = univ_model::units::Unit::Megaparsec;
    assert_eq!(rules.check().unwrap_err().0, "extents.sources[0].unit");
}
