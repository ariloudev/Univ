//! A local D1 and R2 of schema 5 brought under shared rows in place (publish-a-release-locally):
//! a store of schema 5 is made of two fixture releases as schema 5 held them, each release's
//! rows its own and its files under its prefix, then migrated, and compared with the same two
//! releases published under schema 6.

#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod memory_store;
mod release_fixture;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use memory_store::{Clock, MemoryStore, Op, Rows, pruning, publishing};
use release_fixture::{build_next, fixture, read_manifest};
use rusqlite::Connection;
use rusqlite::types::Value;
use univ_ingest::publish::migrate;
use univ_ingest::release::reindex::reindex;
use univ_ingest::release::release_dir;
use univ_model::release::manifest::Target;
use univ_model::release::{SCHEMA_VERSION, Scope, TABLES, keys, valid_in};

/// The schema of version 5, as univ-model wrote it before rows were shared.
const SCHEMA_5: &str = include_str!("fixtures/schema-5.sql");

/// The rows `sql` gives.
fn rows(db: &Connection, sql: &str) -> Vec<Vec<Value>> {
    let mut q = db.prepare(sql).unwrap();
    let n = q.column_count();
    q.query_map([], |r| (0..n).map(|i| r.get::<_, Value>(i)).collect())
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

/// Whether `db` holds a table `name`.
fn holds(db: &Connection, name: &str) -> bool {
    memory_store::int(
        db,
        &format!("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = '{name}'"),
    ) > 0
}

/// Every row `filter` selects of every table of [`Scope::PerRelease`] in each database of
/// `store`, the catalogue's first, without `release` and `retired`, as text, sorted by table:
/// `release_files` left out, which schema 5 has not ([`file_map`] compares it).
fn reads(store: &MemoryStore, filter: &str) -> Vec<BTreeMap<&'static str, Vec<String>>> {
    reads_but(store, filter, |_| false)
}

/// [`reads`], leaving out the columns `out` names.
/// The tables a store of schema 5 cannot hold, which a later schema added: `release_files`, and
/// `source_reading` of schema 7.
const LATER: [&str; 2] = ["release_files", "source_reading"];

fn reads_but(
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
                    t.scope == Scope::PerRelease && !LATER.contains(&t.name) && holds(db, t.name)
                })
                .map(|t| {
                    let columns: Vec<&str> = t
                        .columns
                        .iter()
                        .copied()
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
fn insert(to: &Connection, table: &str, columns: &[&str], rows: &[Vec<Value>]) {
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
fn as_schema_5(a: &MemoryStore, out: &Path, releases: &[u32]) -> MemoryStore {
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
            let columns = t.columns.join(", ");
            if t.scope == Scope::PerRelease {
                for r in releases {
                    let mut each = rows(
                        from,
                        &format!("SELECT {columns} FROM {} WHERE {}", t.name, valid_in("", r)),
                    );
                    let at = t.position("release").unwrap();
                    let named: Vec<usize> = (0..t.columns.len())
                        .filter(|i| t.columns[*i].ends_with("_key"))
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
                    insert(to, t.name, t.columns, &each);
                }
            } else {
                insert(
                    to,
                    t.name,
                    t.columns,
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
fn file_map(store: &MemoryStore, n: u32) -> BTreeSet<String> {
    let sql = format!(
        "SELECT path, sha256 FROM release_files WHERE {}",
        valid_in("", n)
    );
    rows(&store.db, &sql)
        .iter()
        .map(|r| format!("{r:?}"))
        .collect()
}

#[test]
fn a_store_of_schema_5_is_brought_under_shared_rows_with_every_pinned_read_unchanged() {
    const SOURCE: &str = "mcconnachie-2012";
    let mut f = fixture();
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let mut a = MemoryStore::default();
    let (out, state) = (f.out.path().to_owned(), f.state.path().to_owned());
    // Release 1 lacks a source that release 2 adds.
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
    build_next(&f);
    publishing(&mut a, 1, &out, &state, journals.path(), &clock).unwrap();
    f.definitions.insert(at, definition);
    f.normalized.push(normalized);
    build_next(&f);
    publishing(&mut a, 2, &out, &state, journals.path(), &clock).unwrap();

    let mut b = as_schema_5(&a, &out, &[1, 2]);
    for r in [1, 2] {
        // But for the keys it names files by, under each release's prefix.
        let keys = |c: &str| c.ends_with("_key");
        assert_eq!(
            reads_but(&b, &format!("release = {r}"), keys),
            reads_but(&a, &valid_in("", r), keys),
            "the store of schema 5 holds release {r} as schema 6 does"
        );
        assert_ne!(
            reads(&b, &format!("release = {r}")),
            reads(&a, &valid_in("", r))
        );
    }

    let work = tempfile::tempdir().unwrap();
    let databases = 1 + i64::try_from(b.shards.len()).unwrap();
    migrate(&mut b, work.path(), &mut |_| {}).unwrap();
    assert_eq!(
        b.int_everywhere(&format!(
            "SELECT COUNT(*) FROM schema_version WHERE version = {SCHEMA_VERSION}"
        )),
        databases
    );
    // Nothing built or uploaded again: D1 runs only the migration's own files, and R2 is given
    // only the files it held, under their SHA-256.
    for op in &b.ops {
        match op {
            Op::Execute(file) => assert!(file.contains("migrate-"), "{file}"),
            Op::Put(key) => assert!(key.starts_with(keys::FILES), "{key}"),
            _ => {}
        }
    }
    for r in [1, 2] {
        assert_eq!(
            reads(&b, &valid_in("", r)),
            reads(&a, &valid_in("", r)),
            "release {r}"
        );
        assert_eq!(file_map(&b, r), file_map(&a, r), "release {r}'s files");
    }
    // Every file once under its SHA-256, no key left under a release's prefix but manifests.
    let keys = |s: &MemoryStore| -> BTreeSet<String> { s.r2.keys().cloned().collect() };
    assert_eq!(keys(&b), keys(&a));
    for (key, bytes) in a.r2.iter().filter(|(k, _)| k.starts_with(keys::FILES)) {
        assert_eq!(&b.r2[key], bytes, "{key}");
    }

    // Migrating again changes nothing.
    b.reset();
    migrate(&mut b, work.path(), &mut |_| {}).unwrap();
    assert!(
        !b.ops
            .iter()
            .any(|o| matches!(o, Op::Execute(_) | Op::Put(_) | Op::Delete(_)))
    );

    // Pruning release 1 deletes the files it alone maps, though its manifest names its old keys.
    let held_by = |store: &MemoryStore, n: u32| -> BTreeSet<String> {
        let sql = format!(
            "SELECT DISTINCT sha256 FROM release_files WHERE {}",
            valid_in("", n)
        );
        rows(&store.db, &sql)
            .into_iter()
            .map(|r| match &r[0] {
                Value::Text(t) => keys::file(t),
                other => panic!("{other:?}"),
            })
            .collect()
    };
    let (first, second) = (held_by(&b, 1), held_by(&b, 2));
    let only_first: Vec<&String> = first.difference(&second).collect();
    assert!(
        !only_first.is_empty(),
        "release 1 maps files release 2 does not"
    );
    // A journal of schema 5 named the old keys: only the map of paths names the new ones.
    std::fs::remove_file(univ_ingest::publish::journal::file(journals.path(), 1)).unwrap();
    let pruned = pruning(&mut b, 1, &out, &state, journals.path(), &clock).unwrap();
    assert!(pruned.files > 0);
    assert!(only_first.iter().all(|k| !b.r2.contains_key(*k)));
    assert!(second.iter().all(|k| b.r2.contains_key(k)));
}

#[test]
fn a_release_built_on_a_migrated_and_reindexed_store_publishes_onto_it_as_its_change() {
    let f = fixture();
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let mut a = MemoryStore::default();
    let (out, state) = (f.out.path().to_owned(), f.state.path().to_owned());
    for n in [1, 2] {
        build_next(&f);
        publishing(&mut a, n, &out, &state, journals.path(), &clock).unwrap();
    }
    let mut b = as_schema_5(&a, &out, &[1, 2]);
    let dir = univ_ingest::state::current_dir(&state).unwrap();
    // A store of schema 5 is refused: it holds no map of paths to index.
    assert!(reindex(&Rows(&b), 2, &dir).is_err());
    // Migrated, then the state's numbering and row index of release 2 made again from it.
    let work = tempfile::tempdir().unwrap();
    assert!(migrate(&mut b, work.path(), &mut |_| {}).unwrap());
    reindex(&Rows(&b), 2, &dir).unwrap();
    for n in [1, 2] {
        std::fs::remove_file(univ_ingest::publish::journal::file(journals.path(), n)).unwrap();
    }
    let third = build_next(&f);
    assert_eq!(third.manifest.parent, Some(2));

    // Publishing it loads only its change.
    publishing(&mut b, 3, &out, &state, journals.path(), &clock).unwrap();
    assert_eq!(b.int("SELECT release FROM latest"), 3);
    // Release 3 reads as release 2 did, but for each scale domain's tile manifest, which names
    // its release.
    let but_domains = |mut r: Vec<BTreeMap<&'static str, Vec<String>>>| {
        for tables in &mut r {
            tables.remove("scale_domains");
        }
        r
    };
    assert_eq!(
        but_domains(reads(&b, &valid_in("", 3))),
        but_domains(reads(&a, &valid_in("", 2)))
    );
    for r in [1, 2] {
        assert_eq!(
            reads(&b, &valid_in("", r)),
            reads(&a, &valid_in("", r)),
            "release {r}"
        );
    }
    let objects = third
        .manifest
        .shards
        .iter()
        .flat_map(|s| &s.d1.tables)
        .filter(|t| t.table == "objects");
    for t in objects {
        assert_eq!((t.added, t.retired), (Some(0), Some(0)));
    }
}

#[test]
fn a_migration_stopped_while_it_deleted_the_old_keys_deletes_the_rest_when_run_again() {
    let f = fixture();
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let mut a = MemoryStore::default();
    let (out, state) = (f.out.path().to_owned(), f.state.path().to_owned());
    for n in [1, 2] {
        build_next(&f);
        publishing(&mut a, n, &out, &state, journals.path(), &clock).unwrap();
    }
    let old = |s: &MemoryStore| -> usize {
        s.r2.keys()
            .filter(|k| k.starts_with("releases/") && !k.ends_with(keys::MANIFEST))
            .count()
    };
    let work = tempfile::tempdir().unwrap();

    // A release a publish of schema 5 left part way is refused, changing nothing.
    let mut b = as_schema_5(&a, &out, &[1, 2]);
    b.db.execute(
        "UPDATE releases SET published_at = NULL WHERE number = 2",
        [],
    )
    .unwrap();
    let refused = migrate(&mut b, work.path(), &mut |_| {}).unwrap_err();
    assert!(
        refused.contains("release 2 was published only in part"),
        "{refused}"
    );
    assert!(
        !b.ops
            .iter()
            .any(|o| matches!(o, Op::Execute(_) | Op::Put(_)))
    );

    // R2 refuses the deletes once the catalogue is migrated.
    let mut b = as_schema_5(&a, &out, &[1, 2]);
    let before = old(&b);
    assert!(before > 0);
    b.fail_at = Some(|op| matches!(op, Op::Delete(_)));
    assert!(migrate(&mut b, work.path(), &mut |_| {}).is_err());
    assert_eq!(old(&b), before);
    assert_eq!(
        b.int(&format!(
            "SELECT COUNT(*) FROM schema_version WHERE version = {SCHEMA_VERSION}"
        )),
        1
    );
    b.fail_at = None;
    assert!(migrate(&mut b, work.path(), &mut |_| {}).unwrap());
    assert_eq!(old(&b), 0);
    assert!(!migrate(&mut b, work.path(), &mut |_| {}).unwrap());
}

#[test]
fn a_bound_shard_given_the_tables_of_schema_5_is_brought_under_shared_rows_whenever_found() {
    let f = fixture();
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let mut a = MemoryStore::default();
    let (out, state) = (f.out.path().to_owned(), f.state.path().to_owned());
    for n in [1, 2] {
        build_next(&f);
        publishing(&mut a, n, &out, &state, journals.path(), &clock).unwrap();
    }
    let mut b = as_schema_5(&a, &out, &[1, 2]);
    // A shard bound and given the tables of schema 5, never loaded: no version, no rows.
    let never_loaded = || {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(SCHEMA_5).unwrap();
        db
    };
    let shared = |db: &Connection| {
        let sql = "SELECT COUNT(*) FROM pragma_table_info('objects') WHERE name = 'retired'";
        rows(db, sql) == [[Value::Integer(1)]]
    };
    let spare = b.bound;
    b.bound += 2;
    b.shards.insert(spare, never_loaded());
    let work = tempfile::tempdir().unwrap();
    assert!(migrate(&mut b, work.path(), &mut |_| {}).unwrap());
    assert!(shared(&b.shards[&spare]));

    // One found after the catalogue was migrated, holding a row that a publish of release 3,
    // stopped part way, loaded over the old tables, with the schema 6 that publish recorded.
    let late = never_loaded();
    late.execute("INSERT INTO schema_version (version) VALUES (6)", [])
        .unwrap();
    let objects = TABLES.iter().find(|t| t.name == "objects").unwrap();
    let columns = objects.columns.join(", ");
    let mut row = rows(
        &a.shards[&0],
        &format!("SELECT {columns} FROM objects LIMIT 1"),
    );
    row[0][objects.position("release").unwrap()] = Value::Integer(3);
    insert(&late, "objects", objects.columns, &row);
    b.shards.insert(spare + 1, late);
    assert!(migrate(&mut b, work.path(), &mut |_| {}).unwrap());
    // Valid from release 3, which latest has not reached: not retired.
    assert_eq!(
        rows(
            &b.shards[&(spare + 1)],
            "SELECT release, retired FROM objects"
        ),
        [[Value::Integer(3), Value::Null]]
    );
    assert!(!migrate(&mut b, work.path(), &mut |_| {}).unwrap());
}
