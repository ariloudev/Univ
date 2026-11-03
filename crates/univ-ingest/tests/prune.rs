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

use std::path::Path;

use memory_store::{Clock, MemoryStore, Op};
use release_fixture::{build_next, fixture};
use univ_ingest::publish::{
    Latest, PruneError, PruneOptions, Pruned, PublishOptions, prune, publish,
};
use univ_model::release::{TABLES, keys};

fn publishing(store: &mut MemoryStore, number: u32, out: &Path, journals: &Path, clock: &Clock) {
    let now = || clock.now();
    let options = PublishOptions {
        out,
        journals,
        now: &now,
    };
    publish(store, number, &options, &mut |_| {}).unwrap();
}

fn pruning(
    store: &mut MemoryStore,
    number: u32,
    out: &Path,
    journals: &Path,
    clock: &Clock,
) -> Result<Pruned, PruneError> {
    let now = || clock.now();
    let options = PruneOptions {
        out,
        journals,
        now: &now,
    };
    prune(store, number, &options, &mut |_| {})
}

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
    publishing(&mut store, 1, f.out.path(), journals.path(), &clock);
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
    publishing(&mut store, 2, f.out.path(), journals.path(), &clock);
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

    let pruned = pruning(&mut store, 1, f.out.path(), journals.path(), &clock).unwrap();
    assert!(!pruned.already);
    assert_eq!(pruned.latest, Latest(Some(2)));
    assert!(pruned.rows > 0 && pruned.files > 1);
    assert!(!store.r2.keys().any(|k| k.starts_with(&keys::prefix(1))));
    assert!(keys_of_2.iter().all(|k| store.r2.contains_key(k)));
    for t in TABLES.iter().filter(|t| t.per_release) {
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
    let again = pruning(&mut store, 1, f.out.path(), journals.path(), &clock).unwrap();
    assert!(again.already);
    assert_eq!(again.pruned_at, pruned.pruned_at);
}

#[test]
fn the_release_latest_points_to_is_not_pruned() {
    let f = fixture();
    build_next(&f);
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let mut store = MemoryStore::default();
    publishing(&mut store, 1, f.out.path(), journals.path(), &clock);
    let r2 = store.r2.clone();
    store.reset();

    let refused = pruning(&mut store, 1, f.out.path(), journals.path(), &clock).unwrap_err();
    assert_eq!(refused, PruneError::Latest { number: 1 });
    assert!(
        refused.to_string().contains("latest points to release 1"),
        "{refused}"
    );
    assert_eq!(store.r2, r2);
    assert!(store.ops.iter().all(|o| matches!(o, Op::Query(_))));
    assert_eq!(
        store.int("SELECT COUNT(*) FROM objects WHERE release = 1"),
        24
    );

    let unknown = pruning(&mut store, 7, f.out.path(), journals.path(), &clock).unwrap_err();
    assert_eq!(unknown, PruneError::NotListed { number: 7 });
}
