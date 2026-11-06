//! Pruning a release of an in-memory D1 and R2 (prune-old-releases): its files and rows
//! deleted, the release kept in the list of releases with its pruning time, the id ledger kept
//! so every id still resolves, and the release `latest` points to refused.

#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod memory_store;
mod release_fixture;

use memory_store::{Clock, MemoryStore, Op, pruning, publishing};
use std::collections::BTreeSet;

use release_fixture::{build_next, fixture, read_manifest};
use univ_ingest::publish::{Latest, PruneError};
use univ_model::release::{Scope, TABLES, keys, valid_in};

/// The ids an id stands for in release `latest`, following the ledger's replacements: empty
/// when retired. Fails when the ledger and the release do not account for the id.
fn resolve(store: &MemoryStore, id: i64, latest: i64) -> Vec<i64> {
    let issued = store.int(&format!(
        "SELECT COUNT(*) FROM ids_issued WHERE first <= {id} AND {id} <= last"
    ));
    assert_eq!(issued, 1, "the ledger records which release issued {id}");
    let present = store.int(&format!(
        "SELECT COUNT(*) FROM object_shards WHERE {} AND id = {id}",
        valid_in("", latest)
    ));
    if present == 1 {
        return vec![id];
    }
    let by: String = store
        .db
        .query_row(
            &format!("SELECT replaced_by FROM ids_replaced WHERE id = {id}"),
            [],
            |r| r.get(0),
        )
        .map_err(|e| format!("{id} is neither in release {latest} nor replaced: {e}"))
        .unwrap();
    let by: Vec<i64> = serde_json::from_str(&by).unwrap();
    by.into_iter()
        .flat_map(|b| resolve(store, b, latest))
        .collect()
}

/// The ids of the objects of release `number`, by the catalogue's index of the object shards.
fn ids_of(store: &MemoryStore, number: u32) -> Vec<i64> {
    let mut s = store
        .db
        .prepare(&format!(
            "SELECT id FROM object_shards WHERE {}",
            valid_in("", "?1")
        ))
        .unwrap();
    s.query_map([number], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

/// Each table's count of the rows valid in release `n`, in every database.
fn valid_counts(store: &MemoryStore, n: u32) -> Vec<i64> {
    TABLES
        .iter()
        .filter(|t| t.scope == Scope::PerRelease)
        .map(|t| {
            store.int_everywhere(&format!(
                "SELECT COUNT(*) FROM {} WHERE {}",
                t.name,
                valid_in("", n)
            ))
        })
        .collect()
}

/// The rows valid in release `n` and not in `kept`, of every table, in every database.
fn only_in(store: &MemoryStore, n: u32, kept: u32) -> i64 {
    TABLES
        .iter()
        .filter(|t| t.scope == Scope::PerRelease)
        .map(|t| {
            store.int_everywhere(&format!(
                "SELECT COUNT(*) FROM {} WHERE {} AND NOT ({})",
                t.name,
                valid_in("", n),
                valid_in("", kept)
            ))
        })
        .sum()
}

#[test]
fn pruning_deletes_the_files_and_rows_and_keeps_the_release_listed_and_its_ids_resolving() {
    let mut f = fixture();
    build_next(&f);
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let mut store = MemoryStore::default();
    let (out, state) = (f.out.path().to_owned(), f.state.path().to_owned());
    let publish = |store: &mut MemoryStore, n| {
        publishing(store, n, &out, &state, journals.path(), &clock).unwrap();
    };
    let prune = |store: &mut MemoryStore| pruning(store, 1, &out, &state, journals.path(), &clock);
    publish(&mut store, 1);
    let ids_of_1 = ids_of(&store, 1);
    // Release 2 leaves out a source, so the objects only it held are retired.
    let dropped = "mcconnachie-2012";
    let at = f
        .normalized
        .iter()
        .position(|n| n.source.as_str() == dropped)
        .unwrap();
    f.normalized.remove(at);
    f.definitions.retain(|d| d.id.as_str() != dropped);
    build_next(&f);
    publish(&mut store, 2);
    let ledger_before = (
        store.int("SELECT COUNT(*) FROM ids_issued"),
        store.int("SELECT COUNT(*) FROM ids_replaced"),
    );
    assert!(ledger_before.1 > 0, "release 2 retired some ids");
    // Each table's rows valid in release 2, some of them added by release 1 and shared.
    let rows_of_2 = valid_counts(&store, 2);
    assert!(
        store.int_everywhere(&format!(
            "SELECT COUNT(*) FROM objects WHERE release = 1 AND {}",
            valid_in("", 2)
        )) > 0,
        "release 2 shares rows release 1 added"
    );
    // Release 2's files and manifest, and the files release 1 alone held.
    let r2_of = |n: u32| -> BTreeSet<String> {
        let mut held: BTreeSet<String> = read_manifest(&univ_ingest::release::release_dir(&out, n))
            .files
            .iter()
            .filter_map(|f| f.key.clone())
            .collect();
        held.insert(keys::manifest(n));
        held
    };
    let keys_of_2 = r2_of(2);
    let only_1: BTreeSet<String> = r2_of(1).difference(&keys_of_2).cloned().collect();
    assert!(only_1.len() > 1, "release 1 holds files release 2 does not");
    store.reset();
    let built_1 = univ_ingest::release::release_dir(&out, 1);
    // The local build is deleted last: it is there after every D1 and R2 operation.
    let watched = built_1.clone();
    store.observer = Some(Box::new(move |op, _, _, _| {
        assert!(watched.exists(), "the local build is gone at {op:?}");
    }));

    let pruned = prune(&mut store).unwrap();
    assert!(store.ops.iter().any(|o| matches!(o, Op::Delete(_))));
    assert!(!pruned.already);
    // The local build of release 1 is deleted, and release 2's kept.
    assert_eq!(pruned.build.as_deref(), Some(built_1.as_path()));
    assert!(!built_1.exists());
    assert!(univ_ingest::release::release_dir(&out, 2).exists());
    assert_eq!(pruned.latest, Latest(Some(2)));
    assert!(pruned.rows > 0 && pruned.files > 1);
    assert!(!store.r2.keys().any(|k| only_1.contains(k)));
    assert!(keys_of_2.iter().all(|k| store.r2.contains_key(k)));
    // The rows release 1 alone held are gone from the catalogue and from every object shard;
    // those it shares with release 2 are kept.
    let loaded = store.int_everywhere("SELECT COUNT(*) FROM schema_version");
    assert!(
        loaded >= 3,
        "the fixture spans object shards: {loaded} databases"
    );
    assert_eq!(only_in(&store, 1, 2), 0);
    assert_eq!(valid_counts(&store, 2), rows_of_2);

    // AC-100: the release stays in the list of releases, with its pruning time.
    let (published, pruned_at): (Option<String>, Option<String>) = store
        .db
        .query_row(
            "SELECT published_at, pruned_at FROM releases WHERE number = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert!(published.is_some());
    assert_eq!(pruned_at.as_deref(), Some(pruned.pruned_at.as_str()));

    // AC-99: the id ledger is kept, and every id of release 1 resolves through it.
    let ledger_after = (
        store.int("SELECT COUNT(*) FROM ids_issued"),
        store.int("SELECT COUNT(*) FROM ids_replaced"),
    );
    assert_eq!(ledger_after, ledger_before);
    let mut retired = 0;
    for id in &ids_of_1 {
        if resolve(&store, *id, 2).is_empty() {
            retired += 1;
        }
    }
    assert!(retired > 0 && retired < ids_of_1.len(), "{retired}");
    assert!(!univ_ingest::publish::journal::file(journals.path(), 1).exists());

    // Pruning again keeps the first pruning time, its local build already gone.
    store.reset();
    let again = prune(&mut store).unwrap();
    assert!(again.already);
    assert_eq!(again.pruned_at, pruned.pruned_at);
    assert_eq!(again.build, None);
}

#[test]
fn the_release_latest_points_to_is_not_pruned() {
    let f = fixture();
    build_next(&f);
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let mut store = MemoryStore::default();
    publishing(
        &mut store,
        1,
        f.out.path(),
        f.state.path(),
        journals.path(),
        &clock,
    )
    .unwrap();
    let r2 = store.r2.clone();
    store.reset();

    let refused = pruning(
        &mut store,
        1,
        f.out.path(),
        f.state.path(),
        journals.path(),
        &clock,
    )
    .unwrap_err();
    assert_eq!(refused, PruneError::Latest { number: 1 });
    assert!(
        refused.to_string().contains("latest points to release 1"),
        "{refused}"
    );
    assert_eq!(store.r2, r2);
    assert!(store.ops.iter().all(|o| matches!(o, Op::Query(_))));
    assert_eq!(
        store.int_everywhere("SELECT COUNT(*) FROM objects WHERE release = 1"),
        25
    );

    let unknown = pruning(
        &mut store,
        7,
        f.out.path(),
        f.state.path(),
        journals.path(),
        &clock,
    )
    .unwrap_err();
    assert_eq!(unknown, PruneError::NotListed { number: 7 });
}

#[test]
fn with_neither_manifest_nor_journal_pruning_refuses_and_with_the_journal_alone_it_prunes() {
    let f = fixture();
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let mut store = MemoryStore::default();
    let (out, state) = (f.out.path(), f.state.path());
    for n in [1, 2] {
        build_next(&f);
        publishing(&mut store, n, out, state, journals.path(), &clock).unwrap();
    }
    // Release 1's files that release 2, left unpruned, does not hold, and its manifest.
    let r2_of = |n: u32| -> BTreeSet<String> {
        read_manifest(&univ_ingest::release::release_dir(out, n))
            .files
            .iter()
            .filter_map(|f| f.key.clone())
            .collect()
    };
    let held_by_2 = r2_of(2);
    let mut keys_of_1: BTreeSet<String> = r2_of(1).difference(&held_by_2).cloned().collect();
    keys_of_1.insert(keys::manifest(1));
    // R2 lost the manifest, and the build output no longer holds the release.
    store.r2.remove(&keys::key(1, keys::MANIFEST));
    std::fs::remove_dir_all(univ_ingest::release::release_dir(out, 1)).unwrap();
    let journal = univ_ingest::publish::journal::file(journals.path(), 1);
    let kept = std::fs::read(&journal).unwrap();
    std::fs::remove_file(&journal).unwrap();
    store.reset();

    let refused = pruning(&mut store, 1, out, state, journals.path(), &clock).unwrap_err();
    assert_eq!(refused, PruneError::NoFileList { number: 1 });
    assert!(
        refused
            .to_string()
            .contains("nothing tells which R2 files it has"),
        "{refused}"
    );
    assert!(!store.ops.iter().any(|o| matches!(o, Op::Delete(_))));
    assert_eq!(
        store.int("SELECT COUNT(*) FROM releases WHERE number = 1 AND pruned_at IS NULL"),
        1
    );

    // The journal's list of files uploaded names them.
    std::fs::write(&journal, kept).unwrap();
    let pruned = pruning(&mut store, 1, out, state, journals.path(), &clock).unwrap();
    assert!(pruned.listed && pruned.rows > 0);
    assert_eq!(pruned.files, keys_of_1.len());
    assert!(!store.r2.keys().any(|k| keys_of_1.contains(k)));
    assert!(held_by_2.iter().all(|k| store.r2.contains_key(k)));
}

#[test]
fn a_release_never_listed_whose_files_were_uploaded_has_them_deleted() {
    let f = fixture();
    let schemas = build_next(&f).manifest.shards.len() + 1;
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let (out, state) = (f.out.path(), f.state.path());
    // The publish stops after each database's schema and two R2 files, before any table is
    // loaded.
    let mut store = MemoryStore {
        fail_after: Some(schemas + 2),
        ..MemoryStore::default()
    };
    publishing(&mut store, 1, out, state, journals.path(), &clock).unwrap_err();
    let uploaded: Vec<String> = store.r2.keys().cloned().collect();
    assert_eq!(uploaded.len(), 2);
    assert_eq!(store.int("SELECT COUNT(*) FROM releases"), 0);
    store.reset();
    let built = univ_ingest::release::release_dir(out, 1);
    let r2_files = univ_ingest::release::read_manifest(&built, 1)
        .unwrap()
        .0
        .files
        .iter()
        .filter(|f| f.key.is_some())
        .count();

    let pruned = pruning(&mut store, 1, out, state, journals.path(), &clock).unwrap();
    assert!(!pruned.listed);
    // The keys its journal and its manifest in the build output name, deleted, then the build.
    assert_eq!((pruned.files, pruned.rows), (r2_files + 1, 0));
    assert_eq!(pruned.build.as_deref(), Some(built.as_path()));
    assert!(!built.exists());
    assert!(store.r2.is_empty(), "{:?}", store.r2.keys());
    assert!(!univ_ingest::publish::journal::file(journals.path(), 1).exists());
    // Nothing is left of it: pruning again finds nothing to prune.
    let again = pruning(&mut store, 1, out, state, journals.path(), &clock).unwrap_err();
    assert_eq!(again, PruneError::NotListed { number: 1 });
}

#[test]
fn a_file_a_release_part_way_through_publishing_holds_is_not_pruned() {
    let f = fixture();
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let mut store = MemoryStore::default();
    let (out, state) = (f.out.path(), f.state.path());
    for n in [1, 2] {
        build_next(&f);
        publishing(&mut store, n, out, state, journals.path(), &clock).unwrap();
    }
    let third = build_next(&f);
    // A tile releases 1 and 3 hold, but not 2: release 3 uploads it.
    let tile = third
        .manifest
        .files
        .iter()
        .find(|f| f.path.starts_with("tiles/") && !f.path.ends_with("manifest.json"))
        .unwrap();
    store
        .db
        .execute(
            "DELETE FROM release_files WHERE release = 2 AND path = ?1",
            [&tile.path],
        )
        .unwrap();
    // Release 3 stops at its first table, its R2 files up and its release_files not yet in D1:
    // only its journal says it holds the tile.
    store.reset();
    store.fail_at = Some(|op| matches!(op, Op::Execute(p) if !p.contains("schema")));
    publishing(&mut store, 3, out, state, journals.path(), &clock).unwrap_err();
    store.reset();
    let pruned = pruning(&mut store, 1, out, state, journals.path(), &clock).unwrap();
    assert!(pruned.files > 0);
    assert!(store.r2.contains_key(&keys::file(&tile.sha256)));
}
