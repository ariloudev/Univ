//! What publishing checks beyond the uploads themselves, against an in-memory D1 and R2: each
//! table's rows reaching D1 as built (a text Wrangler would strip, a row altered on the way),
//! the refusals before any upload, and a missing publication time recorded by a rerun once
//! every unit is journaled. The CON-3 cap is in `publish_cap`.

#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod memory_store;
mod release_fixture;

use memory_store::{Clock, MemoryStore, Op, publishing};
use release_fixture::{build_next, fixture};
use univ_ingest::publish::journal::Journal;
use univ_ingest::publish::{Latest, PublishError};

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
        observer: Some(Box::new(|op, db, _, _| {
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

#[test]
fn a_release_needing_more_shards_than_are_bound_is_refused_before_any_upload() {
    let f = fixture();
    let built = build_next(&f);
    let needs = u32::try_from(built.manifest.shards.len()).unwrap();
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let mut store = MemoryStore {
        bound: needs - 1,
        ..MemoryStore::default()
    };
    let refused = publishing(
        &mut store,
        1,
        f.out.path(),
        f.state.path(),
        journals.path(),
        &clock,
    )
    .unwrap_err();
    assert_eq!(
        refused,
        PublishError::TooManyShards {
            number: 1,
            needs,
            binds: needs - 1
        }
    );
    let message = refused.to_string();
    assert!(
        message.contains(&format!(
            "it needs {needs} object shards and Univ Server binds {}",
            needs - 1
        )),
        "{message}"
    );
    assert_eq!(store.uploaded(), Vec::<String>::new());
    // Only the shards it binds were read, and none was written to.
    assert!(store.r2.is_empty());
    assert!(store.shards.keys().all(|&i| i < needs - 1));
    assert!(
        store
            .shards
            .values()
            .all(|db| memory_store::int(db, "SELECT COUNT(*) FROM sqlite_master") == 0)
    );
}

#[test]
fn a_database_of_an_older_schema_is_refused_before_anything_is_uploaded() {
    let f = fixture();
    build_next(&f);
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let mut store = MemoryStore::default();
    // A catalogue of schema 5, whose rows each belong to one release.
    store
        .db
        .execute_batch(
            "CREATE TABLE schema_version (version INTEGER PRIMARY KEY);
             INSERT INTO schema_version VALUES (5);",
        )
        .unwrap();
    let error = publishing(
        &mut store,
        1,
        f.out.path(),
        f.state.path(),
        journals.path(),
        &clock,
    )
    .unwrap_err();
    assert!(
        matches!(
            &error,
            PublishError::OlderSchema {
                holds: 5,
                needs: 6,
                ..
            }
        ),
        "{error}"
    );
    assert!(
        error.to_string().contains("nothing was uploaded"),
        "{error}"
    );
    assert!(store.r2.is_empty());
    assert!(!store.ops.iter().any(|o| matches!(o, Op::Execute(_))));
}

#[test]
fn a_release_built_on_a_parent_latest_does_not_point_to_is_refused() {
    let f = fixture();
    build_next(&f);
    build_next(&f);
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let mut store = MemoryStore::default();
    // Release 2 holds only its change from release 1, which was never published.
    let error = publishing(
        &mut store,
        2,
        f.out.path(),
        f.state.path(),
        journals.path(),
        &clock,
    )
    .unwrap_err();
    assert!(
        matches!(
            &error,
            PublishError::NotOnLatest {
                number: 2,
                parent: 1,
                latest: None
            }
        ),
        "{error}"
    );
    // The message names the release it was built on and where latest points.
    let said = error.to_string();
    assert!(
        said.contains("changed from release 1") && said.contains("latest points to no release"),
        "{said}"
    );
    assert!(store.r2.is_empty());
    assert!(!store.ops.iter().any(|o| matches!(o, Op::Execute(_))));
}

#[test]
fn a_table_whose_rows_a_release_all_keeps_is_checked_though_it_writes_no_file_of_it() {
    let mut f = fixture();
    let last = f.structures.pop().unwrap();
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let mut store = MemoryStore::default();
    build_next(&f);
    publishing(
        &mut store,
        1,
        f.out.path(),
        f.state.path(),
        journals.path(),
        &clock,
    )
    .unwrap();
    // Release 2 only adds a structure: it writes no file of the objects, whose rows it keeps.
    f.structures.push(last);
    let second = build_next(&f);
    assert!(
        !second
            .manifest
            .files
            .iter()
            .any(|file| file.path.contains("-objects-"))
    );
    // A row release 1 loaded is lost from D1 before release 2 is published.
    let shard = *store.shards.keys().next().unwrap();
    store
        .shards
        .get(&shard)
        .unwrap()
        .execute(
            "DELETE FROM objects WHERE id = (SELECT MIN(id) FROM objects)",
            [],
        )
        .unwrap();
    let error = publishing(
        &mut store,
        2,
        f.out.path(),
        f.state.path(),
        journals.path(),
        &clock,
    )
    .unwrap_err();
    assert!(error.to_string().contains("objects"), "{error}");
    assert_eq!(latest(&store), Some(1));
}
