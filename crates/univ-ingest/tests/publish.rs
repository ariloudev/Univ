//! Publishing the fixture's releases to an in-memory D1 and R2 (publish-a-release-locally):
//! every file checked against the manifest, reads that name no release answering from the
//! previous release until `latest` moves in one write, an interrupted publish continuing from
//! its journal, and a published release refused.

#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod memory_store;
mod release_fixture;

use std::path::Path;

use memory_store::{Clock, MemoryStore, Op, int};
use release_fixture::{build_next, fixture};
use univ_ingest::publish::journal::Journal;
use univ_ingest::publish::{Latest, PublishError, PublishOptions, Published, publish};
use univ_ingest::tiles::sha256;
use univ_model::release::manifest::Target;
use univ_model::release::{ReleaseManifest, keys};

fn options<'a>(
    out: &'a Path,
    journals: &'a Path,
    clock: &'a dyn Fn() -> String,
) -> PublishOptions<'a> {
    PublishOptions {
        out,
        journals,
        now: clock,
    }
}

fn publishing(
    store: &mut MemoryStore,
    number: u32,
    out: &Path,
    journals: &Path,
    clock: &Clock,
) -> Result<Published, PublishError> {
    let now = || clock.now();
    let mut lines = Vec::new();
    publish(store, number, &options(out, journals, &now), &mut |l| {
        lines.push(l);
    })
}

/// What `latest` points to, by D1.
fn latest(store: &MemoryStore) -> Option<i64> {
    store
        .db
        .query_row("SELECT release FROM latest", [], |r| r.get(0))
        .ok()
}

/// Whether `sql` writes `latest`.
fn writes_latest(sql: &str) -> bool {
    let s = sql.to_uppercase();
    (s.starts_with("INSERT") || s.starts_with("UPDATE") || s.starts_with("DELETE"))
        && s.contains("LATEST")
}

#[test]
fn every_uploaded_file_is_checked_against_its_sha256_and_latest_moves_last() {
    let f = fixture();
    let built = build_next(&f);
    let m: &ReleaseManifest = &built.manifest;
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let mut store = MemoryStore::default();

    let done = publishing(&mut store, 1, f.out.path(), journals.path(), &clock).unwrap();
    assert_eq!(done.number, 1);
    assert_eq!(done.previous, Latest(None));
    assert_eq!((done.uploaded, done.journaled), (m.files.len() + 1, 0));

    // AC-102: each R2 file is in R2 with the manifest's SHA-256, read back after it was put.
    for file in m.files.iter().filter(|f| f.target == Target::R2) {
        let key = file.key.as_deref().unwrap();
        assert_eq!(sha256(&store.r2[key]), file.sha256, "{key}");
        let put = store
            .ops
            .iter()
            .position(|o| *o == Op::Put(key.into()))
            .unwrap();
        assert_eq!(store.ops[put + 1], Op::Get(key.into()), "{key} read back");
    }
    assert!(store.r2.contains_key(&keys::key(1, keys::MANIFEST)));
    // Each D1 table holds the rows the manifest counts.
    for t in m.d1.tables.iter().filter(|t| t.rows > 0) {
        let column = if t.table == "releases" {
            "number"
        } else {
            "release"
        };
        let rows = store.int(&format!(
            "SELECT COUNT(*) FROM {} WHERE {column} <= 1",
            t.table
        ));
        assert_eq!(rows, i64::try_from(t.rows).unwrap(), "{}", t.table);
    }
    let journal = Journal::read(journals.path(), 1).unwrap().unwrap();
    assert_eq!(journal.done.len(), m.files.len() + 1);
    assert!(
        journal
            .done
            .iter()
            .any(|d| d.checked == "objects: 24 rows of release 1")
    );

    // AC-104: one single-statement write moves latest, after every upload.
    let moves: Vec<usize> = (0..store.ops.len())
        .filter(|&i| matches!(&store.ops[i], Op::Query(sql) if writes_latest(sql)))
        .collect();
    assert_eq!(moves.len(), 1, "{:?}", store.ops);
    let last_upload = store.ops.iter().rposition(Op::uploads).unwrap();
    assert!(moves[0] > last_upload);
    let Op::Query(sql) = &store.ops[moves[0]] else {
        unreachable!()
    };
    assert_eq!(sql.matches(';').count(), 0, "one statement: {sql}");
    assert_eq!(latest(&store), Some(1));
    let published_at: String = store
        .db
        .query_row(
            "SELECT published_at FROM releases WHERE number = 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(published_at, done.published_at);
}

#[test]
fn a_file_that_reads_back_with_another_sha256_stops_the_publish() {
    let f = fixture();
    build_next(&f);
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let mut store = MemoryStore {
        corrupt_reads: true,
        ..MemoryStore::default()
    };
    let error = publishing(&mut store, 1, f.out.path(), journals.path(), &clock).unwrap_err();
    let message = error.to_string();
    assert!(message.contains("back with SHA-256"), "{message}");
    assert!(message.contains("latest points to no release"), "{message}");
    assert_eq!(latest(&store), None);

    // A local file changed after its build is refused before anything is uploaded.
    let mut store = MemoryStore::default();
    let dir = univ_ingest::release::release_dir(f.out.path(), 1);
    std::fs::write(dir.join(keys::rules("matching")), "changed").unwrap();
    let refused = publishing(&mut store, 1, f.out.path(), journals.path(), &clock).unwrap_err();
    assert!(
        matches!(&refused, PublishError::NotBuilt { problem, .. } if problem.contains("SHA-256")),
        "{refused}"
    );
    assert_eq!(store.ops, []);
}

#[test]
fn until_latest_moves_reads_that_name_no_release_answer_from_the_previous_release() {
    let f = fixture();
    build_next(&f);
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let mut store = MemoryStore::default();
    publishing(&mut store, 1, f.out.path(), journals.path(), &clock).unwrap();
    let objects_of_1 = store.int("SELECT COUNT(*) FROM objects WHERE release = 1");
    let keys_of_1: Vec<(String, Vec<u8>)> = store.r2.clone().into_iter().collect();
    build_next(&f);
    store.reset();

    // After every operation of release 2's publish but the move, a read naming no release
    // follows latest to release 1 and finds release 1's rows and files as they were.
    let seen = std::rc::Rc::new(std::cell::Cell::new((0, false)));
    let watch = seen.clone();
    store.observer = Some(Box::new(move |op, db, r2| {
        let moved = matches!(op, Op::Query(sql) if writes_latest(sql)) || watch.get().1;
        let latest = int(db, "SELECT release FROM latest");
        let read = int(
            db,
            "SELECT COUNT(*) FROM objects WHERE release = (SELECT release FROM latest)",
        );
        if moved {
            assert_eq!(latest, 2);
        } else {
            assert_eq!((latest, read), (1, objects_of_1), "after {op:?}");
            for (key, bytes) in &keys_of_1 {
                assert_eq!(r2.get(key), Some(bytes), "{key} after {op:?}");
            }
        }
        watch.set((watch.get().0 + 1, moved));
    }));
    let done = publishing(&mut store, 2, f.out.path(), journals.path(), &clock).unwrap();
    assert_eq!(done.previous, Latest(Some(1)));
    assert!(seen.get().0 > 20 && seen.get().1);
    assert_eq!(latest(&store), Some(2));
    assert_eq!(
        store.int("SELECT COUNT(*) FROM objects WHERE release = 1"),
        objects_of_1
    );
}

#[test]
fn an_interrupted_publish_leaves_latest_and_continues_from_its_journal() {
    let f = fixture();
    build_next(&f);
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let mut store = MemoryStore::default();
    publishing(&mut store, 1, f.out.path(), journals.path(), &clock).unwrap();
    let built = build_next(&f);
    store.reset();
    store.fail_after = Some(9);

    let error = publishing(&mut store, 2, f.out.path(), journals.path(), &clock).unwrap_err();
    let PublishError::Interrupted {
        latest: at,
        done,
        total,
        ..
    } = &error
    else {
        panic!("{error}")
    };
    assert_eq!(
        (*at, *total),
        (Latest(Some(1)), built.manifest.files.len() + 1)
    );
    assert!(*done > 0 && *done < *total, "{error}");
    let message = error.to_string();
    assert!(message.contains("latest points to release 1"), "{message}");
    assert!(
        message.contains("publishing release 2 again continues"),
        "{message}"
    );
    // No release is left half published: latest is where it was, and release 2 is not listed
    // as published.
    assert_eq!(latest(&store), Some(1));
    assert_eq!(
        store.int("SELECT COUNT(*) FROM releases WHERE published_at IS NOT NULL"),
        1
    );

    let journal = Journal::read(journals.path(), 2).unwrap().unwrap();
    assert_eq!(journal.done.len(), *done);
    let journaled: Vec<String> = journal
        .done
        .iter()
        .map(|d| d.key.clone().unwrap_or_else(|| d.path.replace("d1/", "")))
        .collect();
    store.reset();
    let again = publishing(&mut store, 2, f.out.path(), journals.path(), &clock).unwrap();
    assert_eq!(again.journaled, *done);
    assert_eq!(again.uploaded + again.journaled, *total);
    let uploaded = store.uploaded();
    for path in &journaled {
        assert!(!uploaded.contains(path), "{path} was uploaded again");
    }
    assert_eq!(latest(&store), Some(2));
    for file in built
        .manifest
        .files
        .iter()
        .filter(|f| f.target == Target::R2)
    {
        let key = file.key.as_deref().unwrap();
        assert_eq!(sha256(&store.r2[key]), file.sha256, "{key}");
    }
}

#[test]
fn a_release_already_published_is_refused_naming_its_number_and_publication_time() {
    let f = fixture();
    build_next(&f);
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let mut store = MemoryStore::default();
    let first = publishing(&mut store, 1, f.out.path(), journals.path(), &clock).unwrap();
    let r2 = store.r2.clone();
    store.reset();

    let refused = publishing(&mut store, 1, f.out.path(), journals.path(), &clock).unwrap_err();
    let message = refused.to_string();
    assert_eq!(
        message,
        format!(
            "release 1 was already published at {}, and is left as it is; latest points to \
             release 1",
            first.published_at
        )
    );
    assert_eq!(store.uploaded(), Vec::<String>::new());
    assert!(!store.ops.iter().any(|o| matches!(o, Op::Delete(_))));
    assert_eq!(store.r2, r2);
}
