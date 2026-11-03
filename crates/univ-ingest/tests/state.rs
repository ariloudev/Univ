//! The state between releases read back whole, or refused naming what is missing: the id ledger
//! and the last release's entry ids go together.
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

use univ_ingest::ids::{EntryIds, assign, write_ledger};
use univ_ingest::state::{ENTRY_IDS, LEDGER, State};
use univ_model::ids::{Designation, SourceId};
use univ_model::ledger::Ledger;

fn entry(key: &str) -> Designation {
    Designation::new(SourceId::new("test-catalogue").unwrap(), key).unwrap()
}

/// A ledger after release 1, which issued ids 1 and 2 to the entries A and B, and its entry ids.
fn first_release() -> (Ledger, EntryIds) {
    let mut ledger = Ledger::new();
    let groups = vec![vec![entry("A")], vec![entry("B")]];
    let assigned = assign(&EntryIds::new(), &mut ledger, "1", groups).unwrap();
    (ledger, assigned.entries)
}

#[test]
fn a_ledger_with_releases_and_no_entry_ids_is_refused_by_name() {
    let (ledger, _) = first_release();
    let dir = tempfile::tempdir().unwrap();
    write_ledger(&dir.path().join(LEDGER), &ledger).unwrap();
    let error = State::read(dir.path()).unwrap_err();
    assert!(error.file.ends_with(ENTRY_IDS), "{error}");
    let text = error.to_string();
    assert!(
        text.contains("is missing") && text.contains("\"1\""),
        "{text}"
    );
}

#[test]
fn entry_ids_that_leave_out_a_live_id_are_refused_naming_it() {
    let (ledger, mut entries) = first_release();
    entries.remove(&entry("B"));
    let dir = tempfile::tempdir().unwrap();
    let state = State {
        ledger: ledger.clone(),
        previous: entries,
    };
    state.write(dir.path()).unwrap();
    let error = State::read(dir.path()).unwrap_err();
    assert_eq!(error.field, "entries");
    assert!(
        error.problem.contains("1 id(s)") && error.problem.ends_with(": 2"),
        "{error}"
    );
    // Whole, the state reads back as written.
    let (ledger, entries) = first_release();
    let whole = State {
        ledger,
        previous: entries,
    };
    whole.write(dir.path()).unwrap();
    assert_eq!(State::read(dir.path()).unwrap(), whole);
    // Before the first release neither file exists, and nothing is missing.
    let empty = tempfile::tempdir().unwrap();
    assert_eq!(State::read(empty.path()).unwrap(), State::default());
}
