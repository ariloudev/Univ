//! Releases that share D1 and R2 (schema 6): each row is valid from the release that adds it
//! until one retires it, and each file is kept once by its SHA-256. A release published after
//! another leaves every read of the earlier one as it was (AC-128), and reads of it see its own
//! rows alone.

#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod memory_store;
mod release_fixture;

use std::collections::BTreeMap;

use memory_store::{Clock, MemoryStore, publishing};
use release_fixture::{build_next, fixture};
use rusqlite::Connection;
use univ_model::release::{Scope, TABLES, valid_in};

/// Every row of every table of [`Scope::PerRelease`] valid in release `n` in `db`, without its
/// `release` and `retired`, as text, by table.
fn valid_rows(db: &Connection, n: u32) -> BTreeMap<&'static str, Vec<String>> {
    let mut out = BTreeMap::new();
    for table in TABLES.iter().filter(|t| t.scope == Scope::PerRelease) {
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

/// What every database of `store` holds valid in release `n`, the catalogue's first.
fn everything_valid(store: &MemoryStore, n: u32) -> Vec<BTreeMap<&'static str, Vec<String>>> {
    std::iter::once(&store.db)
        .chain(store.shards.values())
        .map(|db| valid_rows(db, n))
        .collect()
}

#[test]
fn a_later_release_leaves_every_read_of_an_earlier_one_as_it_was() {
    let f = fixture();
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let mut store = MemoryStore::default();
    let (out, state) = (f.out.path(), f.state.path());
    let first = build_next(&f);
    publishing(&mut store, 1, out, state, journals.path(), &clock).unwrap();
    let before = everything_valid(&store, 1);

    let second = build_next(&f);
    publishing(&mut store, 2, out, state, journals.path(), &clock).unwrap();
    // AC-128: a read pinned to release 1 returns what it returned before release 2.
    assert_eq!(everything_valid(&store, 1), before);

    // Release 2 sees each of its rows once: as many as it wrote, no earlier copy beside them.
    let objects = store.int_everywhere(&format!(
        "SELECT COUNT(*) FROM objects WHERE {}",
        valid_in("", 2)
    ));
    assert_eq!(objects.unsigned_abs(), second.manifest.counts.objects);
    for table in TABLES.iter().filter(|t| t.scope == Scope::PerRelease) {
        let earlier = store.int_everywhere(&format!(
            "SELECT COUNT(*) FROM {} WHERE release = 1 AND {}",
            table.name,
            valid_in("", 2)
        ));
        assert_eq!(
            earlier, 0,
            "{} keeps a row of release 1 valid in 2",
            table.name
        );
    }
    assert_eq!(
        first.manifest.counts.objects,
        second.manifest.counts.objects
    );
}

#[test]
fn a_file_two_releases_hold_is_kept_once_under_its_sha256() {
    let f = fixture();
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let mut store = MemoryStore::default();
    let (out, state) = (f.out.path(), f.state.path());
    let first = build_next(&f);
    publishing(&mut store, 1, out, state, journals.path(), &clock).unwrap();
    let after_first = store.r2.len();
    let second = build_next(&f);
    publishing(&mut store, 2, out, state, journals.path(), &clock).unwrap();
    let r2 = |m: &univ_model::release::ReleaseManifest| -> std::collections::BTreeSet<String> {
        m.files.iter().filter_map(|f| f.key.clone()).collect()
    };
    let (a, b) = (r2(&first.manifest), r2(&second.manifest));
    let shared = a.intersection(&b).count();
    assert!(shared > 0, "the fixture's releases share their tiles");
    // R2 grew by release 2's manifest and the files release 1 did not hold, nothing more.
    assert_eq!(store.r2.len(), after_first + 1 + b.difference(&a).count());
    // Each release maps each of its paths to its file's SHA-256.
    for (n, m) in [(1, &first.manifest), (2, &second.manifest)] {
        let mapped = store.int(&format!(
            "SELECT COUNT(*) FROM release_files WHERE {}",
            valid_in("", n)
        ));
        let files = m.files.iter().filter(|f| f.key.is_some()).count();
        assert_eq!(mapped.unsigned_abs(), files as u64, "release {n}");
    }
}
