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
mod schema_5_store;

use std::collections::{BTreeMap, BTreeSet};

use memory_store::{Clock, MemoryStore, Op, Rows, pruning, publishing};
use release_fixture::{build_next, fixture};
use rusqlite::Connection;
use rusqlite::types::Value;
use schema_5_store::{SCHEMA_5, as_schema_5, file_map, insert, reads, reads_but, rows, schema8};
use univ_ingest::publish::migrate;
use univ_ingest::release::reindex::reindex;
use univ_model::release::{SHARED_ROWS_SINCE, TABLES, keys, valid_in};

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
    let at = |v: u32| format!("SELECT COUNT(*) FROM schema_version WHERE version = {v}");
    assert_eq!(b.int_everywhere(&at(SHARED_ROWS_SINCE)), databases);
    // Then the JSON the old tables kept, into the rows of schema 8.
    assert!(schema8(&b));
    assert_eq!(b.int_everywhere(&at(8)), databases);
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
    assert!(!schema8(&b));
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
    assert!(schema8(&b));
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
            "SELECT COUNT(*) FROM schema_version WHERE version = {SHARED_ROWS_SINCE}"
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
