//! Univ Server's fixture of objects, `crates/univ-api/src/testkit_objects.sql`, against what
//! ingest writes now: release 2 of the fixture, built and published in memory as `cargo xtask
//! integration` publishes it (releases 1 and 2), its tables dumped as `sqlite3`'s `.mode insert`
//! dumps them. With `UPDATE_FIXTURES=1` set, the test rewrites the file instead.
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod memory_store;
mod release_fixture;

use std::fmt::Write as _;

use common::workspace;
use memory_store::{Clock, MemoryStore, publishing};
use release_fixture::{build_next, fixture};
use rusqlite::Connection;
use rusqlite::types::ValueRef;
use univ_model::release::shards::SHARD_TABLES;

/// The variable that asks the test to rewrite the file.
const UPDATE: &str = "UPDATE_FIXTURES";

/// The file, relative to the workspace.
const FILE: &str = "crates/univ-api/src/testkit_objects.sql";

/// The release the file holds.
const RELEASE: u32 = 2;

/// The tables the file holds, in its order.
const TABLES: [&str; 8] = [
    "sources",
    "terms",
    "chains",
    "objects",
    "entries",
    "measurements",
    "designations",
    "names",
];

const HEADER: &str = "\
-- Release 2 of Univ Ingest's fixture: its sources and every object with its entries,
-- measurements, designations and names, and the vocabulary they are coded in, as
-- cargo xtask integration publishes them. Checked against what ingest writes by
-- univ-ingest's test api_fixture; UPDATE_FIXTURES=1 cargo test -p univ-ingest --test
-- api_fixture rewrites it.
";

/// A value as `sqlite3`'s `.mode insert` writes it: a whole real as `n.0`, any other real by
/// `%!.20g`, text quoted with its quotes doubled.
fn literal(db: &Connection, value: ValueRef<'_>) -> String {
    match value {
        ValueRef::Null => "NULL".to_owned(),
        ValueRef::Integer(n) => n.to_string(),
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a whole real within the range of i64, as the shell tests it"
        )]
        ValueRef::Real(x) if x.fract() == 0.0 && x.abs() < 9.2e18 => format!("{}.0", x as i64),
        ValueRef::Real(x) => db
            .query_row("SELECT printf('%!.20g', ?1)", [x], |r| r.get(0))
            .unwrap(),
        ValueRef::Text(t) => format!("'{}'", String::from_utf8_lossy(t).replace('\'', "''")),
        ValueRef::Blob(_) => unreachable!("a release holds no blob"),
    }
}

/// A key value, ordered as `SQLite` orders a key's integers and texts.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum KeyValue {
    Integer(i64),
    Text(String),
}

/// Release [`RELEASE`]'s rows of `table` in each of `dbs` as `INSERT INTO` statements, one a
/// line, in the order of the table's primary key whichever database holds them.
fn dump<'a>(dbs: impl Iterator<Item = &'a Connection>, table: &str, out: &mut String) {
    let key = univ_model::release::TABLES
        .iter()
        .find(|t| t.name == table)
        .unwrap()
        .key;
    let mut lines = Vec::new();
    for db in dbs {
        let mut q = db
            .prepare(&format!("SELECT * FROM {table} WHERE release = ?1"))
            .unwrap();
        let columns: Vec<String> = q.column_names().iter().map(|c| (*c).to_owned()).collect();
        let mut rows = q.query([RELEASE]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            let order: Vec<KeyValue> = key
                .iter()
                .map(|k| {
                    let i = columns.iter().position(|c| c == k).unwrap();
                    match row.get_ref(i).unwrap() {
                        ValueRef::Integer(n) => KeyValue::Integer(n),
                        ValueRef::Text(t) => KeyValue::Text(String::from_utf8_lossy(t).into()),
                        other => unreachable!("a key of {table} holds {other:?}"),
                    }
                })
                .collect();
            let values: Vec<String> = (0..columns.len())
                .map(|i| literal(db, row.get_ref(i).unwrap()))
                .collect();
            lines.push((
                order,
                format!("INSERT INTO {table} VALUES({});", values.join(",")),
            ));
        }
    }
    lines.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    for (_, line) in lines {
        writeln!(out, "{line}").unwrap();
    }
}

/// The file's text for the release published in `store`: each table's rows from the database
/// that holds them, a shard table's from each object shard in turn, so that the file is one
/// database of the release.
fn text(store: &MemoryStore) -> String {
    let db = &store.db;
    let rule: String = db
        .query_row(
            "SELECT ranking_rule_name FROM releases WHERE number = ?1",
            [RELEASE],
            |r| r.get(0),
        )
        .unwrap();
    let mut out = HEADER.to_owned();
    writeln!(
        out,
        "UPDATE releases SET ranking_rule_name = (SELECT '{}') WHERE number = {RELEASE};",
        rule.replace('\'', "''")
    )
    .unwrap();
    for table in TABLES {
        if SHARD_TABLES.contains(&table) {
            dump(store.shards.values(), table, &mut out);
        } else {
            dump(std::iter::once(db), table, &mut out);
        }
    }
    out
}

#[test]
fn univ_servers_fixture_of_objects_is_release_2_as_ingest_writes_it() {
    let f = fixture();
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let mut store = MemoryStore::default();
    let (out, state) = (f.out.path(), f.state.path());
    for number in 1..=RELEASE {
        build_next(&f);
        publishing(&mut store, number, out, state, journals.path(), &clock).unwrap();
    }
    let written = text(&store);
    let path = workspace().join(FILE);
    if std::env::var_os(UPDATE).is_some() {
        std::fs::write(&path, &written).unwrap();
        return;
    }
    let file = std::fs::read_to_string(&path).unwrap();
    let first_difference = file
        .lines()
        .zip(written.lines())
        .position(|(a, b)| a != b)
        .unwrap_or_else(|| file.lines().count().min(written.lines().count()));
    assert!(
        file == written,
        "{FILE} differs from what ingest writes from line {}: run {UPDATE}=1 cargo test -p \
         univ-ingest --test api_fixture to rewrite it\n  file: {:?}\n  ours: {:?}",
        first_difference + 1,
        file.lines().nth(first_difference),
        written.lines().nth(first_difference),
    );
}
