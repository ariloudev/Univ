//! The fixture release read from a local D1 as `wrangler dev` keeps it, its catalogue and its
//! object shard each in a file Miniflare names by no order of its own.

use std::path::Path;

use rusqlite::Connection;
use univ_model::release::TABLES;
use univ_model::release::shards::SHARD_TABLES;

use super::super::{load, science};
use super::checks;
use super::fixture::build;

/// What science says of the release, but how long each step took.
fn said(dir: &Path, store: Option<&Path>) -> Vec<String> {
    let rules = dir.join("checks.yaml");
    std::fs::write(&rules, checks()).unwrap();
    let mut args = vec![
        dir.to_string_lossy().into_owned(),
        "--rules".to_owned(),
        rules.to_string_lossy().into_owned(),
    ];
    if let Some(store) = store {
        args.extend(["--store".to_owned(), store.to_string_lossy().into_owned()]);
    }
    let mut out = Vec::new();
    science(dir, &args, &mut out).unwrap();
    String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|l| {
            let l = l.split("; checks took").next().unwrap_or(l);
            l.rsplit_once(" (").map_or(l, |(head, _)| head).to_owned()
        })
        .collect()
}

#[test]
fn a_release_read_from_the_local_d1_is_judged_as_one_read_from_its_builds() {
    let dir = tempfile::tempdir().unwrap();
    build(dir.path());
    let from_builds = said(dir.path(), None);

    // The release as published: its rows in the catalogue and one shard, each a file.
    let whole = dir.path().join("whole.sqlite");
    {
        let (db, _) = load::open(Some(&whole)).unwrap();
        let text = std::fs::read_to_string(dir.path().join("manifest.yaml")).unwrap();
        load::load(&db, dir.path(), &serde_norway::from_str(&text).unwrap()).unwrap();
    }
    let persist = tempfile::tempdir().unwrap();
    let files = persist.path().join(univ_model::release::LOCAL_D1_FILES);
    std::fs::create_dir_all(&files).unwrap();
    let (catalogue, shard) = (files.join("f0.sqlite"), files.join("0f.sqlite"));
    std::fs::copy(&whole, &catalogue).unwrap();
    std::fs::copy(&whole, &shard).unwrap();
    let db = Connection::open(&catalogue).unwrap();
    // Miniflare keeps each database in WAL mode.
    db.execute_batch(
        "PRAGMA journal_mode = WAL;
         INSERT INTO latest (one, release, moved_at) VALUES (1, 1, 't');
         INSERT INTO object_shards (release, id, shard) SELECT 1, id, 0 FROM objects;
         INSERT INTO release_shards (release, shard, objects, estimated_bytes)
           SELECT 1, 0, COUNT(*), 0 FROM objects;",
    )
    .unwrap();
    for table in SHARD_TABLES {
        db.execute(&format!("DELETE FROM {table}"), []).unwrap();
    }
    drop(db);
    // The shard holds the rows of its tables alone.
    let db = Connection::open(&shard).unwrap();
    db.execute_batch("PRAGMA journal_mode = WAL").unwrap();
    for table in TABLES.iter().filter(|t| !SHARD_TABLES.contains(&t.name)) {
        db.execute(&format!("DELETE FROM {}", table.name), [])
            .unwrap();
    }
    drop(db);
    // A database of no release beside them.
    Connection::open(files.join("layers.sqlite"))
        .unwrap()
        .execute_batch("CREATE TABLE layers (id TEXT)")
        .unwrap();

    let from_store = said(dir.path(), Some(persist.path()));
    assert!(
        from_store[0].starts_with("load: release 1 from 2 databases of the local D1"),
        "{}",
        from_store[0]
    );
    // The fixture release is not sharded: the shard's index rows the test added are the only
    // rows its manifest does not list.
    let fingerprints = |lines: &[String]| -> Vec<String> {
        let at = lines
            .iter()
            .position(|l| l.contains(" fingerprints (t)"))
            .unwrap();
        lines[at + 1..]
            .iter()
            .take_while(|l| l.starts_with("          "))
            .cloned()
            .collect()
    };
    let added = fingerprints(&from_store);
    assert_eq!(added.len(), 13, "{added:?}");
    assert!(added.iter().all(|l| {
        let l = l.trim_start();
        l.starts_with("object_shards") || l.starts_with("release_shards")
    }));
    assert_eq!(fingerprints(&from_builds), Vec::<String>::new());
    let but_fingerprints = |lines: &[String]| -> Vec<String> {
        lines[1..]
            .iter()
            .filter(|l| {
                !l.contains(" fingerprints (t)")
                    && !added.contains(l)
                    && !l.starts_with("science: ")
            })
            .cloned()
            .collect()
    };
    assert_eq!(
        but_fingerprints(&from_store),
        but_fingerprints(&from_builds)
    );
}
