//! The fingerprint publishing checks a table's rows by, against an in-memory D1: a NULL put in
//! place of a 0, which no total sees, and a lookup table's text renamed to another of the same
//! length, which no length sees, each stop the publish naming the table.

#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod memory_store;
mod release_fixture;

use memory_store::{Clock, MemoryStore, Op, publishing};
use release_fixture::{build_next, fixture};

/// The error of publishing release 1 when `sql` runs on D1 right after the file of `table` is
/// executed, and whether `latest` was left unset.
fn corrupted(table: &'static str, sql: &'static str) -> String {
    let f = fixture();
    build_next(&f);
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let part = format!("-{table}-");
    let mut changed = 0;
    let mut store = MemoryStore {
        observer: Some(Box::new(move |op, db, _| {
            if matches!(op, Op::Execute(name) if name.contains(&part)) {
                changed += db.execute(sql, []).unwrap();
                assert!(changed > 0, "{sql} changed no row");
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
    .unwrap_err()
    .to_string();
    assert!(
        store
            .db
            .query_row("SELECT release FROM latest", [], |r| r.get::<_, i64>(0))
            .is_err(),
        "latest is not moved"
    );
    error
}

#[test]
fn a_preferred_entry_or_chain_of_0_nulled_on_the_way_stops_the_publish() {
    for column in ["preferred_entry", "chain"] {
        let sql: &'static str = Box::leak(
            format!(
                "UPDATE objects SET {column} = NULL WHERE release = 1 AND id = (SELECT MIN(id) \
                 FROM objects WHERE release = 1 AND {column} = 0)"
            )
            .into_boxed_str(),
        );
        let error = corrupted("objects", sql);
        assert!(
            error.contains(&format!("values of {column} that are not NULL")),
            "{error}"
        );
    }
}

#[test]
fn a_scale_domain_nulled_on_the_way_stops_the_publish() {
    let error = corrupted(
        "objects",
        "UPDATE objects SET scale_domain = NULL WHERE release = 1 AND id = (SELECT MIN(id) FROM \
         objects WHERE release = 1 AND scale_domain IS NOT NULL)",
    );
    assert!(error.contains("rows of objects"), "{error}");
    assert!(error.contains("scale_domain"), "{error}");
}

#[test]
fn a_lookup_row_renamed_to_a_text_of_the_same_length_stops_the_publish() {
    for (table, sql) in [
        (
            "terms",
            "UPDATE terms SET term = substr(term, 1, length(term) - 1) || \
             char(unicode(substr(term, -1)) + 1) WHERE release = 1 AND id = 0",
        ),
        (
            "names",
            "UPDATE names SET name = upper(name) WHERE release = 1 AND name != upper(name) AND \
             key = (SELECT MIN(key) FROM names WHERE release = 1 AND name != upper(name))",
        ),
        (
            "sources",
            "UPDATE sources SET credit = replace(credit, 'a', 'b') WHERE release = 1 AND n = 0",
        ),
    ] {
        let error = corrupted(table, sql);
        assert!(
            error.contains(&format!("rows of {table} whose content differs")),
            "{table}: {error}"
        );
    }
}
