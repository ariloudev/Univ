//! What a store holds valid in a release, read the same way by every test of shared rows.

#![allow(
    dead_code,
    reason = "each test binary that includes this module uses its own share of it"
)]

use std::collections::BTreeMap;

use rusqlite::Connection;
use univ_model::release::{Scope, TABLES, valid_in};

use crate::memory_store::MemoryStore;

/// Every row of every table of [`Scope::PerRelease`] valid in release `n` in `db`, without its
/// `release` and `retired`, as text, by table.
pub fn valid_rows(db: &Connection, n: u32) -> BTreeMap<&'static str, Vec<String>> {
    let mut out = BTreeMap::new();
    // A shard bound but never loaded holds no table.
    let held = |name: &str| {
        db.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name = ?1",
            [name],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
            > 0
    };
    for table in TABLES
        .iter()
        .filter(|t| t.scope == Scope::PerRelease && held(t.name))
    {
        let columns: Vec<&str> = table
            .columns
            .iter()
            .copied()
            .filter(|c| *c != "release")
            .collect();
        let sql = format!(
            "SELECT {} FROM {} WHERE {} ORDER BY 1, 2",
            columns.join(", "),
            table.name,
            valid_in("", n)
        );
        let mut stmt = db.prepare(&sql).unwrap();
        let mut rows: Vec<String> = stmt
            .query_map([], |r| {
                (0..columns.len())
                    .map(|i| {
                        r.get::<_, rusqlite::types::Value>(i)
                            .map(|v| format!("{v:?}"))
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map(|v| v.join("|"))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect();
        rows.sort();
        out.insert(table.name, rows);
    }
    out
}

/// What every database of `store` holds valid in release `n`, the catalogue's first: those
/// that hold nothing valid in it, such as an object shard a later release added, left out.
pub fn everything_valid(store: &MemoryStore, n: u32) -> Vec<BTreeMap<&'static str, Vec<String>>> {
    std::iter::once(&store.db)
        .chain(store.shards.values())
        .map(|db| valid_rows(db, n))
        .filter(|tables| tables.values().any(|rows| !rows.is_empty()))
        .collect()
}
