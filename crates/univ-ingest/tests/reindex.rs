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
use univ_ingest::release::local_d1::LocalD1;
use univ_ingest::release::reindex::reindex;
use univ_ingest::release::{Kept, NUMBERING, ROWS};
use univ_ingest::run::reindex_command;
use univ_ingest::state::State;
use univ_model::release::LOCAL_D1_FILES;
use univ_model::release::{Scope, TABLES};

#[test]
fn a_reindex_from_the_store_gives_the_numbering_the_build_kept_and_an_index_it_writes_nothing_on() {
    let f = fixture();
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let mut store = MemoryStore::default();
    let (out, state) = (f.out.path(), f.state.path());
    build_next(&f);
    publishing(&mut store, 1, out, state, journals.path(), &clock).unwrap();
    let built: Kept = State::read(state).unwrap().kept;
    // The state loses its numbering and row index.
    let dir = univ_ingest::state::current_dir(state).unwrap();
    std::fs::remove_file(dir.join(NUMBERING)).unwrap();
    std::fs::remove_dir_all(dir.join(ROWS)).unwrap();

    let reindexed = reindex(&Rows(&store), 1, &dir).unwrap();
    assert_eq!(reindexed.kept, built);
    assert!(reindexed.rows["objects"] > 0);

    // The same inputs again: every row of a table releases share is the release before's, but
    // for each scale domain's tile manifest, which names its release: the domain's row naming
    // it in scale_domains, and its path's in release_files, are written again, each once.
    let second = build_next(&f);
    let domains = Some(reindexed.rows["scale_domains"]);
    let estimates =
        std::iter::once(&second.manifest.d1).chain(second.manifest.shards.iter().map(|s| &s.d1));
    for (at, estimate) in estimates.enumerate() {
        for t in estimate.tables.iter().filter(|t| {
            TABLES
                .iter()
                .any(|s| s.name == t.table && s.scope == Scope::PerRelease)
        }) {
            let manifests = ["scale_domains", "release_files"].contains(&t.table.as_str());
            // The catalogue's, the first.
            let rewritten = if at == 0 && manifests {
                domains
            } else {
                Some(0)
            };
            assert_eq!((t.added, t.retired), (rewritten, rewritten), "{}", t.table);
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
    let files = persist.path().join(LOCAL_D1_FILES);
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
    // One given the release tables before rows were shared, and never loaded since.
    let older = rusqlite::Connection::open(files.join("older.sqlite")).unwrap();
    older
        .execute_batch(
            "CREATE TABLE latest (id INTEGER PRIMARY KEY, release INTEGER NOT NULL);
             CREATE TABLE objects (release INTEGER NOT NULL, id INTEGER NOT NULL);",
        )
        .unwrap();

    let local = LocalD1::open(persist.path()).unwrap();
    assert_eq!(local.latest, 2);
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let from_files = reindex(&local, 2, a.path()).unwrap();
    let from_store = reindex(&Rows(&store), 2, b.path()).unwrap();
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

    // Holding objects at that older schema, it is refused rather than read.
    older
        .execute("INSERT INTO objects (release, id) VALUES (1, 1)", [])
        .unwrap();
    let refused = LocalD1::open(persist.path()).err().unwrap();
    assert!(
        refused.contains("older.sqlite holds objects of a schema older"),
        "{refused}"
    );
    older.execute("DELETE FROM objects", []).unwrap();

    // `univ-ingest reindex` on a state kept before there was a numbering, as release 10's was:
    // the ledger alone names the release, and the numbering and index come back whole.
    let built = State::read(state).unwrap().kept;
    let dir = univ_ingest::state::current_dir(state).unwrap();
    std::fs::remove_file(dir.join(NUMBERING)).unwrap();
    std::fs::remove_dir_all(dir.join(ROWS)).unwrap();
    reindex_command(state, persist.path()).unwrap();
    assert_eq!(State::read(state).unwrap().kept, built);
    assert_eq!(listed(&dir), listed(a.path()));
}
