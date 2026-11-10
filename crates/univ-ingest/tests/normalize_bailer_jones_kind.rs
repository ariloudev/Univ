//! A Bailer-Jones 2021 distance resting on a parallax below the ranking rule's prior-led ratio,
//! five times its error, is inferred; at or above it, estimated (normalize-records, the
//! Requirement on Bailer-Jones distances).

mod common;

use std::path::PathBuf;

use common::{definition_of, entry, normalize_body, normalized_file, normalized_fixture};
use univ_ingest::ranking::RankingRules;
use univ_model::value::KindOfValue;
use univ_model::value::KindOfValue::{Estimated, Inferred};

const SOURCE: &str = "bailer-jones-2021";

fn kinds(n: &univ_ingest::normalize::Normalized, id: &str) -> Vec<KindOfValue> {
    entry(n, id)
        .distances
        .iter()
        .map(|d| d.kind_of_value)
        .collect()
}

#[test]
fn a_parallax_below_five_times_its_error_makes_both_distances_inferred() {
    // Recorded 2026-11-10 from the first G < 14 chunk: ϖ/σ = −1.009, 4.971 and 5.141.
    let n = normalized_file(SOURCE, "bailer-jones-2021-prior-led.csv");
    assert_eq!(n.report.published, 3);
    assert_eq!(kinds(&n, "264499655345620736"), [Inferred, Inferred]);
    // No photogeometric distance is published for this one, so it has the geometric alone.
    assert_eq!(kinds(&n, "403572036421829504"), [Inferred]);
    assert_eq!(kinds(&n, "5695782149587747968"), [Estimated, Estimated]);
    // The ratio is kept as published beside the distances.
    let s = entry(&n, "403572036421829504");
    assert_eq!(s.flags.get("parallax_over_error").unwrap(), "4.970551");
}

#[test]
fn the_bright_stars_ratios_far_above_five_leave_their_distances_estimated() {
    let n = normalized_fixture(SOURCE);
    assert_eq!(n.report.published, 5);
    for e in &n.entries {
        assert!(
            e.distances.iter().all(|d| d.kind_of_value == Estimated),
            "{}",
            e.designation.identifier
        );
    }
}

#[test]
fn exactly_five_is_estimated_and_a_blank_ratio_is_inferred() {
    // The recorded row of ϖ/σ = 5.141, its ratio set by hand to 5 and to blank: no Gaia source
    // sits at exactly 5, and the archive gives no Bailer-Jones distance without a parallax.
    let row = |ratio: &str| {
        format!(
            "source_id,r_med_geo,r_lo_geo,r_hi_geo,r_med_photogeo,r_lo_photogeo,r_hi_photogeo,\
             flag,parallax_over_error\n5695782149587747968,8992.514,8055.6226,10076.477,\
             10370.372,9438.839,11487.332,10033,{ratio}\n"
        )
    };
    let definition = definition_of(SOURCE);
    let at_five = normalize_body(&definition, row("5").as_bytes());
    assert_eq!(
        kinds(&at_five, "5695782149587747968"),
        [Estimated, Estimated]
    );
    let blank = normalize_body(&definition, row("").as_bytes());
    assert_eq!(kinds(&blank, "5695782149587747968"), [Inferred, Inferred]);
}

#[test]
fn the_definition_s_bound_is_the_ranking_rule_s_prior_led_ratio() {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../rules/ranking.yaml");
    let rules = RankingRules::read(&file).unwrap();
    let definition = definition_of(SOURCE);
    let bounds: Vec<f64> = definition
        .normalization
        .rules
        .iter()
        .filter_map(|r| r.when.below)
        .collect();
    assert_eq!(bounds, [rules.prior_led_parallax_ratio]);
}
