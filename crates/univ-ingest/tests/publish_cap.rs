//! The CON-3 cap a publish checks before anything is uploaded, against an in-memory D1 and R2:
//! the rows each database holds already that the release does not keep, at the release's bytes
//! per row, with its own estimate; neither the rows it keeps nor, on a resumed publish, those
//! it loaded already counted twice.

#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod memory_store;
mod release_fixture;

use memory_store::{Clock, MemoryStore, Op, publishing};
use release_fixture::{build_next, fixture, read_manifest};
use univ_ingest::publish::PublishError;
use univ_model::release::D1_CAP_BYTES;
use univ_model::release::shards::Database;

/// What `latest` points to, by D1.
fn latest(store: &MemoryStore) -> Option<i64> {
    store
        .db
        .query_row("SELECT release FROM latest", [], |r| r.get(0))
        .ok()
}

/// Edits release `dir`'s manifest as if its rows of `table` in `database` took `per_row` bytes
/// each, and, when `written`, as if it wrote every one of them again rather than keeping any:
/// the cap then counts the rows D1 holds of it, that it does not keep, at that size.
fn as_if_rows_were(
    dir: &std::path::Path,
    (database, table): (Database, &str),
    per_row: u64,
    written: bool,
) {
    let mut m = read_manifest(dir);
    let estimate = match database {
        Database::Catalogue => &mut m.d1,
        Database::Shard(i) => &mut m.shards[i as usize].d1,
    };
    let t = estimate
        .tables
        .iter_mut()
        .find(|t| t.table == table)
        .unwrap();
    if written {
        t.added = Some(t.rows);
    }
    t.estimated_bytes = t.rows * per_row;
    univ_ingest::release::write_manifest(dir, &m).unwrap();
}

#[test]
fn a_release_beyond_the_cap_with_those_not_pruned_is_refused_with_both_estimates() {
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
    let recorded = store.int("SELECT d1_estimated_bytes FROM releases WHERE number = 1");
    let built = build_next(&f);
    assert_eq!(
        u64::try_from(recorded).unwrap(),
        read_manifest(&univ_ingest::release::release_dir(f.out.path(), 1))
            .d1
            .estimated_bytes
    );
    // Release 2 as if it wrote its terms again, each so large that release 1's take the cap.
    let terms = store.int("SELECT COUNT(*) FROM terms").unsigned_abs();
    let per_row = D1_CAP_BYTES / terms;
    as_if_rows_were(&built.dir, (Database::Catalogue, "terms"), per_row, true);
    store.reset();
    let refused = publishing(
        &mut store,
        2,
        f.out.path(),
        f.state.path(),
        journals.path(),
        &clock,
    )
    .unwrap_err();
    let this = built.manifest.d1.estimated_bytes;
    let PublishError::OverCap {
        number: 2,
        database: Database::Catalogue,
        this: refused_this,
        others,
        releases: 1,
    } = &refused
    else {
        panic!("{refused}");
    };
    assert_eq!(*refused_this, this);
    // Release 1's terms, which release 2 does not keep, and the rest it does not keep.
    assert!(*others >= terms * per_row, "{others}");
    let message = refused.to_string();
    assert!(
        message.contains(&format!(
            "its rows in the catalogue database are estimated at {this} bytes"
        )) && message.contains(&format!("1 release(s) not pruned, at {others} bytes"))
            && message.contains("beyond the 10 GB cap of 10000000000 bytes"),
        "{message}"
    );
    assert_eq!(store.uploaded(), Vec::<String>::new());
    assert_eq!(latest(&store), Some(1));
}

#[test]
fn a_release_beyond_the_cap_in_one_object_shard_is_refused_naming_the_shard() {
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
    let built = build_next(&f);
    // Release 2 as if it wrote shard 1's objects again, so large that release 1's take the cap.
    let objects =
        memory_store::int(&store.shards[&1], "SELECT COUNT(*) FROM objects").unsigned_abs();
    let per_row = D1_CAP_BYTES / objects + 1;
    as_if_rows_were(&built.dir, (Database::Shard(1), "objects"), per_row, true);
    store.reset();
    let refused = publishing(
        &mut store,
        2,
        f.out.path(),
        f.state.path(),
        journals.path(),
        &clock,
    )
    .unwrap_err();
    let PublishError::OverCap {
        number: 2,
        database: Database::Shard(1),
        this,
        others,
        releases: 1,
    } = &refused
    else {
        panic!("{refused}");
    };
    assert_eq!(*this, built.manifest.shards[1].d1.estimated_bytes);
    assert!(*others >= objects * per_row, "{others}");
    assert!(
        refused
            .to_string()
            .contains("its rows in object shard 1 are estimated at")
    );
    assert_eq!(store.uploaded(), Vec::<String>::new());
}

#[test]
fn a_release_is_not_charged_again_for_the_rows_it_keeps() {
    let f = fixture();
    build_next(&f);
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let mut store = MemoryStore::default();
    let (out, state) = (f.out.path(), f.state.path());
    publishing(&mut store, 1, out, state, journals.path(), &clock).unwrap();
    // Release 2 keeps every object of shard 1: were they counted again at this size, the
    // shard would pass the cap.
    let built = build_next(&f);
    let objects =
        memory_store::int(&store.shards[&1], "SELECT COUNT(*) FROM objects").unsigned_abs();
    let per_row = D1_CAP_BYTES / objects + 1;
    as_if_rows_were(&built.dir, (Database::Shard(1), "objects"), per_row, false);
    publishing(&mut store, 2, out, state, journals.path(), &clock).unwrap();
    assert_eq!(latest(&store), Some(2));
}

#[test]
fn a_resumed_publish_is_not_charged_for_the_rows_it_loaded_already() {
    const SOURCE: &str = "mcconnachie-2012";
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
    build_next(&f);
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let mut store = MemoryStore::default();
    let (out, state) = (f.out.path(), f.state.path());
    publishing(&mut store, 1, out, state, journals.path(), &clock).unwrap();
    // Release 2 adds a source and with it names, at a size that takes the whole cap.
    f.definitions.insert(at, definition);
    f.normalized.push(normalized);
    let built = build_next(&f);
    let names = built
        .manifest
        .d1
        .tables
        .iter()
        .find(|t| t.table == "names")
        .unwrap();
    let added = names.added.unwrap();
    assert!(added > 0);
    let per_row = D1_CAP_BYTES / added + 1;
    as_if_rows_were(&built.dir, (Database::Catalogue, "names"), per_row, false);
    // The first run stops once the catalogue is loaded, its names among it.
    store.reset();
    store.fail_at =
        Some(|op| matches!(op, Op::Execute(p) if p.contains("shard-") && !p.contains("schema")));
    publishing(&mut store, 2, out, state, journals.path(), &clock).unwrap_err();
    assert!(store.int("SELECT COUNT(*) FROM names WHERE release = 2") > 0);
    // Counted twice, its own names would pass the cap; once, they do not.
    store.reset();
    publishing(&mut store, 2, out, state, journals.path(), &clock).unwrap();
    assert_eq!(latest(&store), Some(2));
}
