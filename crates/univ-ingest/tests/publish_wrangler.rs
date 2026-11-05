//! Publishing and pruning against real local D1 and R2: the fixture's release built, published
//! with `univ-ingest publish` through Wrangler into a temporary `--persist-to` directory,
//! `latest` read back, a second release published, the first pruned, and its ids resolved
//! through the ledger. It needs Wrangler from `npm install`, so it is run by hand:
//! `cargo test -p univ-ingest --test publish_wrangler -- --ignored --nocapture`.
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod release_fixture;

use common::workspace;
use release_fixture::{build_next, fixture, run_ok, univ_ingest};
use univ_ingest::publish::{Store, Wrangler};
use univ_model::release::keys;
use univ_model::release::shards::Database;

/// The single integer a query of `database` gives.
fn int_in(store: &mut Wrangler, database: Database, sql: &str) -> i64 {
    let rows = store.query(database, sql).unwrap();
    let value = rows[0].values().next().unwrap().as_i64().unwrap();
    eprintln!("D1> {sql}\n    = {value}");
    value
}

/// The single integer a query of the catalogue database gives.
fn int(store: &mut Wrangler, sql: &str) -> i64 {
    int_in(store, Database::Catalogue, sql)
}

/// The sum over the release's `shards` of the single integer a query of each gives.
fn in_shards(store: &mut Wrangler, shards: u32, sql: &str) -> i64 {
    (0..shards)
        .map(|i| int_in(store, Database::Shard(i), sql))
        .sum()
}

#[test]
#[ignore = "runs Wrangler from node_modules: run `npm install` first, then this test by hand"]
fn the_fixture_release_is_published_to_local_d1_and_r2_and_pruned() {
    let f = fixture();
    let persist = tempfile::tempdir().unwrap();
    let (out, state) = (f.out.path(), f.state.path());
    let built = build_next(&f);
    eprintln!("built release 1: {} files", built.manifest.files.len());

    let log = run_ok("publish", 1, out, state, persist.path());
    assert!(log.contains("Published release 1 at "), "{log}");
    let mut store = Wrangler::new(&workspace().join("wrangler.toml"), persist.path()).unwrap();
    assert_eq!(int(&mut store, "SELECT release FROM latest"), 1);
    let shards = u32::try_from(built.manifest.shards.len()).unwrap();
    assert!(shards >= 2, "the fixture spans object shards");
    assert_eq!(
        in_shards(
            &mut store,
            shards,
            "SELECT COUNT(*) FROM objects WHERE release = 1"
        ),
        25
    );
    assert_eq!(
        int(
            &mut store,
            "SELECT COUNT(*) FROM object_shards WHERE release = 1"
        ),
        25
    );
    let manifest = store.get(&keys::key(1, keys::MANIFEST)).unwrap().unwrap();
    assert_eq!(
        manifest,
        std::fs::read(built.dir.join(keys::MANIFEST)).unwrap()
    );

    // A second publish of release 1 is refused, naming its publication time.
    let again = univ_ingest("publish", 1, out, state, persist.path());
    let refused = String::from_utf8(again.stderr).unwrap();
    eprintln!("$ univ-ingest publish 1\n{refused}");
    assert!(!again.status.success());
    assert!(
        refused.contains("release 1 was already published at "),
        "{refused}"
    );

    build_next(&f);
    run_ok("publish", 2, out, state, persist.path());
    assert_eq!(int(&mut store, "SELECT release FROM latest"), 2);

    // The release latest points to is refused; the one before it is pruned.
    let refused = univ_ingest("prune", 2, out, state, persist.path());
    let said = String::from_utf8(refused.stderr).unwrap();
    eprintln!("$ univ-ingest prune 2\n{said}");
    assert!(said.contains("latest points to release 2"), "{said}");
    let ids: Vec<i64> = store
        .query(
            Database::Catalogue,
            "SELECT id FROM object_shards WHERE release = 1",
        )
        .unwrap()
        .iter()
        .map(|r| r["id"].as_i64().unwrap())
        .collect();
    let log = run_ok("prune", 1, out, state, persist.path());
    assert!(log.contains("Release 1 pruned at "), "{log}");
    assert_eq!(
        in_shards(
            &mut store,
            shards,
            "SELECT COUNT(*) FROM objects WHERE release = 1"
        ),
        0
    );
    assert_eq!(
        int(
            &mut store,
            "SELECT COUNT(*) FROM object_shards WHERE release = 1"
        ),
        0
    );
    assert_eq!(
        int(
            &mut store,
            "SELECT COUNT(*) FROM releases WHERE number = 1 AND pruned_at IS NOT NULL"
        ),
        1
    );
    assert_eq!(store.get(&keys::key(1, keys::MANIFEST)).unwrap(), None);
    // Every id of release 1 resolves: issued by a release the ledger records, and in release 2
    // or replaced.
    let list = ids
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    let resolved = int(
        &mut store,
        &format!(
            "SELECT COUNT(*) FROM ids_issued i JOIN (SELECT value AS id FROM json_each('[{list}]')) \
             x ON x.id BETWEEN i.first AND i.last WHERE x.id IN (SELECT id FROM object_shards \
             WHERE release = 2) OR x.id IN (SELECT id FROM ids_replaced)"
        ),
    );
    assert_eq!(resolved, i64::try_from(ids.len()).unwrap());
}
