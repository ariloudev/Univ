//! An open cluster keeps the age its source publishes: Hunt & Reffert 2023's 50th percentile of
//! the logarithm of its age, kept as published beside the age in years, with the 16th and 84th
//! percentiles as a 68% credible interval, estimated (normalize-records, the Requirement on the
//! ages of star clusters).

mod common;

use common::{definition_of, entry, fixture_of, normalize_body, normalized_fixture};
use univ_model::units::Unit;
use univ_model::value::{KindOfValue, Uncertainty};

#[test]
fn an_open_cluster_keeps_its_log_age_as_published_and_its_age_in_years_as_an_estimate() {
    // Recorded 2026-11-10: ASCC_5, logAge50 7.32589102, logAge16 6.96985722, logAge84
    // 7.54225826.
    let n = normalized_fixture("hunt-reffert-2023");
    let e = entry(&n, "ASCC_5");
    assert_eq!(e.ages.len(), 1);
    let v = &e.ages[0].value;
    assert_eq!(v.kind, KindOfValue::Estimated);
    assert_eq!(
        (v.native.value, v.native.unit),
        (7.325_891_02, Unit::LogYear)
    );
    let Uncertainty::CredibleInterval {
        minus,
        plus,
        probability,
    } = v.native.uncertainty
    else {
        panic!("{:?}", v.native.uncertainty)
    };
    assert!((v.native.value - minus - 6.969_857_22).abs() < 1e-9);
    assert!((v.native.value + plus - 7.542_258_26).abs() < 1e-9);
    assert!((probability - 0.68).abs() < 1e-12);

    // About 21.2 Myr, between 9.3 and 34.8 Myr.
    let years = v.normalized;
    assert_eq!(years.unit, Unit::Year);
    let expected = 10_f64.powf(7.325_891_02);
    assert!((years.value - expected).abs() / expected < 1e-12);
    let Uncertainty::CredibleInterval {
        minus,
        plus,
        probability,
    } = years.uncertainty
    else {
        panic!("{:?}", years.uncertainty)
    };
    assert!(((years.value - minus) - 10_f64.powf(6.969_857_22)).abs() < 1.0);
    assert!(((years.value + plus) - 10_f64.powf(7.542_258_26)).abs() < 1.0);
    assert!((probability - 0.68).abs() < 1e-12);
}

#[test]
fn every_recorded_open_cluster_has_one_age() {
    let n = normalized_fixture("hunt-reffert-2023");
    assert_eq!(n.report.published, 5);
    assert!(n.entries.iter().all(|e| e.ages.len() == 1));
}

#[test]
fn an_open_cluster_whose_age_is_blank_has_none() {
    let definition = definition_of("hunt-reffert-2023");
    let body = std::fs::read_to_string(fixture_of(&definition)).unwrap();
    let blank = body.replace("\t 7.32589102\t", "\t           \t");
    assert_ne!(blank, body);
    let n = normalize_body(&definition, blank.as_bytes());
    assert_eq!(n.report.published, 5);
    assert!(entry(&n, "ASCC_5").ages.is_empty());
    assert_eq!(entry(&n, "ASCC_6").ages.len(), 1);
}
