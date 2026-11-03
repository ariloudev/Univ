//! What publishing checks beyond the uploads themselves, against an in-memory D1 and R2: each
//! table's rows reaching D1 as built (a text Wrangler would strip, a row altered on the way),
//! the CON-3 cap over every release not pruned, and a missing publication time recorded by a
//! rerun once every unit is journaled.

#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod memory_store;
mod release_fixture;

use memory_store::{Clock, MemoryStore, Op, publishing};
use release_fixture::{build_next, fixture, read_manifest};
use univ_ingest::publish::journal::Journal;
use univ_ingest::publish::{Latest, PublishError};
use univ_model::release::D1_CAP_BYTES;

/// What `latest` points to, by D1.
fn latest(store: &MemoryStore) -> Option<i64> {
    store
        .db
        .query_row("SELECT release FROM latest", [], |r| r.get(0))
        .ok()
}

#[test]
fn a_text_holding_begin_transaction_reaches_d1_as_it_is() {
    let mut f = fixture();
    let title = "BEGIN TRANSACTION; Hipparcos, the New Reduction; COMMIT; begin transaction";
    f.definitions[0].title = title.to_owned();
    build_next(&f);
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let mut store = MemoryStore::default();
    publishing(
        &mut store,
        1,
        f.out.path(),
        f.state.path(),
        journals.path(),
        &clock,
    )
    .unwrap();
    let stored: String = store
        .db
        .query_row(
            &format!(
                "SELECT title FROM sources WHERE release = 1 AND id = '{}'",
                f.definitions[0].id
            ),
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stored, title);
}

#[test]
fn a_row_altered_on_its_way_to_d1_stops_the_publish_naming_its_table_and_measure() {
    let f = fixture();
    build_next(&f);
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let mut store = MemoryStore {
        observer: Some(Box::new(|op, db, _| {
            if matches!(op, Op::Execute(name) if name.contains("-sources-")) {
                db.execute(
                    "UPDATE sources SET title = title || '!' WHERE release = 1 AND id = \
                     (SELECT MIN(id) FROM sources WHERE release = 1)",
                    [],
                )
                .unwrap();
            }
        })),
        ..MemoryStore::default()
    };
    let error = publishing(
        &mut store,
        1,
        f.out.path(),
        f.state.path(),
        journals.path(),
        &clock,
    )
    .unwrap_err();
    let message = error.to_string();
    assert!(
        message.contains("rows of sources with total bytes of title "),
        "{message}"
    );
    assert!(
        message.contains("a row was dropped or altered"),
        "{message}"
    );
    assert_eq!(latest(&store), None);
    let journal = Journal::read(journals.path(), 1).unwrap().unwrap();
    assert!(!journal.done.iter().any(|d| d.path.contains("-sources-")));
}

#[test]
fn a_release_beyond_the_cap_with_those_not_pruned_is_refused_with_both_estimates() {
    let f = fixture();
    build_next(&f);
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let mut store = MemoryStore::default();
    publishing(
        &mut store,
        1,
        f.out.path(),
        f.state.path(),
        journals.path(),
        &clock,
    )
    .unwrap();
    let recorded = store.int("SELECT d1_estimated_bytes FROM releases WHERE number = 1");
    let built = build_next(&f);
    assert_eq!(
        u64::try_from(recorded).unwrap(),
        read_manifest(&univ_ingest::release::release_dir(f.out.path(), 1))
            .d1
            .estimated_bytes
    );
    // Release 1 as if it took nearly all of the cap.
    let others = D1_CAP_BYTES - 1000;
    store
        .db
        .execute(
            &format!("UPDATE releases SET d1_estimated_bytes = {others} WHERE number = 1"),
            [],
        )
        .unwrap();
    store.reset();
    let refused = publishing(
        &mut store,
        2,
        f.out.path(),
        f.state.path(),
        journals.path(),
        &clock,
    )
    .unwrap_err();
    let this = built.manifest.d1.estimated_bytes;
    assert_eq!(
        refused,
        PublishError::OverCap {
            number: 2,
            this,
            others,
            releases: 1
        }
    );
    let message = refused.to_string();
    assert!(
        message.contains(&format!("estimated at {this} bytes"))
            && message.contains(&format!("1 release(s) not pruned at {others} bytes"))
            && message.contains("beyond the 10 GB cap of 10000000000 bytes"),
        "{message}"
    );
    assert_eq!(store.uploaded(), Vec::<String>::new());
    assert_eq!(latest(&store), Some(1));
}

#[test]
fn a_rerun_records_a_missing_publication_time_once_every_unit_is_journaled() {
    let f = fixture();
    build_next(&f);
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let mut store = MemoryStore::default();
    publishing(
        &mut store,
        1,
        f.out.path(),
        f.state.path(),
        journals.path(),
        &clock,
    )
    .unwrap();
    // A run stopped after latest moved, before it recorded the publication time; then a newer
    // release was published.
    store
        .db
        .execute(
            "UPDATE releases SET published_at = NULL WHERE number = 1",
            [],
        )
        .unwrap();
    build_next(&f);
    publishing(
        &mut store,
        2,
        f.out.path(),
        f.state.path(),
        journals.path(),
        &clock,
    )
    .unwrap();
    assert_eq!(latest(&store), Some(2));
    store.reset();

    let moved = Journal::read(journals.path(), 1)
        .unwrap()
        .unwrap()
        .latest_moved_at
        .unwrap();
    let again = publishing(
        &mut store,
        1,
        f.out.path(),
        f.state.path(),
        journals.path(),
        &clock,
    )
    .unwrap_err();
    assert_eq!(
        again,
        PublishError::AlreadyPublished {
            number: 1,
            published_at: moved.clone(),
            latest: Latest(Some(2)),
        }
    );
    let recorded: String = store
        .db
        .query_row(
            "SELECT published_at FROM releases WHERE number = 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(recorded, moved);
    assert_eq!(store.uploaded(), Vec::<String>::new());
    assert_eq!(latest(&store), Some(2));
}
