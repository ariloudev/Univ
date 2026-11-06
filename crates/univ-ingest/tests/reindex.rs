//! The numbering and row index of the release `latest` points to, read back from the store it
//! was published to (build-a-release): the numbering its build kept, and an index on which the
//! next build of the same inputs writes no row.

#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod memory_store;
mod release_fixture;

use memory_store::{Clock, MemoryStore, Rows, publishing};
use release_fixture::{build_next, fixture};
use univ_ingest::release::local_d1::{D1_FILES, LocalD1};
use univ_ingest::release::reindex::reindex;
use univ_ingest::release::{Kept, NUMBERING, ROWS};
use univ_ingest::state::State;
use univ_model::release::{Scope, TABLES};

/// The directory of the state the state directory `state` holds now.
fn current(state: &std::path::Path) -> std::path::PathBuf {
    let named = std::fs::read_to_string(state.join("current")).unwrap();
    state.join(named.trim())
}

#[test]
fn a_reindex_from_the_store_gives_the_numbering_the_build_kept_and_an_index_it_writes_nothing_on() {
    let f = fixture();
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let mut store = MemoryStore::default();
    let (out, state) = (f.out.path(), f.state.path());
    // Release 1 stores its number in no bytes, as SQLite does 0 and 1, so its rows are
    // estimated a byte smaller than any later release's: the comparison starts at release 2.
    for n in [1, 2] {
        build_next(&f);
        publishing(&mut store, n, out, state, journals.path(), &clock).unwrap();
    }
    let built: Kept = State::read(state).unwrap().kept;
    // The state loses its numbering and row index.
    let dir = current(state);
    std::fs::remove_file(dir.join(NUMBERING)).unwrap();
    std::fs::remove_dir_all(dir.join(ROWS)).unwrap();

    let reindexed = reindex(&mut Rows(&store), (2, 6), &dir).unwrap();
    assert_eq!(reindexed.kept, built);
    assert!(reindexed.rows["objects"] > 0);

    // The same inputs again: every row of a table releases share is the release before's, but
    // each scale domain's tile manifest, which names its release: its path's row in
    // release_files and the domain's row naming it in scale_domains.
    let second = build_next(&f);
    let estimates =
        std::iter::once(&second.manifest.d1).chain(second.manifest.shards.iter().map(|s| &s.d1));
    for estimate in estimates {
        for t in estimate.tables.iter().filter(|t| {
            !["scale_domains", "release_files"].contains(&t.table.as_str())
                && TABLES
                    .iter()
                    .any(|s| s.name == t.table && s.scope == Scope::PerRelease)
        }) {
            assert_eq!((t.added, t.retired), (Some(0), Some(0)), "{}", t.table);
        }
    }
}

#[test]
fn the_local_d1_is_read_from_its_files_each_told_by_what_it_holds() {
    let f = fixture();
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let mut store = MemoryStore::default();
    let (out, state) = (f.out.path(), f.state.path());
    for n in [1, 2] {
        build_next(&f);
        publishing(&mut store, n, out, state, journals.path(), &clock).unwrap();
    }
    // Each database written to a file named as Miniflare names it, by no order of its own, beside
    // a database of no release and a shard bound but never loaded.
    let persist = tempfile::tempdir().unwrap();
    let files = persist.path().join(D1_FILES);
    std::fs::create_dir_all(&files).unwrap();
    let databases = std::iter::once(&store.db).chain(store.shards.values());
    for (i, db) in databases.enumerate() {
        let file = files.join(format!(
            "{:064x}.sqlite",
            0x9e37_79b9_u64.wrapping_mul(i as u64 + 1)
        ));
        db.execute(&format!("VACUUM INTO '{}'", file.display()), [])
            .unwrap();
    }
    let layers = rusqlite::Connection::open(files.join("layers.sqlite")).unwrap();
    layers
        .execute_batch("CREATE TABLE layers (id TEXT)")
        .unwrap();
    let empty = rusqlite::Connection::open(files.join("empty.sqlite")).unwrap();
    empty.execute_batch(univ_model::release::SCHEMA).unwrap();

    let mut local = LocalD1::open(persist.path()).unwrap();
    assert_eq!((local.latest, local.version), (2, 6));
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let from_files = reindex(&mut local, (2, 6), a.path()).unwrap();
    let from_store = reindex(&mut Rows(&store), (2, 6), b.path()).unwrap();
    assert_eq!(from_files, from_store);
    assert_eq!(from_files.kept, State::read(state).unwrap().kept);
    // The same index, file for file.
    let listed = |dir: &std::path::Path| -> Vec<(String, Vec<u8>)> {
        let mut all = Vec::new();
        for db in std::fs::read_dir(dir.join(ROWS)).unwrap() {
            for t in std::fs::read_dir(db.unwrap().path()).unwrap() {
                let t = t.unwrap().path();
                let name = t.strip_prefix(dir).unwrap().display().to_string();
                all.push((name, std::fs::read(&t).unwrap()));
            }
        }
        all.sort();
        all
    };
    assert_eq!(listed(a.path()), listed(b.path()));
}
