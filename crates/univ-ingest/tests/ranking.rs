//! The preferred measurement (RQ-28..RQ-30, AC-60..AC-65) on SIMULATED objects under the
//! repository's `rules/ranking.yaml` and the Planck 2018 cosmology, then on the real recorded
//! answers as a smoke test.
#![expect(clippy::unwrap_used, reason = "a failed unwrap is the test failing")]

mod common;
mod simulated;

use simulated::{
    designation, entry, matching_rules, offset, ranking_rules, with_cz, with_distance, with_error,
    with_modulus,
};
use univ_ingest::matching::match_entries;
use univ_ingest::normalize::reads_rows;
use univ_ingest::ranking::{RankingRules, rank, rank_all};
use univ_model::derive::Release;
use univ_model::distance::{DistanceKind, Method};
use univ_model::entry::CatalogueEntry;
use univ_model::object::{
    Decided, FractionalUncertainty, MeasurementRef, MethodRank, NotPlacedReason, Object,
    ObjectEntry, Placement, PositionClass, Resolution,
};

const TF: (DistanceKind, Method) = (DistanceKind::SecondaryIndicator, Method::TullyFisher);

fn object(entries: Vec<CatalogueEntry>) -> Object {
    Object::new(
        entries
            .into_iter()
            .map(|entry| ObjectEntry {
                entry,
                resolved_by: Resolution::Anchor,
            })
            .collect(),
    )
}

fn ranked(entries: Vec<CatalogueEntry>, rules: &RankingRules) -> Object {
    let mut o = object(entries);
    rank(&mut o, rules, &Release::planck_2018());
    o
}

/// A simulated galaxy at cz with a Tully-Fisher modulus of ±0.4 mag. It lies 90° from the CMB
/// dipole's apex (l = 264.021°, b = 48.253° is RA 168.0°, Dec −7.0° in ICRS to a degree), so
/// that its CMB-frame redshift is its heliocentric one to better than 1e-4.
fn tf_and_redshift(cz: f64, mu: f64) -> Vec<CatalogueEntry> {
    // RA 78°, Dec 0° is 90° from (168°, 0°); the apex lies 7° south of that, which moves cz by
    // 370 sin 7° cos(...) ≈ under 45 km/s at most: the comparison below allows for it.
    vec![
        with_modulus(entry("cosmicflows-4", "1", 78.0, 0.0), mu, 0.4, TF),
        with_cz(entry("sixdfgs-dr3", "g", 78.0, 0.0), cz, 30.0),
    ]
}

fn total(u: FractionalUncertainty) -> f64 {
    u.total().unwrap()
}

#[test]
fn a_redshift_distance_beats_tully_fisher_at_15000_km_s_and_loses_at_1000() {
    // AC-60. A modulus ±0.4 mag spans d·10^(−0.08) to d·10^(+0.08): −16.8% and +20.2%, a mean
    // fractional error of 18.5%. The redshift distance's error is D'(z)·sqrt(σz² + ((1+z)·300/c)²)
    // over D(z): at cz = 15,000 km/s, (1.05·300 ⊕ 30)/15,000 ≈ 2.1%, so the redshift wins; at
    // cz = 1,000 km/s, (1.003·300 ⊕ 30)/1,000 ≈ 30%, so the Tully-Fisher distance wins.
    let rules = ranking_rules();
    let far = ranked(tf_and_redshift(15_000.0, 36.15), &rules);
    let p = far.preferred_distance.unwrap();
    assert!(matches!(p.measurement, MeasurementRef::Redshift { .. }));
    assert_eq!(
        (p.rank, p.decided),
        (MethodRank::SpectroscopicRedshift, Decided::Uncertainty)
    );
    let won = total(p.uncertainty);
    assert!((won - 0.021).abs() < 0.001, "redshift {won}");
    let FractionalUncertainty::Reported {
        peculiar_velocity,
        measurement,
        ..
    } = p.uncertainty
    else {
        panic!()
    };
    assert!(peculiar_velocity.unwrap() > 10.0 * measurement.unwrap());
    let (lost, tf) = p.runner_up.unwrap();
    assert!(matches!(lost, MeasurementRef::Distance { .. }));
    assert!((total(tf) - 0.185).abs() < 0.001, "TF {}", total(tf));

    let near = ranked(tf_and_redshift(1_000.0, 30.25), &rules);
    let p = near.preferred_distance.unwrap();
    assert!(matches!(p.measurement, MeasurementRef::Distance { .. }));
    assert!((total(p.uncertainty) - 0.185).abs() < 0.001);
    let (_, z) = p.runner_up.unwrap();
    assert!((total(z) - 0.30).abs() < 0.02, "redshift {}", total(z));
}

#[test]
fn the_peculiar_velocity_term_is_the_rule_s_parameter() {
    // AC-60: a rule setting 100 km/s instead makes the redshift win at 1,000 km/s (≈ 10%).
    let mut rules = ranking_rules();
    assert_eq!(rules.peculiar_velocity_km_s, 300.0);
    rules.peculiar_velocity_km_s = 100.0;
    let p = ranked(tf_and_redshift(1_000.0, 30.25), &rules)
        .preferred_distance
        .unwrap();
    assert!(matches!(p.measurement, MeasurementRef::Redshift { .. }));
    assert!(
        (total(p.uncertainty) - 0.105).abs() < 0.01,
        "{}",
        total(p.uncertainty)
    );
}

#[test]
fn a_tie_is_broken_by_method_rank() {
    // AC-61: 5% from the tip of the red giant branch and 5% from Tully-Fisher: the primary
    // indicator wins; against a 5% parallax distance, the geometric one wins.
    let rules = ranking_rules();
    let trgb = (
        DistanceKind::PrimaryIndicator,
        Method::TipOfTheRedGiantBranch,
    );
    let a = with_distance(
        entry("updated-nearby-galaxies", "a", 10.0, 10.0),
        4.0,
        Some(0.2),
        TF,
    );
    let b = with_distance(
        entry("mcconnachie-2012", "b", 10.0, 10.0),
        4.1,
        Some(0.205),
        trgb,
    );
    let p = ranked(vec![a.clone(), b.clone()], &rules)
        .preferred_distance
        .unwrap();
    assert_eq!(
        (p.rank, p.decided),
        (MethodRank::PrimaryIndicator, Decided::MethodRank)
    );
    let geo = (
        DistanceKind::Geometric,
        Method::Other("eclipsing binary".into()),
    );
    let c = with_distance(entry("gcns", "c", 10.0, 10.0), 4.0, Some(0.2), geo);
    let p = ranked(vec![a, b, c], &rules).preferred_distance.unwrap();
    assert_eq!(p.rank, MethodRank::Geometric);
    assert_eq!(rules.method_rank[0], MethodRank::Geometric);
    assert_eq!(rules.method_rank[4], MethodRank::PhotometricRedshift);
}

#[test]
fn the_ranking_rule_and_its_parameters_are_recorded_for_the_manifest() {
    // AC-62.
    let rules = ranking_rules();
    let yaml = rules.record().unwrap();
    for text in [
        "peculiar_velocity_km_s: 300",
        "tie_tolerance: 0.001",
        "- geometric",
    ] {
        assert!(yaml.contains(text), "{text}");
    }
    assert_eq!(
        serde_norway::from_str::<RankingRules>(&yaml).unwrap(),
        rules
    );
    let mut bad = rules;
    bad.method_rank.pop();
    assert!(bad.check().is_err());
}

#[test]
fn the_object_names_its_preferred_measurement_and_the_uncertainty_that_made_it_win() {
    // AC-63: the measurement by entry and place, its uncertainty with both terms, what
    // decided it and the measurement it beat.
    let o = ranked(tf_and_redshift(15_000.0, 36.15), &ranking_rules());
    let p = o.preferred_distance.as_ref().unwrap();
    assert_eq!(
        p.measurement,
        MeasurementRef::Redshift {
            entry: designation("sixdfgs-dr3", "g"),
            index: 0
        }
    );
    assert!(
        p.distance_mpc > 200.0 && p.distance_mpc < 230.0,
        "{}",
        p.distance_mpc
    );
    let yaml = serde_norway::to_string(p).unwrap();
    for text in [
        "measurement: redshift",
        "peculiar_velocity:",
        "decided: uncertainty",
        "runner_up:",
    ] {
        assert!(yaml.contains(text), "{text} in {yaml}");
    }
    assert_eq!(o.placement, Placement::Placed);
}

#[test]
fn gaia_is_preferred_over_hipparcos_and_either_over_a_survey_unless_an_error_says_otherwise() {
    // AC-64.
    let rules = ranking_rules();
    let gaia = || with_error(entry("gaia-dr3-bright", "g", 45.0, 30.0), 0.02, 2016.0);
    let hip = |mas| with_error(entry("hipparcos-2007", "h", 45.0, 30.0), mas, 1991.25);
    let survey = || entry("twomrs", "s", 45.0, 30.0);
    let p = ranked(vec![survey(), hip(0.8), gaia()], &rules)
        .preferred_position
        .unwrap();
    assert_eq!(
        (p.class, p.entry),
        (PositionClass::Gaia, designation("gaia-dr3-bright", "g"))
    );
    assert!((p.uncertainty_mas.unwrap() - 0.02).abs() < 1e-9);
    let p = ranked(vec![survey(), hip(0.8)], &rules)
        .preferred_position
        .unwrap();
    assert_eq!(p.class, PositionClass::Hipparcos);
    // A Hipparcos error smaller than the Gaia one wins: the published uncertainty decides.
    let p = ranked(vec![hip(0.01), gaia()], &rules)
        .preferred_position
        .unwrap();
    assert_eq!(p.class, PositionClass::Hipparcos);
    let p = ranked(vec![survey()], &rules).preferred_position.unwrap();
    assert_eq!((p.class, p.uncertainty_mas), (PositionClass::Survey, None));
}

#[test]
fn an_object_without_a_distance_is_found_but_not_placed() {
    // AC-65: a simulated galaxy with a position and no distance or redshift: it is found by its
    // designation, carries its sky position and is marked `placed: no` with its reason.
    let lone = entry("sixdfgs-dr3", "lone", 300.0, -40.0);
    let mut m = match_entries(&matching_rules(), vec![lone]);
    rank_all(&mut m.objects, &ranking_rules(), &Release::planck_2018());
    let o = m.find(&designation("sixdfgs-dr3", "lone")).unwrap();
    assert_eq!(
        o.placement,
        Placement::NotPlaced {
            reason: NotPlacedReason::NoDistance
        }
    );
    assert!(o.preferred_distance.is_none() && o.preferred_position.is_some());
    let yaml = serde_norway::to_string(&o.placement).unwrap();
    assert!(
        yaml.contains("placed: no") || yaml.contains("placed: 'no'"),
        "{yaml}"
    );
    assert!(yaml.contains("reason: no_distance"));
}

#[test]
fn a_candidate_link_never_lends_its_measurements() {
    // AC-72 with AC-63: the 2MRS galaxy linked to two 6dFGS candidates (simulated) is placed by
    // its own redshift, never by theirs.
    let (ra, dec) = (200.0, -10.0);
    let (e, w) = (offset(ra, dec, 2.0, 0.0), offset(ra, dec, -2.0, 0.0));
    let entries = vec![
        entry("twomrs", "a", ra, dec),
        with_cz(entry("sixdfgs-dr3", "east", e.0, e.1), 8_040.0, 45.0),
        with_cz(entry("sixdfgs-dr3", "west", w.0, w.1), 7_980.0, 45.0),
    ];
    let mut m = match_entries(&matching_rules(), entries);
    rank_all(&mut m.objects, &ranking_rules(), &Release::planck_2018());
    let a = m.find(&designation("twomrs", "a")).unwrap();
    assert_eq!(a.links.len(), 2);
    assert!(
        a.preferred_distance.is_none(),
        "it has no measurement of its own"
    );
    for o in &m.objects {
        if let Some(p) = &o.preferred_distance {
            assert!(o.entry(p.measurement.entry()).is_some());
        }
    }
}

#[test]
fn every_recorded_answer_ranks() {
    // Smoke test on the real recorded rows: every object with a distance, a fine parallax or a
    // positive redshift is placed, and every placed object's measurement is its own.
    let mut entries = Vec::new();
    for definition in common::read_sources() {
        if reads_rows(&definition) && common::fixture_of(&definition).exists() {
            entries.extend(common::normalized_fixture(definition.id.as_str()).entries);
        }
    }
    let mut m = match_entries(&matching_rules(), entries);
    rank_all(&mut m.objects, &ranking_rules(), &Release::planck_2018());
    let placed = m.objects.iter().filter(|o| o.placement.is_placed()).count();
    assert!(
        placed > 0 && placed < m.objects.len(),
        "{placed} of {}",
        m.objects.len()
    );
    for o in &m.objects {
        assert_eq!(o.placement.is_placed(), o.preferred_distance.is_some());
        if let Some(p) = &o.preferred_distance {
            assert!(o.entry(p.measurement.entry()).is_some());
            assert!(p.distance_mpc > 0.0);
        }
    }
}
