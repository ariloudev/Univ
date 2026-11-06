//! Fibres of one survey on one nearby galaxy that no source gives an extent join as one galaxy:
//! below the extent rule's 1,000 km/s, each within 1 arcmin and 300 km/s of every fibre already
//! joined (cross-match-catalogues).

mod common;
mod simulated;

use simulated::{entry, holding, matching_rules, offset, with_cz};
use univ_ingest::matching::match_entries;
use univ_model::entry::CatalogueEntry;
use univ_model::object::{LineOfSight, MatchMethod, Object, Resolution};

const RA: f64 = 150.0;
const DEC: f64 = 2.0;

/// A DESI fibre `east` and `north` arcseconds from the galaxy's first fibre, at `cz`.
fn fibre(id: &str, east: f64, north: f64, cz: f64) -> CatalogueEntry {
    let (ra, dec) = offset(RA, DEC, east, north);
    with_cz(entry("desi-dr1", id, ra, dec), cz, 5.0)
}

fn ids(o: &Object) -> Vec<String> {
    let mut ids: Vec<String> = o
        .entries
        .iter()
        .map(|e| e.entry.designation.identifier.to_string())
        .collect();
    ids.sort();
    ids
}

#[test]
fn nearby_fibres_of_one_survey_within_an_arcminute_and_300_km_s_are_one_galaxy() {
    let m = match_entries(
        &matching_rules(),
        vec![
            fibre("a", 0.0, 0.0, 400.0),
            fibre("b", 30.0, 0.0, 403.0),
            fibre("c", 0.0, 30.0, 398.0),
        ],
    );
    assert_eq!(m.objects.len(), 1, "{:#?}", m.objects);
    assert_eq!(ids(holding(&m.objects, "desi-dr1", "a")), ["a", "b", "c"]);
    assert_eq!(m.report.fibres_of_one_galaxy, 2);
    // Each joined fibre records the rule, its separation and its velocity difference.
    let joined: Vec<_> = m.objects[0]
        .entries
        .iter()
        .filter_map(|e| match &*e.resolved_by {
            Resolution::Match(r) if r.method == MatchMethod::FibresOfOneGalaxy => Some(r),
            _ => None,
        })
        .collect();
    assert_eq!(joined.len(), 2, "{joined:#?}");
    for r in joined {
        assert!(r.separation_arcsec <= 60.0, "{r:?}");
        let Some(LineOfSight::Velocity { delta_km_s }) = r.difference else {
            panic!("{r:?}")
        };
        assert!(delta_km_s <= 300.0, "{r:?}");
    }
}

#[test]
fn a_fibre_beyond_an_arcminute_of_any_fibre_joined_stays_apart() {
    // d lies 40 arcsec from b but 70 arcsec from a: complete linkage keeps it apart.
    let m = match_entries(
        &matching_rules(),
        vec![
            fibre("a", 0.0, 0.0, 400.0),
            fibre("b", 30.0, 0.0, 400.0),
            fibre("d", 70.0, 0.0, 400.0),
        ],
    );
    assert_eq!(ids(holding(&m.objects, "desi-dr1", "a")), ["a", "b"]);
    assert_eq!(ids(holding(&m.objects, "desi-dr1", "d")), ["d"]);
}

#[test]
fn fibres_beyond_the_nearby_volume_or_its_velocity_tolerance_stay_apart() {
    let m = match_entries(
        &matching_rules(),
        vec![
            fibre("far-1", 0.0, 0.0, 1_500.0),
            fibre("far-2", 20.0, 0.0, 1_500.0),
            fibre("near", 0.0, 20.0, 400.0),
            fibre("other-velocity", 20.0, 20.0, 800.0),
        ],
    );
    assert_eq!(m.objects.len(), 4, "{:#?}", m.objects);
    assert_eq!(m.report.fibres_of_one_galaxy, 0);
}

#[test]
fn two_fibres_each_on_a_galaxy_of_its_own_identity_stay_apart() {
    // Two SDSS fibres 30 arcsec apart at one cz, each merged with a Cosmicflows-4 galaxy of its
    // own PGC number: two galaxies, which the fibre rule never joins.
    let galaxy = |pgc: &str, east: f64| {
        let (ra, dec) = offset(RA, DEC, east, 0.0);
        with_cz(entry("cosmicflows-4-galaxies", pgc, ra, dec), 400.0, 5.0)
    };
    let sdss = |id: &str, east: f64| {
        let (ra, dec) = offset(RA, DEC, east, 0.0);
        with_cz(entry("sdss-dr17-galaxies", id, ra, dec), 400.0, 5.0)
    };
    let m = match_entries(
        &matching_rules(),
        vec![
            galaxy("1001", 0.0),
            sdss("s1", 0.0),
            galaxy("1002", 30.0),
            sdss("s2", 30.0),
        ],
    );
    assert_eq!(
        ids(holding(&m.objects, "sdss-dr17-galaxies", "s1")),
        ["1001", "s1"]
    );
    assert_eq!(
        ids(holding(&m.objects, "sdss-dr17-galaxies", "s2")),
        ["1002", "s2"]
    );
}

#[test]
fn only_the_fibre_surveys_the_rules_list_join() {
    // 2MRS is no fibre survey of the rule: two of its entries 20 arcsec apart at one cz stay two.
    let twomrs = |id: &str, east: f64| {
        let (ra, dec) = offset(RA, DEC, east, 0.0);
        with_cz(entry("twomrs", id, ra, dec), 400.0, 5.0)
    };
    let m = match_entries(&matching_rules(), vec![twomrs("x", 0.0), twomrs("y", 20.0)]);
    assert_eq!(m.objects.len(), 2, "{:#?}", m.objects);
    let listed = &matching_rules().extents.unwrap().fibres.unwrap().surveys;
    assert!(
        listed.iter().any(|s| s.as_str() == "desi-dr1"),
        "{listed:?}"
    );
}

#[test]
fn a_fibre_never_joins_a_galaxy_whose_extent_a_source_publishes_or_a_group_of_a_shared_source() {
    // Two DESI fibres 40 arcsec apart, each merged with an ALFALFA detection 40 arcsec beyond it
    // (80 arcsec from the other fibre): two galaxies of one HI survey, which the fibre rule never
    // makes one.
    let alfalfa = |id: &str, east: f64| {
        let (ra, dec) = offset(RA, DEC, east, 0.0);
        with_cz(entry("alfalfa-a100", id, ra, dec), 400.0, 5.0)
    };
    let m = match_entries(
        &matching_rules(),
        vec![
            alfalfa("h1", -40.0),
            fibre("f1", 0.0, 0.0, 400.0),
            alfalfa("h2", 80.0),
            fibre("f2", 40.0, 0.0, 400.0),
        ],
    );
    assert_eq!(ids(holding(&m.objects, "desi-dr1", "f1")), ["f1", "h1"]);
    assert_eq!(ids(holding(&m.objects, "desi-dr1", "f2")), ["f2", "h2"]);
    assert_eq!(m.report.fibres_of_one_galaxy, 0);
}
