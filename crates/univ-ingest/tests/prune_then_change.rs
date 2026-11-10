//! Pruning and additive releases together: pruning the releases before the latest leaves every
//! read of the releases kept as it was, and the next release, built as a change against the
//! state, publishes only its change and reads as it would have without the prunes; and a release
//! whose shard gave up a cell keeps that cell's rows in its new shard when the release before is
//! pruned (prune-old-releases; share-unchanged-rows-and-files-between-releases).

#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod memory_store;
mod release_fixture;
mod valid_rows;

use std::collections::{BTreeMap, BTreeSet};

use memory_store::{Clock, MemoryStore, pruning, publishing};
use release_fixture::{Fixture, SHARD_BUDGET, build_next, build_next_within, fixture};
use univ_model::release::{Scope, TABLES, valid_in};
use valid_rows::everything_valid;

const SOURCE: &str = "mcconnachie-2012";

/// What every database holds valid in one release, as [`everything_valid`] reads it.
type Reads = Vec<BTreeMap<&'static str, Vec<String>>>;

/// The fixture without `SOURCE`, which [`changed`] puts back, and that source's definition and
/// normalized entries.
fn without_source() -> (
    Fixture,
    univ_ingest::source::SourceDefinition,
    univ_ingest::normalize::Normalized,
) {
    let mut f = fixture();
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
    (f, definition, normalized)
}

/// Builds and publishes releases 1 to 4 into a fresh store: 2 adds a structure, 3 the source
/// and 4 drops it again. Before building release 4, `prune_before_four` prunes releases 1 and
/// 2, checking that the reads of 3 do not change. Gives the store and each release's reads
/// taken right after it was published.
fn four_releases(prune_before_four: bool) -> (MemoryStore, Vec<Reads>) {
    let (mut f, definition, normalized) = without_source();
    let structure = f.structures.pop().unwrap();
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let mut store = MemoryStore::default();
    let (out, state) = (f.out.path().to_owned(), f.state.path().to_owned());
    let mut reads = Vec::new();
    for n in 1..=4_u32 {
        match n {
            2 => f.structures.push(structure.clone()),
            3 => {
                f.definitions.push(definition.clone());
                f.normalized.push(normalized.clone());
            }
            4 => {
                f.definitions.retain(|d| d.id.as_str() != SOURCE);
                f.normalized.retain(|x| x.source.as_str() != SOURCE);
                if prune_before_four {
                    let third = everything_valid(&store, 3);
                    for old in [1, 2] {
                        let pruned =
                            pruning(&mut store, old, &out, &state, journals.path(), &clock)
                                .unwrap();
                        assert_eq!(pruned.number, old);
                        assert_eq!(everything_valid(&store, 3), third, "after pruning {old}");
                    }
                }
            }
            _ => {}
        }
        build_next(&f);
        publishing(&mut store, n, &out, &state, journals.path(), &clock).unwrap();
        reads.push(everything_valid(&store, n));
    }
    (store, reads)
}

/// The rows each per-release table holds that release `n` added, summed over every database.
fn added_by(store: &MemoryStore, n: u32) -> i64 {
    TABLES
        .iter()
        .filter(|t| t.scope == Scope::PerRelease)
        .map(|t| {
            store.int_everywhere(&format!(
                "SELECT COUNT(*) FROM {} WHERE release = {n}",
                t.name
            ))
        })
        .sum()
}

#[test]
fn after_pruning_the_releases_before_the_latest_the_next_change_builds_and_reads_as_without() {
    let (kept, kept_reads) = four_releases(false);
    let (pruned, pruned_reads) = four_releases(true);
    // Release 4, built against the state after 1 and 2 were pruned, reads exactly as the same
    // release built with every release kept, and so does 3.
    assert_eq!(pruned_reads[3], kept_reads[3]);
    assert_eq!(everything_valid(&pruned, 3), everything_valid(&kept, 3));
    // It wrote only its change: the same rows added as with nothing pruned, and some.
    let added = added_by(&pruned, 4);
    assert!(
        added > 0,
        "release 4 drops a source and adds rows that retire it"
    );
    assert_eq!(added, added_by(&kept, 4));
    // Nothing valid only in 1 or 2 is left.
    for table in TABLES.iter().filter(|t| t.scope == Scope::PerRelease) {
        let only_early = pruned.int_everywhere(&format!(
            "SELECT COUNT(*) FROM {} WHERE retired IS NOT NULL AND retired <= 3",
            table.name
        ));
        assert_eq!(only_early, 0, "{}", table.name);
    }
}

/// The shard each object valid in release `n` is in, by the catalogue's index.
fn shards_of(store: &MemoryStore, n: u32) -> BTreeMap<i64, i64> {
    let sql = format!(
        "SELECT id, shard FROM object_shards WHERE {}",
        valid_in("", n)
    );
    let mut stmt = store.db.prepare(&sql).unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

/// The rows of object `id` that the object shard `db` holds; none in a shard bound but never
/// loaded, which holds no table.
fn held_in(db: &rusqlite::Connection, id: i64) -> i64 {
    let loaded: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'objects'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    if loaded == 0 {
        return 0;
    }
    db.query_row("SELECT COUNT(*) FROM objects WHERE id = ?1", [id], |r| {
        r.get(0)
    })
    .unwrap()
}

#[test]
fn pruning_after_a_cell_moved_shards_deletes_only_the_old_shard_s_copies() {
    let f = fixture();
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let mut store = MemoryStore::default();
    let (out, state) = (f.out.path().to_owned(), f.state.path().to_owned());
    // Release 1 packs into shards of twice the fixture's budget; release 2, of the same objects,
    // into the fixture's budget, so that shards that outgrow it give up their last cells, as
    // shard 0 will when release 13 adds the stars below G = 14.
    build_next_within(&f, 2 * SHARD_BUDGET);
    publishing(&mut store, 1, &out, &state, journals.path(), &clock).unwrap();
    build_next(&f);
    publishing(&mut store, 2, &out, &state, journals.path(), &clock).unwrap();
    let (before, after) = (shards_of(&store, 1), shards_of(&store, 2));
    let moved: BTreeSet<i64> = after
        .iter()
        .filter(|(id, shard)| before.get(id).is_some_and(|b| b != *shard))
        .map(|(id, _)| *id)
        .collect();
    assert!(
        !moved.is_empty(),
        "a tighter budget moves some cells to other shards"
    );
    let second = everything_valid(&store, 2);
    // Each moved object is held twice until release 1 is pruned: in its old shard, valid in 1,
    // and in its new one, valid in 2.
    let copies = |store: &MemoryStore, id: i64| -> i64 {
        store.shards.values().map(|db| held_in(db, id)).sum()
    };
    let one = *moved.iter().next().unwrap();
    assert_eq!(copies(&store, one), 2);

    pruning(&mut store, 1, &out, &state, journals.path(), &clock).unwrap();
    // Every read of release 2 is unchanged, and each moved object is held once, in its new
    // shard.
    assert_eq!(everything_valid(&store, 2), second);
    for id in &moved {
        assert_eq!(copies(&store, *id), 1, "object {id}");
        let shard = u32::try_from(after[id]).unwrap();
        let held = held_in(&store.shards[&shard], *id);
        assert_eq!(held, 1, "object {id} in shard {shard}");
    }
}
