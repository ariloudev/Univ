//! Fixes that keep a row a published value would otherwise cost: an undocumented sentinel error
//! read as none reported (eRASS1's `BEST_ZERR` = -1).

mod common;

use common::{definition_of, entry, fixture_of, normalize_body};
use univ_ingest::source::{Fix, FixAction, LeftOutReason};
use univ_model::value::Uncertainty;

#[test]
fn erass1_reads_a_best_zerr_of_minus_one_as_no_error_reported() {
    let erass1 = definition_of("erass1-clusters");
    let sentinel: Vec<_> = erass1
        .fixes
        .iter()
        .filter_map(|f| match &f.action {
            Some(FixAction::SentinelErrorIsNoneReported { columns, value }) => {
                Some((columns.clone(), *value))
            }
            _ => None,
        })
        .collect();
    assert_eq!(sentinel, [(vec!["BEST_ZERR".to_owned()], -1.0)]);
}

/// The recorded 2MRS rows with row 2's `e_cz` set to `error`, read with the sentinel fix or
/// without it.
fn twomrs_with_error(error: &str, fix: bool) -> univ_ingest::normalize::Normalized {
    let mut definition = definition_of("twomrs");
    if fix {
        definition.fixes.push(Fix {
            defect: "a test defect".into(),
            change: "an e_cz of -1 is no error reported".into(),
            evidence: "a test".into(),
            action: Some(FixAction::SentinelErrorIsNoneReported {
                columns: vec!["e_cz".into()],
                value: -1.0,
            }),
        });
    }
    let text = std::fs::read_to_string(fixture_of(&definition)).unwrap_or_default();
    let mut row = 0;
    let lines: Vec<String> = text
        .lines()
        .map(|line| {
            let mut cells: Vec<&str> = line.split('\t').collect();
            let data = !line.starts_with('#')
                && cells.len() == 8
                && cells[3].trim().parse::<f64>().is_ok();
            if data {
                row += 1;
                if row == 2 {
                    cells[4] = error;
                }
            }
            cells.join("\t")
        })
        .collect();
    normalize_body(&definition, (lines.join("\n") + "\n").as_bytes())
}

#[test]
fn a_sentinel_error_is_none_reported_and_the_row_kept_where_a_fix_says_so() {
    // Row 2: 00473313-2517196 (NGC 253), cz 243 ± 2 km/s, its error made -1.
    let without = twomrs_with_error("-1", false);
    assert_eq!(
        without.report.left_out.get(&LeftOutReason::BadUncertainty),
        Some(&1)
    );
    let with = twomrs_with_error("-1", true);
    assert_eq!(with.report.left_out_total(), 0);
    let r = &entry(&with, "00473313-2517196").redshifts[0];
    assert_eq!(r.cmb.heliocentric.uncertainty, Uncertainty::NoneReported);
    assert_eq!(with.report.fixes.last().map(|f| f.entries_changed), Some(1));
    // Any other negative error still leaves the row out.
    let other = twomrs_with_error("-2", true);
    assert_eq!(
        other.report.left_out.get(&LeftOutReason::BadUncertainty),
        Some(&1)
    );
}
