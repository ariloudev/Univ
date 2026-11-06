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

/// What every database of `store` holds valid in release `n`, the catalogue's first: those
/// that hold nothing valid in it, such as an object shard a later release added, left out.
fn everything_valid(store: &MemoryStore, n: u32) -> Vec<BTreeMap<&'static str, Vec<String>>> {
    std::iter::once(&store.db)
        .chain(store.shards.values())
        .map(|db| valid_rows(db, n))
        .filter(|tables| tables.values().any(|rows| !rows.is_empty()))
        .collect()
}

/// How many rows of `table` valid in release `n` share their key without `release` with
/// another valid in `n`, in every database of `store`.
fn repeated_keys(store: &MemoryStore, table: &univ_model::release::Table, n: u32) -> i64 {
    let key: Vec<&str> = table
        .key
        .iter()
        .copied()
        .filter(|c| *c != "release")
        .collect();
    store.int_everywhere(&format!(
        "SELECT COUNT(*) FROM (SELECT 1 FROM {} WHERE {} GROUP BY {} HAVING COUNT(*) > 1)",
        table.name,
        valid_in("", n),
        key.join(", ")
    ))
}

#[test]
fn a_later_release_leaves_every_read_of_an_earlier_one_as_it_was() {
    const SOURCE: &str = "mcconnachie-2012";
    let mut f = fixture();
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let mut store = MemoryStore::default();
    let (out, state) = (f.out.path().to_owned(), f.state.path().to_owned());
    // Release 1 lacks a structure and a source; 2 adds the structure, 3 the source, and 4
    // drops it again.
    let structure = f.structures.pop().unwrap();
    let at = f
        .definitions
        .iter()
        .position(|d| d.id.as_str() == SOURCE)
        .unwrap();
    let definition = f.definitions.remove(at);
    let normalized = f
        .normalized
        .iter()
        .position(|n| n.source.as_str() == SOURCE)
        .map(|i| f.normalized.remove(i))
        .unwrap();
    let mut releases = Vec::new();
    let mut reads = Vec::new();
    for n in 1..=4_u32 {
        match n {
            2 => f.structures.push(structure.clone()),
            3 => {
                f.definitions.insert(at, definition.clone());
                f.normalized.push(normalized.clone());
            }
            4 => {
                f.definitions.retain(|d| d.id.as_str() != SOURCE);
                f.normalized.retain(|x| x.source.as_str() != SOURCE);
            }
            _ => {}
        }
        // Publishing checks the rows valid in n against what its build made of all of them.
        releases.push(build_next(&f));
        publishing(&mut store, n, &out, &state, journals.path(), &clock).unwrap();
        // AC-128: each read pinned to an earlier release returns what it returned before.
        for (k, before) in reads.iter().enumerate() {
            let k = u32::try_from(k).unwrap() + 1;
            assert_eq!(
                &everything_valid(&store, k),
                before,
                "release {k} after {n}"
            );
        }
        reads.push(everything_valid(&store, n));
        // Release n sees each of its rows once.
        for table in TABLES.iter().filter(|t| t.scope == Scope::PerRelease) {
            assert_eq!(repeated_keys(&store, table, n), 0, "{} in {n}", table.name);
        }
        let objects = store.int_everywhere(&format!(
            "SELECT COUNT(*) FROM objects WHERE {}",
            valid_in("", n)
        ));
        assert_eq!(
            objects.unsigned_abs(),
            releases[n as usize - 1].manifest.counts.objects
        );
    }
    // Release 2 shares release 1's objects; 3 and 4 write what the source changed.
    let shared = store.int_everywhere(&format!(
        "SELECT COUNT(*) FROM objects WHERE release = 1 AND {}",
        valid_in("", 2)
    ));
    assert_eq!(shared.unsigned_abs(), releases[0].manifest.counts.objects);
    for n in [3, 4] {
        let added =
            store.int_everywhere(&format!("SELECT COUNT(*) FROM entries WHERE release = {n}"));
        let retired =
            store.int_everywhere(&format!("SELECT COUNT(*) FROM entries WHERE retired = {n}"));
        assert!(added > 0 || retired > 0, "release {n} changes entries");
    }
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
