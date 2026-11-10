//! The preferred measurement where an uncertainty is missing or a parallax has a zero point, on
//! SIMULATED entries under the repository's `rules/ranking.yaml`.

mod common;
mod simulated;

use common::near;
use simulated::{entry, ranking_rules, with_bare_cz, with_distance, with_parallax};
use univ_ingest::crossmatch::place;
use univ_ingest::ranking::rank;
use univ_model::derive::{Release, Transformation};
use univ_model::distance::{DistanceKind, Method};
use univ_model::entry::CatalogueEntry;
use univ_model::object::{
    Decided, FractionalUncertainty, MeasurementRef, Object, ObjectEntry, Resolution,
};
use univ_model::units::Unit;
use univ_model::value::KindOfValue;

fn ranked(entries: Vec<CatalogueEntry>) -> Object {
    let mut object = Object::new(
        entries
            .into_iter()
            .map(|entry| ObjectEntry {
                entry,
                resolved_by: Resolution::Anchor.into(),
            })
            .collect::<Vec<_>>(),
    );
    rank(&mut object, &ranking_rules(), &Release::planck_2018());
    object
}

#[test]
fn a_redshift_without_an_error_ranks_by_its_peculiar_velocity_term() {
    // A 2MRS cz of 10,000 km/s published with a zero error, read as none reported, beside a
    // Tully-Fisher distance of 25%. The redshift distance still carries the rule's 300 km/s
    // peculiar-velocity term, about 3%, and wins by it; it has no measurement term.
    let tf = (DistanceKind::SecondaryIndicator, Method::TullyFisher);
    let object = ranked(vec![
        with_distance(
            entry("cosmicflows-4", "a", 78.0, 0.0),
            140.0,
            Some(35.0),
            tf,
        ),
        with_bare_cz(entry("twomrs", "g", 78.0, 0.0), 10_000.0),
    ]);
    let preferred = object.preferred_distance.unwrap();
    assert!(
        matches!(preferred.measurement, MeasurementRef::Redshift { .. }),
        "{preferred:?}"
    );
    assert_eq!(preferred.decided, Decided::Uncertainty);
    let FractionalUncertainty::Reported {
        total,
        measurement,
        peculiar_velocity,
    } = preferred.uncertainty
    else {
        panic!("{:?}", preferred.uncertainty)
    };
    assert_eq!(measurement, None, "no error of its own is invented");
    assert_eq!(Some(total), peculiar_velocity);
    assert!(total > 0.025 && total < 0.035, "{total}");
    let (runner_up, uncertainty) = *preferred.runner_up.unwrap();
    assert!(matches!(runner_up, MeasurementRef::Distance { .. }));
    assert!(near(uncertainty.total().unwrap(), 0.25, 1e-12));
}

#[test]
fn a_gaia_parallax_is_ranked_and_placed_after_its_zero_point_and_a_hipparcos_one_is_not() {
    // ϖ = 2.000 ± 0.050 mas in Gaia DR3 is 2.017 mas after the −0.017 mas zero point of
    // the rules; in Hipparcos-2 it stays 2.000.
    let release = Release::planck_2018();
    for (source, corrected) in [("gaia-dr3-bright", 2.017), ("hipparcos-2007", 2.0)] {
        let mut object = ranked(vec![with_parallax(
            entry(source, "1", 45.0, 30.0),
            2.0,
            0.05,
        )]);
        let preferred = object.preferred_distance.clone().unwrap();
        assert!(matches!(
            preferred.measurement,
            MeasurementRef::Parallax { .. }
        ));
        assert!(
            near(preferred.distance_mpc, 1e-3 / corrected, 1e-12),
            "{source}"
        );
        let total = preferred.uncertainty.total().unwrap();
        assert!(near(total, 0.05 / corrected, 1e-12), "{source}");
        let placed = place(&mut object, &release, &ranking_rules()).unwrap();
        // The derived position's native value is the parallax as published.
        assert_eq!(
            (placed.native().value, placed.native().unit),
            (2.0, Unit::Milliarcsecond)
        );
        assert!(near(placed.distance().value, 1e-3 / corrected, 1e-12));
        let zero_point = placed
            .chain()
            .iter()
            .any(|s| matches!(s.transformation, Transformation::ParallaxZeroPoint { .. }));
        assert_eq!(zero_point, source == "gaia-dr3-bright", "{source}");
        assert!(matches!(
            placed.chain()[0].transformation,
            Transformation::PublishedParallax { .. }
        ));
    }
}

#[test]
fn the_prior_led_parallax_ratio_is_the_rule_s_parameter_recorded_for_the_manifest() {
    let rules = ranking_rules();
    assert!((rules.prior_led_parallax_ratio - 5.0).abs() < 1e-12);
    let yaml = rules.record().unwrap();
    for text in ["prior_led_parallax_ratio: 5.0", "prior_led_parallax_basis:"] {
        assert!(yaml.contains(text), "{text}");
    }
    for refused in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let mut bad = rules.clone();
        bad.prior_led_parallax_ratio = refused;
        let (field, _) = bad.check().unwrap_err();
        assert_eq!(field, "prior_led_parallax_ratio", "{refused}");
    }
}

/// The entry with a SIMULATED Bailer-Jones-like geometric distance of `pc` ± `sigma` pc that
/// leans on its prior, inferred.
fn prior_led(pc: f64, sigma: f64) -> CatalogueEntry {
    let mut e = with_distance(
        entry("bailer-jones-2021", "bj", 84.0, -1.0),
        pc * 1e-6,
        Some(sigma * 1e-6),
        (DistanceKind::Geometric, Method::BayesianGeometric),
    );
    if let Some(d) = e.distances.last_mut() {
        d.kind_of_value = KindOfValue::Inferred;
    }
    e
}

#[test]
fn an_estimated_indicator_distance_wins_over_an_inferred_one_that_is_more_precise() {
    // A Cepheid-like period-luminosity distance of 3,500 pc at 8%, estimated, beside a
    // prior-led Bailer-Jones distance of 3,000 pc at 5%: by uncertainty the inferred one would
    // win; the rule sets it aside, and the kind of value decides.
    let mut cepheid = with_distance(
        entry("skowron-2025-cepheids", "c", 84.0, -1.0),
        3_500e-6,
        Some(280e-6),
        (DistanceKind::PrimaryIndicator, Method::Cepheids),
    );
    cepheid.distances.last_mut().unwrap().kind_of_value = KindOfValue::Estimated;
    let object = ranked(vec![prior_led(3_000.0, 150.0), cepheid]);
    let preferred = object.preferred_distance.unwrap();
    assert!(
        matches!(&preferred.measurement, MeasurementRef::Distance { entry, .. } if &*entry.identifier == "c"),
        "{preferred:?}"
    );
    assert_eq!(preferred.decided, Decided::KindOfValue);
}

#[test]
fn an_inferred_distance_alone_still_wins_and_places_its_star() {
    let mut object = ranked(vec![prior_led(3_000.0, 900.0)]);
    let preferred = object.preferred_distance.clone().unwrap();
    assert_eq!(preferred.decided, Decided::OnlyCandidate);
    let placed = place(&mut object, &Release::planck_2018(), &ranking_rules()).unwrap();
    assert!(near(placed.distance().value, 3_000.0e-6, 1e-12));
}

#[test]
fn two_inferred_distances_still_compete_by_uncertainty() {
    let object = ranked(vec![prior_led(3_000.0, 900.0), prior_led(2_800.0, 300.0)]);
    let preferred = object.preferred_distance.unwrap();
    assert!(
        near(preferred.distance_mpc, 2_800e-6, 1e-12),
        "{preferred:?}"
    );
    assert_eq!(preferred.decided, Decided::Uncertainty);
}

#[test]
fn an_estimated_distance_too_uncertain_to_place_does_not_set_an_inferred_one_aside() {
    // A prior-led Bailer-Jones distance at 30% beside an estimated indicator distance at 120%:
    // the estimated one could not place the star, so the inferred one still competes, wins by
    // uncertainty and places it.
    let mut loose = with_distance(
        entry("skowron-2025-cepheids", "c", 84.0, -1.0),
        3_500e-6,
        Some(4_200e-6),
        (DistanceKind::PrimaryIndicator, Method::Cepheids),
    );
    if let Some(d) = loose.distances.last_mut() {
        d.kind_of_value = KindOfValue::Estimated;
    }
    let mut object = ranked(vec![prior_led(3_000.0, 900.0), loose]);
    let preferred = object.preferred_distance.clone().unwrap();
    assert!(
        near(preferred.distance_mpc, 3_000e-6, 1e-12),
        "{preferred:?}"
    );
    assert_eq!(preferred.decided, Decided::Uncertainty);
    assert!(place(&mut object, &Release::planck_2018(), &ranking_rules()).is_some());
}
