//! Rows that are not entries: a blank row or a repeated identifier is counted under its reason,
//! and a normalization the owner interrupts stops and says so.

mod common;
mod simulated;

use std::sync::atomic::AtomicBool;

use common::{definition_of, fixture_of, normalize_body};
use univ_ingest::ids::{EntryIds, assign};
use univ_ingest::matching::match_entries;
use univ_ingest::normalize::{NormalizeError, normalize_until};
use univ_ingest::source::LeftOutReason;
use univ_model::ledger::Ledger;
use univ_model::redshift::{CmbCorrection, DipoleModel};

#[test]
fn a_row_of_blank_cells_is_left_out_as_blank_but_a_final_empty_line_is_no_row() {
    let definition = definition_of("cosmicflows-4");
    let text = std::fs::read_to_string(fixture_of(&definition)).unwrap();
    assert!(
        text.ends_with("\n\n"),
        "the recording ends with an empty line"
    );
    // An empty line and a line of empty cells between the first two rows.
    let blank = "\t".repeat(10);
    let body = text.replacen("\n359.980500", &format!("\n\n{blank}\n359.980500"), 1);
    let n = normalize_body(&definition, body.as_bytes());
    assert_eq!(n.report.published, 5);
    assert_eq!(n.report.left_out.get(&LeftOutReason::BlankRow), Some(&2));
    assert_eq!(n.report.left_out_total(), 2);

    let n = normalize_body(&definition, text.as_bytes());
    assert_eq!((n.report.published, n.report.left_out_total()), (5, 0));
}

#[test]
fn a_normalization_whose_cancel_flag_is_set_stops_as_cancelled() {
    let definition = definition_of("cosmicflows-4");
    let files = [fixture_of(&definition)];
    let correction = CmbCorrection::new(DipoleModel::Planck2018);
    let go = AtomicBool::new(false);
    let done = normalize_until(&definition, &files, &correction, &go).unwrap();
    assert_eq!(done.report.published, 5);
    let stop = AtomicBool::new(true);
    let stopped = normalize_until(&definition, &files, &correction, &stop);
    assert_eq!(
        stopped,
        Err(NormalizeError::Cancelled {
            source_id: definition.id.clone()
        })
    );
}

#[test]
fn a_row_repeating_an_identifier_is_left_out_and_counted_and_the_match_goes_on() {
    // AC-50: the first data row of the recorded Cosmicflows-4 answer, given twice.
    let definition = definition_of("cosmicflows-4");
    let text = std::fs::read_to_string(fixture_of(&definition)).unwrap();
    let start = text.find("\n359.980500").unwrap() + 1;
    let end = start + text[start..].find('\n').unwrap() + 1;
    let row = &text[start..end];
    let body = format!("{}{row}{}", &text[..end], &text[end..]);
    let n = normalize_body(&definition, body.as_bytes());
    assert_eq!(n.report.published, 5);
    assert_eq!(
        n.report.left_out.get(&LeftOutReason::DuplicateIdentifier),
        Some(&1)
    );
    let mut ids = n
        .entries
        .iter()
        .map(|e| e.designation.clone())
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 5);
    // The entries match and take ids: no designation names two entries.
    let matched = match_entries(&simulated::matching_rules(), n.entries);
    let groups = matched.objects.iter().map(|o| {
        o.entries
            .iter()
            .map(|e| e.entry.designation.clone())
            .collect()
    });
    assert!(assign(&EntryIds::new(), &mut Ledger::new(), "1", groups).is_ok());
}
