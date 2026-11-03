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
use release_fixture::{build_next, fixture};
use univ_ingest::publish::{Latest, PruneError};
use univ_model::release::{Scope, TABLES, keys};

/// The ids an id stands for in release `latest`, following the ledger's replacements: empty
/// when retired. Fails when the ledger and the release do not account for the id.
fn resolve(store: &MemoryStore, id: i64, latest: i64) -> Vec<i64> {
    let issued = store.int(&format!(
        "SELECT COUNT(*) FROM ids_issued WHERE first <= {id} AND {id} <= last"
    ));
    assert_eq!(issued, 1, "the ledger records which release issued {id}");
    let present = store.int(&format!(
        "SELECT COUNT(*) FROM objects WHERE release = {latest} AND id = {id}"
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
    let ids_of_1: Vec<i64> = {
        let mut s = store
            .db
            .prepare("SELECT id FROM objects WHERE release = 1")
            .unwrap();
        s.query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
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
    let rows_of_2 = store.int("SELECT COUNT(*) FROM objects WHERE release = 2");
    let keys_of_2: Vec<String> = store
        .r2
        .keys()
        .filter(|k| k.starts_with(&keys::prefix(2)))
        .cloned()
        .collect();
    store.reset();

    let pruned = prune(&mut store).unwrap();
    assert!(!pruned.already);
    assert_eq!(pruned.latest, Latest(Some(2)));
    assert!(pruned.rows > 0 && pruned.files > 1);
    assert!(!store.r2.keys().any(|k| k.starts_with(&keys::prefix(1))));
    assert!(keys_of_2.iter().all(|k| store.r2.contains_key(k)));
    for t in TABLES.iter().filter(|t| t.scope == Scope::PerRelease) {
        let left = store.int(&format!(
            "SELECT COUNT(*) FROM {} WHERE release = 1",
            t.name
        ));
        assert_eq!(left, 0, "{}", t.name);
    }
    assert_eq!(
        store.int("SELECT COUNT(*) FROM objects WHERE release = 2"),
        rows_of_2
    );

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

    // Pruning again keeps the first pruning time.
    let again = prune(&mut store).unwrap();
    assert!(again.already);
    assert_eq!(again.pruned_at, pruned.pruned_at);
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
        store.int("SELECT COUNT(*) FROM objects WHERE release = 1"),
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
    let keys_of_1: Vec<String> = store
        .r2
        .keys()
        .filter(|k| k.starts_with(&keys::prefix(1)))
        .cloned()
        .collect();
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
    assert!(!store.r2.keys().any(|k| k.starts_with(&keys::prefix(1))));
}

#[test]
fn a_release_never_listed_whose_files_were_uploaded_has_them_deleted() {
    let f = fixture();
    build_next(&f);
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let (out, state) = (f.out.path(), f.state.path());
    // The publish stops after the schema and two R2 files, before any table is loaded.
    let mut store = MemoryStore {
        fail_after: Some(3),
        ..MemoryStore::default()
    };
    publishing(&mut store, 1, out, state, journals.path(), &clock).unwrap_err();
    let uploaded: Vec<String> = store.r2.keys().cloned().collect();
    assert_eq!(uploaded.len(), 2);
    assert_eq!(store.int("SELECT COUNT(*) FROM releases"), 0);
    store.reset();

    let pruned = pruning(&mut store, 1, out, state, journals.path(), &clock).unwrap();
    assert!(!pruned.listed);
    // The keys its journal and its manifest in the build output name, deleted.
    let built = univ_ingest::release::release_dir(out, 1);
    let r2_files = univ_ingest::release::read_manifest(&built, 1)
        .unwrap()
        .0
        .files
        .iter()
        .filter(|f| f.key.is_some())
        .count();
    assert_eq!((pruned.files, pruned.rows), (r2_files + 1, 0));
    assert!(store.r2.is_empty(), "{:?}", store.r2.keys());
    assert!(!univ_ingest::publish::journal::file(journals.path(), 1).exists());
    // Nothing is left of it: pruning again finds nothing to prune.
    let again = pruning(&mut store, 1, out, state, journals.path(), &clock).unwrap_err();
    assert_eq!(again, PruneError::NotListed { number: 1 });
}
