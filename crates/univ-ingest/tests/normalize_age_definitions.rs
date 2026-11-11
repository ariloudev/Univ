//! A definition reads a star cluster's age only as its `normalization.ages` names it: in years
//! or log years, with its bounds in the same unit and a credible interval's probability in
//! (0, 1]; Hunt & Reffert 2023's definition reads its ages so.
mod common;

use std::path::Path;

use common::{VALID_DEFINITION as VALID, read_definition as read};
use univ_ingest::source::{ErrorColumns, SourceDefinition};
use univ_model::value::KindOfValue;

const COLUMNS: &str = "  - { column: z_helio, field: redshift, unit: dimensionless }\n";

/// The valid definition with three age columns in `unit` and the reading `ages`.
fn with_ages(unit: &str, ages: &str) -> String {
    VALID
        .replace(
            COLUMNS,
            &format!(
                "{COLUMNS}  - {{ column: logAge50, field: age, unit: {unit} }}\n  - {{ column: logAge16, field: age_low, unit: {unit} }}\n  - {{ column: logAge84, field: age_high, unit: {unit} }}\n"
            ),
        )
        .replace("normalization:\n", &format!("normalization:\n  ages:\n{ages}"))
}

fn reading(probability: &str, value: &str) -> String {
    format!(
        "    - value: {value}\n      error: {{ credible_interval: {{ low: logAge16, high: logAge84, probability: {probability} }} }}\n      kind_of_value: estimated\n      basis: \"the percentiles of the log age\"\n"
    )
}

#[test]
fn an_age_reading_in_log_years_with_its_credible_interval_is_read() {
    let d = read(&with_ages("log_year", &reading("0.68", "logAge50"))).unwrap();
    let a = &d.normalization.ages[0];
    assert_eq!(a.value, "logAge50");
    assert_eq!(a.kind_of_value, KindOfValue::Estimated);
    assert_eq!(
        a.error,
        Some(ErrorColumns::CredibleInterval {
            low: "logAge16".to_owned(),
            high: "logAge84".to_owned(),
            probability: 0.68
        })
    );
}

#[test]
fn an_age_reading_naming_a_column_not_taken_is_refused() {
    let refused = read(&with_ages("log_year", &reading("0.68", "logAge"))).unwrap_err();
    assert_eq!(refused.field, "normalization.ages[0]");
    assert!(refused.problem.contains("logAge"), "{}", refused.problem);
}

#[test]
fn an_age_reading_with_a_probability_outside_zero_to_one_is_refused() {
    for p in ["0", "1.5"] {
        let refused = read(&with_ages("log_year", &reading(p, "logAge50"))).unwrap_err();
        assert_eq!(refused.field, "normalization.ages[0]");
        assert!(
            refused.problem.contains("probability"),
            "{}",
            refused.problem
        );
    }
}

#[test]
fn an_age_read_in_a_unit_that_is_not_a_time_is_refused() {
    let refused = read(&with_ages("parsec", &reading("0.68", "logAge50"))).unwrap_err();
    assert_eq!(refused.field, "normalization.ages[0]");
    assert!(refused.problem.contains("years"), "{}", refused.problem);
}

#[test]
fn an_age_column_without_a_reading_is_refused() {
    let unread = with_ages("log_year", "").replace("  ages:\n", "");
    let refused = read(&unread).unwrap_err();
    assert_eq!(refused.field, "normalization");
    assert!(refused.problem.contains("logAge"), "{}", refused.problem);
}

#[test]
fn hunt_and_reffert_read_their_log_ages_as_estimated_with_a_68_percent_interval() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sources/hunt-reffert-2023.yaml");
    let d = SourceDefinition::read(&path).unwrap();
    let a = &d.normalization.ages[0];
    assert_eq!(
        (a.value.as_str(), a.kind_of_value),
        ("logAge50", KindOfValue::Estimated)
    );
    assert_eq!(
        a.error,
        Some(ErrorColumns::CredibleInterval {
            low: "logAge16".to_owned(),
            high: "logAge84".to_owned(),
            probability: 0.68
        })
    );
}
