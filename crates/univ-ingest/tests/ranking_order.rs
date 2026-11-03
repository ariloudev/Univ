//! The total order of the preferred distance (RQ-24 under the decision "Rank a distance without
//! an uncertainty by its method"): redshift distances above the rule's threshold set aside where
//! an indicator distance exists, the smallest published fractional uncertainty leading, and a
//! distance without one winning only by a strictly better method rank. Every object here is
//! SIMULATED under the repository's `rules/ranking.yaml` and the Planck 2018 cosmology; the
//! M81-like case borrows the shape of release 1's M81 (a Cepheid distance without an error
//! against a redshift whose fractional uncertainty is about 6), not its values.
#![expect(clippy::unwrap_used, reason = "a failed unwrap is the test failing")]

mod common;
mod simulated;

use simulated::{entry, ranking_rules, with_cz, with_distance, with_modulus};
use univ_ingest::crossmatch::place;
use univ_ingest::ranking::{RankingRules, rank};
use univ_model::derive::Release;
use univ_model::distance::{DistanceKind, Method};
use univ_model::entry::CatalogueEntry;
use univ_model::object::{
    Decided, MeasurementRef, MethodRank, NotPlacedReason, Object, ObjectEntry, Placement,
    PreferredDistance, Resolution,
};

const TF: (DistanceKind, Method) = (DistanceKind::SecondaryIndicator, Method::TullyFisher);
const CEPHEIDS: (DistanceKind, Method) = (DistanceKind::PrimaryIndicator, Method::Cepheids);
const TRGB: (DistanceKind, Method) = (
    DistanceKind::PrimaryIndicator,
    Method::TipOfTheRedGiantBranch,
);
const SN_IA: (DistanceKind, Method) = (DistanceKind::SecondaryIndicator, Method::TypeIaSupernovae);

/// RA 78°, Dec 0°: about 90° from the CMB dipole's apex, so a CMB-frame cz is within some tens
/// of km/s of the heliocentric one (see `ranking.rs`).
const RA: f64 = 78.0;

fn ranked_object(entries: Vec<CatalogueEntry>, rules: &RankingRules) -> Object {
    let mut o = Object::new(
        entries
            .into_iter()
            .map(|entry| ObjectEntry {
                entry,
                resolved_by: Resolution::Anchor,
            })
            .collect(),
    );
    rank(&mut o, rules, &Release::planck_2018());
    o
}

fn preferred(entries: Vec<CatalogueEntry>) -> PreferredDistance {
    ranked_object(entries, &ranking_rules())
        .preferred_distance
        .unwrap()
}

fn distance(
    src: &str,
    mpc: f64,
    sigma: Option<f64>,
    how: (DistanceKind, Method),
) -> CatalogueEntry {
    with_distance(entry(src, "d", RA, 0.0), mpc, sigma, how)
}

fn redshift(cz: f64) -> CatalogueEntry {
    with_cz(entry("twomrs", "z", RA, 0.0), cz, 10.0)
}

fn fraction(p: &PreferredDistance) -> Option<f64> {
    p.uncertainty.total()
}

#[test]
fn a_cepheid_distance_without_an_error_beats_a_redshift_six_times_uncertain() {
    // M81-like: 3.63 Mpc by Cepheids, no error, against cz = 50 km/s, whose peculiar-velocity
    // term alone is 300/50 = 6 times the distance.
    let p = preferred(vec![
        redshift(50.0),
        distance("updated-nearby-galaxies", 3.63, None, CEPHEIDS),
    ]);
    assert_eq!(p.rank, MethodRank::PrimaryIndicator);
    assert_eq!(p.decided, Decided::MethodRankNoneReported);
    assert!(fraction(&p).is_none(), "no uncertainty is invented");
    let lost = p.runner_up.unwrap().1.total().unwrap();
    assert!(lost > 5.0 && lost < 8.0, "{lost}");
}

#[test]
fn a_trgb_distance_without_an_error_beats_a_ten_percent_redshift_by_method_rank() {
    // cz = 3,000 km/s: (300 ⊕ 10)/3,000 ≈ 10%, under the threshold, so not set aside; the TRGB
    // distance has no uncertainty and a strictly better method rank.
    let p = preferred(vec![
        redshift(3_000.0),
        distance("updated-nearby-galaxies", 40.0, None, TRGB),
    ]);
    assert_eq!(
        (p.rank, p.decided),
        (
            MethodRank::PrimaryIndicator,
            Decided::MethodRankNoneReported
        )
    );
    let lost = p.runner_up.unwrap().1.total().unwrap();
    assert!((lost - 0.10).abs() < 0.01, "{lost}");
}

#[test]
fn a_five_percent_distance_of_the_same_method_rank_beats_one_without_an_error() {
    // A TRGB distance without an error is not strictly better ranked than a 5% primary
    // indicator, so the 5% distance leads.
    let sn_primary = (DistanceKind::PrimaryIndicator, Method::TypeIaSupernovae);
    let p = preferred(vec![
        distance("updated-nearby-galaxies", 20.0, None, TRGB),
        distance("cosmicflows-4", 20.0, Some(1.0), sn_primary),
    ]);
    assert_eq!(
        (p.rank, p.decided, fraction(&p)),
        (
            MethodRank::PrimaryIndicator,
            Decided::Uncertainty,
            Some(0.05)
        )
    );
    // Where the 5% distance is a secondary indicator, as Cosmicflows-4 and UNGC class type Ia
    // supernovae, the TRGB distance's method rank is strictly better, and it wins: the risk the
    // decision names.
    let p = preferred(vec![
        distance("updated-nearby-galaxies", 20.0, None, TRGB),
        distance("cosmicflows-4", 20.0, Some(1.0), SN_IA),
    ]);
    assert_eq!(
        (p.rank, p.decided),
        (
            MethodRank::PrimaryIndicator,
            Decided::MethodRankNoneReported
        )
    );
}

#[test]
fn no_loop_a_tully_fisher_distance_without_an_error_does_not_beat_a_5_percent_redshift() {
    // SN Ia 8%, spec-z 5%, TF without an error: TF is not strictly better ranked than the SN Ia
    // distance, so it does not qualify; the 5% redshift (under the threshold) leads.
    let p = preferred(vec![
        distance("cosmicflows-4", 100.0, Some(8.0), SN_IA),
        redshift(6_000.0),
        distance("updated-nearby-galaxies", 90.0, None, TF),
    ]);
    assert!(matches!(p.measurement, MeasurementRef::Redshift { .. }));
    assert_eq!(p.decided, Decided::Uncertainty);
    let f = fraction(&p).unwrap();
    assert!((f - 0.05).abs() < 0.005, "{f}");
}

#[test]
fn a_redshift_above_the_threshold_is_set_aside_for_an_indicator_distance() {
    // TF ±0.8 mag (about 38%) against cz = 1,000 km/s (about 30%): the redshift is the more
    // precise, but above the 0.25 threshold, and a secondary indicator exists.
    let tf = with_modulus(entry("cosmicflows-4", "1", RA, 0.0), 30.25, 0.8, TF);
    let rules = ranking_rules();
    assert!((rules.redshift_threshold - 0.25).abs() < 1e-12);
    let p = preferred(vec![tf.clone(), redshift(1_000.0)]);
    assert!(matches!(p.measurement, MeasurementRef::Distance { .. }));
    assert_eq!(p.decided, Decided::RedshiftThreshold);
    // With a threshold above the redshift's fraction, the redshift wins by uncertainty.
    let mut loose = rules;
    loose.redshift_threshold = 0.5;
    let p = ranked_object(vec![tf, redshift(1_000.0)], &loose)
        .preferred_distance
        .unwrap();
    assert!(matches!(p.measurement, MeasurementRef::Redshift { .. }));
    assert_eq!(p.decided, Decided::Uncertainty);
    // Without an indicator distance nothing is set aside: two loose redshifts still compete.
    let p = preferred(vec![
        redshift(1_000.0),
        with_cz(entry("sixdfgs-dr3", "y", RA, 0.0), 1_100.0, 10.0),
    ]);
    assert!(matches!(p.measurement, MeasurementRef::Redshift { .. }));
    assert_eq!(p.decided, Decided::Uncertainty);
}

#[test]
fn among_distances_without_an_error_the_best_method_then_the_rule_s_source_order_wins() {
    let rules = ranking_rules();
    let first = |a: &str, b: &str| {
        rules.source_place(&simulated::source(a)) < rules.source_place(&simulated::source(b))
    };
    assert!(first("mcconnachie-2012", "updated-nearby-galaxies"));
    let ungc = distance("updated-nearby-galaxies", 0.8, None, TRGB);
    let lg = with_distance(entry("mcconnachie-2012", "m", RA, 0.0), 0.78, None, TRGB);
    let p = preferred(vec![ungc.clone(), lg]);
    assert_eq!(p.decided, Decided::SourceOrder);
    assert_eq!(p.measurement.entry().source.as_str(), "mcconnachie-2012");
    let tf = distance("cosmicflows-4", 0.9, None, TF);
    let p = preferred(vec![tf, ungc]);
    assert_eq!(
        (p.rank, p.decided),
        (
            MethodRank::PrimaryIndicator,
            Decided::MethodRankNoneReported
        )
    );
}

#[test]
fn the_threshold_and_the_source_order_are_recorded_for_the_manifest() {
    // AC-62 with the decision's obligation: the threshold is a parameter of the rule.
    let rules = ranking_rules();
    let yaml = rules.record().unwrap();
    for text in [
        "redshift_threshold: 0.25",
        "redshift_threshold_basis:",
        "source_order:",
        "- cosmicflows-4-galaxies",
    ] {
        assert!(yaml.contains(text), "{text}");
    }
    let mut bad = rules.clone();
    bad.redshift_threshold = 0.0;
    assert!(bad.check().is_err());
    let mut twice = rules;
    twice.source_order.push(simulated::source("twomrs"));
    assert!(twice.check().is_err());
}

#[test]
fn the_object_names_the_comparison_that_decided() {
    // AC-63: each decision is written by name.
    let tf = with_modulus(entry("cosmicflows-4", "1", RA, 0.0), 30.25, 0.8, TF);
    let p = preferred(vec![tf, redshift(1_000.0)]);
    let yaml = serde_norway::to_string(&p).unwrap();
    assert!(yaml.contains("decided: redshift_threshold"), "{yaml}");
    let p = preferred(vec![
        redshift(50.0),
        distance("updated-nearby-galaxies", 3.63, None, CEPHEIDS),
    ]);
    let yaml = serde_norway::to_string(&p).unwrap();
    assert!(
        yaml.contains("decided: method_rank_none_reported"),
        "{yaml}"
    );
}

#[test]
fn an_object_whose_distance_is_more_than_100_percent_uncertain_is_not_placed() {
    // A secondary-indicator distance of 2 ± 3 Mpc: kept as the preferred distance, for look-ups,
    // but given no three-dimensional position, so it is in no tile and no cell.
    let release = Release::planck_2018();
    let mut o = ranked_object(
        vec![distance("updated-nearby-galaxies", 2.0, Some(3.0), TF)],
        &ranking_rules(),
    );
    assert_eq!(
        o.placement,
        Placement::NotPlaced {
            reason: NotPlacedReason::DistanceTooUncertain
        }
    );
    assert_eq!(
        NotPlacedReason::DistanceTooUncertain.describe(),
        "distance too uncertain"
    );
    assert!(o.preferred_distance.is_some() && o.preferred_position.is_some());
    assert!(place(&mut o, &release, &ranking_rules()).is_none());
    assert!(!o.placement.is_placed());
    // At 90% it is placed.
    let mut o = ranked_object(
        vec![distance("updated-nearby-galaxies", 2.0, Some(1.8), TF)],
        &ranking_rules(),
    );
    assert!(o.placement.is_placed());
    assert!(place(&mut o, &release, &ranking_rules()).is_some());
}

#[test]
fn a_group_s_redshift_without_an_error_beats_its_uncertain_modulus() {
    // Release 2's object 522737, the Cosmicflows-4 group of 1PGC 503048: DM 37.97 ± 2.11 mag
    // (about 393 Mpc, over 100% uncertain) and Vh = 14,356 km/s with no error published. The
    // redshift's peculiar-velocity term, about 2%, makes it the candidate with the smaller
    // uncertainty, and the group is placed by it rather than left unplaced.
    let (ra, dec) = (204.445_587, -47.333_500);
    let group = simulated::with_bare_cz(
        with_modulus(entry("cosmicflows-4", "503048", ra, dec), 37.97, 2.11, TF),
        14_356.0,
    );
    let o = ranked_object(vec![group], &ranking_rules());
    let p = o.preferred_distance.as_ref().unwrap();
    assert!(
        matches!(p.measurement, MeasurementRef::Redshift { .. }),
        "{p:?}"
    );
    assert_eq!(p.decided, Decided::Uncertainty);
    let f = fraction(p).unwrap();
    assert!(f > 0.015 && f < 0.025, "{f}");
    assert!(o.placement.is_placed());
}

#[test]
fn m81_keeps_its_cepheid_distance_whatever_its_redshift_s_error() {
    // M81: UNGC's Cepheid 3.63 Mpc without an error, 2MRS and 2M++ cz = -34 ± 4 km/s, about
    // +48 km/s in the CMB frame. The redshift distance is several times uncertain, far above the
    // threshold, with or without its own error.
    let (ra, dec) = (148.888_25, 69.065_28);
    let cepheids = with_distance(
        entry("updated-nearby-galaxies", "MESSIER081", ra, dec),
        3.63,
        None,
        CEPHEIDS,
    );
    for bare in [false, true] {
        let z = if bare {
            simulated::with_bare_cz(entry("twomrs", "09553318+6903549", ra, dec), -34.0)
        } else {
            with_cz(entry("twomrs", "09553318+6903549", ra, dec), -34.0, 4.0)
        };
        let p = preferred(vec![z, cepheids.clone()]);
        assert_eq!(p.rank, MethodRank::PrimaryIndicator, "bare: {bare}");
        assert!((p.distance_mpc - 3.63).abs() < 1e-12);
        let lost = p.runner_up.unwrap().1.total().unwrap();
        assert!(lost > 4.0, "bare: {bare}: {lost}");
    }
}

#[test]
fn an_object_placed_only_by_a_redshift_without_an_error_below_300_km_s_is_too_uncertain() {
    // cz = 200 km/s published without an error, the only candidate: its 300 km/s
    // peculiar-velocity term is about 150% of its distance, so it is kept for look-ups but not
    // placed.
    let z = simulated::with_bare_cz(entry("twomrs", "z", RA, 0.0), 200.0);
    let o = ranked_object(vec![z], &ranking_rules());
    let p = o.preferred_distance.as_ref().unwrap();
    assert!(fraction(p).unwrap() > 1.0, "{p:?}");
    assert_eq!(
        o.placement,
        Placement::NotPlaced {
            reason: NotPlacedReason::DistanceTooUncertain
        }
    );
    // At 3,000 km/s the same redshift is placed.
    let z = simulated::with_bare_cz(entry("twomrs", "z", RA, 0.0), 3_000.0);
    assert!(
        ranked_object(vec![z], &ranking_rules())
            .placement
            .is_placed()
    );
}

#[test]
fn the_too_uncertain_limit_is_the_rule_s_parameter_recorded_for_the_manifest() {
    let rules = ranking_rules();
    assert!((rules.too_uncertain_above - 1.0).abs() < 1e-12);
    let yaml = rules.record().unwrap();
    for text in ["too_uncertain_above: 1.0", "too_uncertain_basis:"] {
        assert!(yaml.contains(text), "{text}");
    }
    // A rule with a looser limit places the 150% redshift distance the repository's leaves out.
    let z = simulated::with_bare_cz(entry("twomrs", "z", RA, 0.0), 200.0);
    let mut loose = rules.clone();
    loose.too_uncertain_above = 2.0;
    assert!(ranked_object(vec![z], &loose).placement.is_placed());
    let mut bad = rules;
    bad.too_uncertain_above = 0.0;
    assert!(bad.check().is_err());
}

#[test]
fn the_comparison_recorded_is_the_one_that_decided_between_tied_candidates() {
    // AC-63: within the tie tolerance (0.001), the method rank decides only where the ranks
    // differ; between candidates of one rank, the smaller fraction decides, then the order of
    // the entries.
    let a = |f: f64| distance("cosmicflows-4", 10.0, Some(10.0 * f), TF);
    let b = |f: f64| distance("updated-nearby-galaxies", 10.0, Some(10.0 * f), TF);
    let p = preferred(vec![a(0.1005), b(0.1)]);
    assert_eq!((p.decided, fraction(&p)), (Decided::Uncertainty, Some(0.1)));
    let p = preferred(vec![a(0.1), b(0.1)]);
    assert_eq!(p.decided, Decided::EntryOrder);
    assert_eq!(p.measurement.entry().source.as_str(), "cosmicflows-4");
    let cepheids = distance("updated-nearby-galaxies", 10.0, Some(1.005), CEPHEIDS);
    let p = preferred(vec![a(0.1), cepheids]);
    assert_eq!(
        (p.rank, p.decided),
        (MethodRank::PrimaryIndicator, Decided::MethodRank)
    );
}

#[test]
fn the_source_order_is_recorded_only_where_the_sources_differ() {
    // Two distances without an error of one method and one source: the entry order decided.
    let one = distance("updated-nearby-galaxies", 3.0, None, TRGB);
    let two = with_distance(
        entry("updated-nearby-galaxies", "e", RA, 0.0),
        3.1,
        None,
        TRGB,
    );
    let p = preferred(vec![one.clone(), two]);
    assert_eq!(p.decided, Decided::EntryOrder);
    assert!((p.distance_mpc - 3.0).abs() < 1e-12);
    let lg = with_distance(entry("mcconnachie-2012", "m", RA, 0.0), 3.1, None, TRGB);
    let p = preferred(vec![one, lg]);
    assert_eq!(p.decided, Decided::SourceOrder);
    let yaml = serde_norway::to_string(&Decided::EntryOrder).unwrap();
    assert_eq!(yaml.trim(), "entry_order");
}
