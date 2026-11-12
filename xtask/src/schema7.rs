//! A local state whose databases still hold the JSON of D1 schema 7, which the Worker of
//! schema 8 does not read: `serve`, `explore` and the benches refuse it until
//! `univ-ingest migrate` has moved that JSON into rows.

use std::fs;
use std::path::Path;

use rusqlite::{Connection, OpenFlags};
use univ_model::release::{LOCAL_D1_FILES, SCHEMA_7_JSON_HELD};

/// The first JSON column of schema 7 that a database of the local state `state` still holds
/// ([`SCHEMA_7_JSON_HELD`]), named with its file; none when every database is of schema 8.
/// Each is opened read only, beside any Worker.
pub(crate) fn json_held(state: &Path) -> Option<String> {
    let dir = state.join(LOCAL_D1_FILES);
    let files = fs::read_dir(&dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path());
    for path in files.filter(|p| p.extension().is_some_and(|x| x == "sqlite")) {
        let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let Ok(db) = Connection::open_with_flags(&path, flags) else {
            continue;
        };
        if let Ok(column) = db.query_row(SCHEMA_7_JSON_HELD, [], |r| r.get::<_, String>(0)) {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            return Some(format!("{name} holds {column}"));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A local state whose one database was given `schema`.
    fn state_of(schema: &str) -> tempfile::TempDir {
        let state = tempfile::tempdir().unwrap();
        let dir = state.path().join(LOCAL_D1_FILES);
        fs::create_dir_all(&dir).unwrap();
        let db = Connection::open(dir.join("a1b2.sqlite")).unwrap();
        db.execute_batch(schema).unwrap();
        state
    }

    #[test]
    fn a_state_still_holding_the_json_of_schema_7_is_named_and_one_of_schema_8_is_not() {
        let old = state_of(include_str!(
            "../../crates/univ-ingest/tests/fixtures/schema-7.sql"
        ));
        assert_eq!(
            json_held(old.path()).as_deref(),
            Some("a1b2.sqlite holds objects.links")
        );
        let new = state_of(univ_model::release::SCHEMA);
        assert_eq!(json_held(new.path()), None);
    }
}
