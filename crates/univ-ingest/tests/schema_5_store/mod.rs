//! A store of schema 5 made from releases published under the current schema, as schema 5 held
//! them, and the reads that compare two stores, for the tests of the migrations.

#![allow(
    dead_code,
    reason = "each test binary that declares this module uses a part of it"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use rusqlite::Connection;
use rusqlite::types::Value;
use univ_ingest::release::release_dir;
use univ_ingest::release::schema8::migrate_databases;
use univ_model::release::manifest::Target;
use univ_model::release::{Scope, TABLES, keys, valid_in};

use crate::memory_store::{self, MemoryStore};
use crate::release_fixture::read_manifest;

/// The schema of version 5, as univ-model wrote it before rows were shared.
pub const SCHEMA_5: &str = include_str!("../fixtures/schema-5.sql");

/// The rows `sql` gives.
pub fn rows(db: &Connection, sql: &str) -> Vec<Vec<Value>> {
    let mut q = db.prepare(sql).unwrap();
    let n = q.column_count();
    q.query_map([], |r| (0..n).map(|i| r.get::<_, Value>(i)).collect())
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

/// Whether `db` holds a table `name`.
pub fn holds(db: &Connection, name: &str) -> bool {
    memory_store::int(
        db,
        &format!("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = '{name}'"),
    ) > 0
}

/// Every row `filter` selects of every table of [`Scope::PerRelease`] in each database of
/// `store`, the catalogue's first, without `release` and `retired`, as text, sorted by table:
/// the tables schema 5 has not left out ([`file_map`] compares `release_files`).
pub fn reads(store: &MemoryStore, filter: &str) -> Vec<BTreeMap<&'static str, Vec<String>>> {
    reads_but(store, filter, |_| false)
}

/// The columns schema 5 declares for table `name`, but `retired`, which it never had.
pub fn schema_5_columns(name: &str) -> Vec<String> {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch(SCHEMA_5).unwrap();
    rows(
        &db,
        &format!("SELECT name FROM pragma_table_info('{name}')"),
    )
    .into_iter()
    .filter_map(|r| match &r[0] {
        Value::Text(t) => Some(t.clone()),
        _ => None,
    })
    .collect()
}

/// The columns of `table` that schema 5 declares too: its JSON columns became rows of tables
/// of their own in schema 8, and the columns that replace them are not in schema 5.
pub fn in_both(table: &univ_model::release::Table) -> Vec<&'static str> {
    let old = schema_5_columns(table.name);
    table
        .columns
        .iter()
        .copied()
        .filter(|c| old.iter().any(|o| o == c))
        .collect()
}

/// Brings `store` from the JSON schema 7 kept at scale to the rows of schema 8: what a
/// `univ-ingest migrate` does on the local D1's files after the step to shared rows.
pub fn schema8(store: &MemoryStore) -> bool {
    let shards: Vec<&Connection> = store.shards.values().collect();
    migrate_databases(&store.db, &shards, &mut |_| {}).unwrap()
}

/// Whether schema 5 declares table `name`, which a store of schema 5 can hold.
pub fn in_schema_5(name: &str) -> bool {
    SCHEMA_5.contains(&format!("CREATE TABLE IF NOT EXISTS {name} ("))
}

/// [`reads`], leaving out the columns `out` names.
pub fn reads_but(
    store: &MemoryStore,
    filter: &str,
    out: fn(&str) -> bool,
) -> Vec<BTreeMap<&'static str, Vec<String>>> {
    std::iter::once(&store.db)
        .chain(store.shards.values())
        .map(|db| {
            TABLES
                .iter()
                .filter(|t| {
                    t.scope == Scope::PerRelease && in_schema_5(t.name) && holds(db, t.name)
                })
                .map(|t| {
                    let columns: Vec<&str> = in_both(t)
                        .into_iter()
                        .filter(|c| *c != "release" && !out(c))
                        .collect();
                    let sql = format!(
                        "SELECT {} FROM {} WHERE {filter}",
                        columns.join(", "),
                        t.name
                    );
                    let mut text: Vec<String> =
                        rows(db, &sql).iter().map(|r| format!("{r:?}")).collect();
                    text.sort();
                    (t.name, text)
                })
                .filter(|(_, r)| !r.is_empty())
                .collect()
        })
        .filter(|tables: &BTreeMap<_, _>| !tables.is_empty())
        .collect()
}

/// Copies `rows` into `table` of `to`, its `columns`.
pub fn insert(to: &Connection, table: &str, columns: &[&str], rows: &[Vec<Value>]) {
    let marks = vec!["?"; columns.len()].join(", ");
    let sql = format!(
        "INSERT INTO {table} ({}) VALUES ({marks})",
        columns.join(", ")
    );
    for row in rows {
        to.execute(&sql, rusqlite::params_from_iter(row)).unwrap();
    }
}

/// The store `a`'s `releases`, published under schema 6 from the build output `out`, as
/// schema 5 held them: each release's rows its own, its files under its prefix.
pub fn as_schema_5(a: &MemoryStore, out: &Path, releases: &[u32]) -> MemoryStore {
    // Each release's files by the key schema 6 gives them, to the key schema 5 gave them.
    let old_keys: BTreeMap<u32, BTreeMap<String, String>> = releases
        .iter()
        .map(|r| {
            let m = read_manifest(&release_dir(out, *r));
            let map = m
                .files
                .iter()
                .filter(|f| f.target == Target::R2)
                .filter_map(|f| Some((f.key.clone()?, keys::key(*r, &f.path))))
                .collect();
            (*r, map)
        })
        .collect();
    let mut b = MemoryStore::default();
    let mut pairs = vec![(&a.db, None)];
    pairs.extend(a.shards.iter().map(|(i, db)| (db, Some(*i))));
    for (from, shard) in pairs {
        let to = match shard {
            None => &b.db,
            Some(i) => b
                .shards
                .entry(i)
                .or_insert_with(|| Connection::open_in_memory().unwrap()),
        };
        to.execute_batch(SCHEMA_5).unwrap();
        to.execute("INSERT INTO schema_version (version) VALUES (5)", [])
            .unwrap();
        for t in TABLES
            .iter()
            .filter(|t| holds(from, t.name) && holds(to, t.name))
        {
            let both = in_both(t);
            let columns = both.join(", ");
            if t.scope == Scope::PerRelease {
                for r in releases {
                    let mut each = rows(
                        from,
                        &format!("SELECT {columns} FROM {} WHERE {}", t.name, valid_in("", r)),
                    );
                    let at = both.iter().position(|c| *c == "release").unwrap();
                    let named: Vec<usize> = (0..both.len())
                        .filter(|i| both[*i].ends_with("_key"))
                        .collect();
                    for row in &mut each {
                        row[at] = Value::Integer(i64::from(*r));
                        for i in &named {
                            if let Value::Text(key) = &row[*i]
                                && let Some(old) = old_keys[r].get(key)
                            {
                                row[*i] = Value::Text(old.clone());
                            }
                        }
                    }
                    insert(to, t.name, &both, &each);
                }
            } else {
                insert(
                    to,
                    t.name,
                    &both,
                    &rows(from, &format!("SELECT {columns} FROM {}", t.name)),
                );
            }
        }
        // When each release was published and pruned, which its row does not carry.
        if holds(from, "releases") {
            for r in rows(from, "SELECT number, published_at, pruned_at FROM releases") {
                to.execute(
                    "UPDATE releases SET published_at = ?2, pruned_at = ?3 WHERE number = ?1",
                    rusqlite::params_from_iter(r),
                )
                .unwrap();
            }
        }
        if holds(from, "latest") {
            insert(
                to,
                "latest",
                &["one", "release", "moved_at"],
                &rows(from, "SELECT one, release, moved_at FROM latest"),
            );
        }
    }
    for r in releases {
        let mut m = read_manifest(&release_dir(out, *r));
        for f in m.files.iter_mut().filter(|f| f.target == Target::R2) {
            let old = keys::key(*r, &f.path);
            b.r2.insert(old.clone(), a.r2[f.key.as_ref().unwrap()].clone());
            f.key = Some(old);
        }
        m.parent = None;
        b.r2.insert(
            keys::manifest(*r),
            serde_norway::to_string(&m).unwrap().into_bytes(),
        );
    }
    b
}

/// What `store` maps each path of release `n` to.
pub fn file_map(store: &MemoryStore, n: u32) -> BTreeSet<String> {
    let sql = format!(
        "SELECT path, sha256 FROM release_files WHERE {}",
        valid_in("", n)
    );
    rows(&store.db, &sql)
        .iter()
        .map(|r| format!("{r:?}"))
        .collect()
}
