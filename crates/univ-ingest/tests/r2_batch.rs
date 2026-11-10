//! The R2 batch script, `scripts/r2-batch.mjs`, against real local R2: objects it writes are
//! the ones `wrangler r2 object get --local --persist-to` reads, and the reverse; a prune of the
//! fixture's release deletes its R2 keys through one run of the script, then its local build;
//! and a batched publish interrupted part way continues from its journal. They need Wrangler
//! and Miniflare from `npm install`, so they are run by hand:
//! `cargo test -p univ-ingest --test r2_batch -- --ignored --nocapture`.
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod release_fixture;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::workspace;
use release_fixture::{build_next, fixture};
use univ_ingest::publish::{
    BUCKET, Latest, PublishError, QueryRow, R2Batch, ReadBack, Store, StoreOptions, Wrangler,
    ingest_dir, prune, publish,
};
use univ_ingest::release::release_dir;
use univ_ingest::tiles::sha256;
use univ_model::release::keys;
use univ_model::release::manifest::Target;
use univ_model::release::shards::Database;

/// Runs `wrangler r2 object …` against the local state `persist`.
fn wrangler_r2(persist: &Path, args: &[&str]) -> Output {
    let root = workspace();
    Command::new(root.join("node_modules/.bin/wrangler"))
        .current_dir(&root)
        .args(["r2", "object"])
        .args(args)
        .arg("--local")
        .arg("--persist-to")
        .arg(persist)
        .arg("--config")
        .arg(root.join("wrangler.toml"))
        .env("WRANGLER_SEND_METRICS", "false")
        .env("NO_COLOR", "1")
        .output()
        .unwrap()
}

/// The object at `key`, as `wrangler r2 object get` reads it, or none.
fn wrangler_get(persist: &Path, key: &str, scratch: &Path) -> Option<Vec<u8>> {
    let file = scratch.join("wrangler-get");
    let _ = std::fs::remove_file(&file);
    let object = format!("{BUCKET}/{key}");
    let run = wrangler_r2(persist, &["get", &object, "--file", file.to_str().unwrap()]);
    if run.status.success() {
        return Some(std::fs::read(&file).unwrap());
    }
    let said = String::from_utf8_lossy(&run.stderr);
    assert!(said.contains("The specified key does not exist"), "{said}");
    None
}

fn file_with(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

#[test]
#[ignore = "runs Wrangler and Miniflare from node_modules: run `npm install` first"]
fn the_batch_script_reads_and_writes_the_storage_wrangler_uses() {
    let (persist, scratch) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (persist, scratch) = (persist.path(), scratch.path());
    let ours: Vec<u8> = (0..=255u8).cycle().take(3 * 1024 * 1024 + 7).collect();
    let theirs = b"written by wrangler r2 object put\n".to_vec();
    let ours_file = file_with(scratch, "ours.bin", &ours);
    let theirs_file = file_with(scratch, "theirs.txt", &theirs);

    // Written by the script and read back by it, then read by Wrangler.
    {
        let mut batch = R2Batch::start(&workspace(), persist).unwrap();
        let back = batch
            .put("t/ours.bin", &ours_file, "application/octet-stream")
            .unwrap();
        assert_eq!(
            back,
            Some(ReadBack {
                bytes: ours.len() as u64,
                sha256: sha256(&ours)
            })
        );
        assert!(!batch.get("t/none", &scratch.join("none")).unwrap());
    }
    assert_eq!(wrangler_get(persist, "t/ours.bin", scratch), Some(ours));

    // Written by Wrangler, read and deleted by the script.
    let object = format!("{BUCKET}/t/theirs.txt");
    let put = wrangler_r2(
        persist,
        &["put", &object, "--file", theirs_file.to_str().unwrap()],
    );
    assert!(
        put.status.success(),
        "{}",
        String::from_utf8_lossy(&put.stderr)
    );
    {
        let mut batch = R2Batch::start(&workspace(), persist).unwrap();
        let back = scratch.join("back.txt");
        assert!(batch.get("t/theirs.txt", &back).unwrap());
        assert_eq!(std::fs::read(&back).unwrap(), theirs);
        let both = ["t/ours.bin".to_owned(), "t/theirs.txt".to_owned()];
        batch.delete(&both).unwrap();
    }
    assert_eq!(wrangler_get(persist, "t/ours.bin", scratch), None);
    assert_eq!(wrangler_get(persist, "t/theirs.txt", scratch), None);

    // A failure names the object and the cause.
    let mut batch = R2Batch::start(&workspace(), persist).unwrap();
    let missing = scratch.join("no-such-file");
    let failed = batch
        .put("t/missing.bin", &missing, "application/octet-stream")
        .unwrap_err();
    assert!(
        failed.starts_with("R2 put of t/missing.bin failed: "),
        "{failed}"
    );
    assert!(failed.contains("no-such-file"), "{failed}");
    // The script goes on after a failure.
    assert!(!batch.get("t/ours.bin", &scratch.join("x")).unwrap());
}

/// Publishes or prunes against `store`, its journals in the local state `persist`.
fn options<'a>(
    f: &'a release_fixture::Fixture,
    journals: &'a Path,
    now: &'a dyn Fn() -> String,
) -> StoreOptions<'a> {
    StoreOptions {
        out: f.out.path(),
        state: f.state.path(),
        journals,
        now,
    }
}

fn wrangler(persist: &Path) -> Wrangler {
    Wrangler::new(&workspace().join("wrangler.toml"), persist).unwrap()
}

/// The single integer a query of the catalogue database gives.
fn int(store: &mut dyn Store, sql: &str) -> i64 {
    let rows = store.query(Database::Catalogue, sql).unwrap();
    rows[0].values().next().unwrap().as_i64().unwrap()
}

#[test]
#[ignore = "runs Wrangler and Miniflare from node_modules: run `npm install` first"]
fn a_prune_deletes_the_release_keys_in_one_script_run_then_its_local_build() {
    let f = fixture();
    let persist = tempfile::tempdir().unwrap();
    let journals = ingest_dir(persist.path());
    let now = univ_ingest::run::now;
    for _ in [1, 2] {
        build_next(&f);
    }
    for n in [1, 2] {
        let mut store = wrangler(persist.path());
        publish(&mut store, n, &options(&f, &journals, &now), &mut |_| {}).unwrap();
    }
    let built = release_dir(f.out.path(), 1);
    let manifest = univ_ingest::release::read_manifest(&built, 1).unwrap().0;
    let keys_of_1: Vec<String> = manifest
        .files
        .iter()
        .filter(|f| f.target == Target::R2)
        .filter_map(|f| f.key.clone())
        .chain([keys::manifest(1)])
        .collect();
    assert!(keys_of_1.len() > 10, "{keys_of_1:?}");

    let mut store = wrangler(persist.path());
    let mut lines = Vec::new();
    let pruned = prune(&mut store, 1, &options(&f, &journals, &now), &mut |l| {
        lines.push(l);
    })
    .unwrap();
    eprintln!("{}", lines.join("\n"));
    assert_eq!(store.r2_batches(), 1, "one run of the script");
    assert_eq!(pruned.files, keys_of_1.len());
    assert_eq!(pruned.latest, Latest(Some(2)));
    assert_eq!(pruned.build.as_deref(), Some(built.as_path()));
    assert!(!built.exists());
    assert!(release_dir(f.out.path(), 2).exists());
    assert!(lines.last().unwrap().contains("local build"), "{lines:?}");
    drop(store);

    let mut batch = R2Batch::start(&workspace(), persist.path()).unwrap();
    let scratch = tempfile::tempdir().unwrap();
    for key in &keys_of_1 {
        assert!(!batch.get(key, &scratch.path().join("x")).unwrap(), "{key}");
    }
    let manifest_2 = keys::manifest(2);
    assert!(batch.get(&manifest_2, &scratch.path().join("m")).unwrap());
    drop(batch);
    let mut store = wrangler(persist.path());
    assert_eq!(
        int(
            &mut store,
            "SELECT COUNT(*) FROM object_shards WHERE release = 1"
        ),
        0
    );

    // Pruned again, with no local build left: not an error.
    let again = prune(&mut store, 1, &options(&f, &journals, &now), &mut |_| {}).unwrap();
    assert!(again.already && again.build.is_none());
}

/// A store whose R2 uploads fail once `left` more have been made.
struct Interrupted<'s> {
    store: &'s mut Wrangler,
    left: usize,
}

impl Store for Interrupted<'_> {
    fn execute_file(&mut self, database: Database, file: &Path) -> Result<(), String> {
        self.store.execute_file(database, file)
    }

    fn query(&mut self, database: Database, sql: &str) -> Result<Vec<QueryRow>, String> {
        self.store.query(database, sql)
    }

    fn bound_shards(&mut self) -> Result<u32, String> {
        self.store.bound_shards()
    }

    fn put(&mut self, key: &str, file: &Path) -> Result<(), String> {
        self.store.put(key, file)
    }

    fn put_read_back(&mut self, key: &str, file: &Path) -> Result<Option<ReadBack>, String> {
        if self.left == 0 {
            return Err(format!("interrupted before {key}"));
        }
        self.left -= 1;
        self.store.put_read_back(key, file)
    }

    fn get(&mut self, key: &str) -> Result<Option<Vec<u8>>, String> {
        self.store.get(key)
    }

    fn delete(&mut self, key: &str) -> Result<(), String> {
        self.store.delete(key)
    }

    fn delete_keys(&mut self, keys: &[String]) -> Result<(), String> {
        self.store.delete_keys(keys)
    }
}

#[test]
#[ignore = "runs Wrangler and Miniflare from node_modules: run `npm install` first"]
fn an_interrupted_batched_publish_continues_from_its_journal() {
    let f = fixture();
    let persist = tempfile::tempdir().unwrap();
    let journals = ingest_dir(persist.path());
    let now = univ_ingest::run::now;
    let built = build_next(&f);

    let mut store = wrangler(persist.path());
    let mut interrupted = Interrupted {
        store: &mut store,
        left: 5,
    };
    let error = publish(
        &mut interrupted,
        1,
        &options(&f, &journals, &now),
        &mut |_| {},
    )
    .unwrap_err();
    let PublishError::Interrupted { done, latest, .. } = &error else {
        panic!("{error}")
    };
    assert_eq!(*latest, Latest(None));
    assert!(error.to_string().contains("interrupted before"), "{error}");
    let done = *done;
    assert!(done > 5, "the schema and five R2 files: {done}");
    drop(store);

    let mut store = wrangler(persist.path());
    let again = publish(&mut store, 1, &options(&f, &journals, &now), &mut |_| {}).unwrap();
    assert_eq!(again.journaled, done);
    assert_eq!(
        again.uploaded + again.journaled,
        built.manifest.files.len() + 1
    );
    assert_eq!(store.r2_batches(), 1, "one run of the script");
    assert_eq!(int(&mut store, "SELECT release FROM latest"), 1);
    for file in built
        .manifest
        .files
        .iter()
        .filter(|f| f.target == Target::R2)
    {
        let key = file.key.as_deref().unwrap();
        let bytes = store.get(key).unwrap().unwrap();
        assert_eq!(sha256(&bytes), file.sha256, "{key}");
    }
}
