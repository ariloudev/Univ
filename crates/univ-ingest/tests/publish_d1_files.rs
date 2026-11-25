//! A file of a release's rows written straight into the local D1 database that `wrangler dev`
//! reads: the database's own file, one transaction, and an empty write-ahead log after
//! (publish-a-release-locally).

#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

use std::fmt::Write as _;
use std::path::Path;

use rusqlite::Connection;
use univ_ingest::publish::d1_files::{database_file, database_id, file_name, write};
use univ_model::release::shards::Database;

/// The repository's Wrangler configuration, which names the local D1 databases.
fn repository_toml() -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../wrangler.toml"))
        .unwrap()
}

#[test]
fn the_database_file_written_is_the_one_wrangler_dev_opens_for_the_database_id() {
    // The names Miniflare gave the catalogue's and object shard 0's files in a local D1 that
    // `wrangler d1 execute --local` made, on 2026-11-25 with Wrangler 4.156.2.
    let toml = repository_toml();
    let catalogue = database_id(&toml, Database::Catalogue).unwrap();
    let shard = database_id(&toml, Database::Shard(0)).unwrap();
    assert_eq!(catalogue, "00000000-0000-0000-0000-000000000000");
    assert_eq!(shard, "00000000-0000-0000-0000-000000000100");
    assert_eq!(
        file_name(&catalogue),
        "9ba2b04bf514d9facfd57ed57d849e77241a7adc99d1c1545d06688b43d84248.sqlite"
    );
    assert_eq!(
        file_name(&shard),
        "c3b98b90e803bd37626615fd1d4dbab3d9d5f8e54835c3697760aa3b6c077b5a.sqlite"
    );
    assert_eq!(
        database_file(Path::new("/state"), &shard),
        Path::new("/state/v3/d1/miniflare-D1DatabaseObject")
            .join("c3b98b90e803bd37626615fd1d4dbab3d9d5f8e54835c3697760aa3b6c077b5a.sqlite")
    );
}

/// A database in write-ahead log mode, as Miniflare keeps one, with a table of rows.
fn database(dir: &Path) -> std::path::PathBuf {
    let db = dir.join("d1.sqlite");
    let conn = Connection::open(&db).unwrap();
    conn.execute_batch(
        "PRAGMA journal_mode = WAL; CREATE TABLE rows (n INTEGER PRIMARY KEY, name TEXT);",
    )
    .unwrap();
    db
}

fn rows(db: &Path) -> Vec<(i64, String)> {
    let conn = Connection::open(db).unwrap();
    let mut q = conn.prepare("SELECT n, name FROM rows ORDER BY n").unwrap();
    q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn wal_bytes(db: &Path) -> u64 {
    let wal = db.with_extension("sqlite-wal");
    std::fs::metadata(wal).map_or(0, |m| m.len())
}

#[test]
fn once_a_file_is_written_its_rows_are_in_the_database_and_its_log_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    let db = database(dir.path());
    let sql = dir.path().join("0001-rows-000.sql");
    let inserts = (1..=500).fold(String::new(), |mut out, n| {
        writeln!(
            out,
            "INSERT OR IGNORE INTO rows (n, name) VALUES ({n}, 'row {n}');"
        )
        .unwrap();
        out
    });
    std::fs::write(&sql, inserts).unwrap();
    write(&db, &sql).unwrap();
    let written = rows(&db);
    assert_eq!(written.len(), 500);
    assert_eq!(written[0], (1, "row 1".to_owned()));
    assert_eq!(wal_bytes(&db), 0);
}

#[test]
fn a_file_that_fails_part_way_leaves_none_of_its_rows() {
    let dir = tempfile::tempdir().unwrap();
    let db = database(dir.path());
    let sql = dir.path().join("0001-rows-000.sql");
    std::fs::write(
        &sql,
        "INSERT INTO rows (n, name) VALUES (1, 'one');\n\
         INSERT INTO rows (n, name) VALUES (2, 'two');\n\
         INSERT INTO no_such_table (n) VALUES (3);\n",
    )
    .unwrap();
    let error = write(&db, &sql).unwrap_err();
    assert!(error.contains("no_such_table"), "{error}");
    assert!(error.contains("0001-rows-000.sql"), "{error}");
    assert_eq!(rows(&db), [] as [(i64, std::string::String); 0]);
}
