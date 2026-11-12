//! `univ-ingest build --offline` run as the owner runs it, on the recorded answers of five
//! `VizieR` sources and the repository's researched structures: the build stage's lines, the
//! summary naming the release (AC-117's built half), the release on disk, and a second build
//! numbered after it.

mod common;

use common::{copied, workspace};

use std::path::Path;

use univ_ingest::acquire::acquire;
use univ_ingest::lock::StateLock;
use univ_ingest::release::release_dir;
use univ_ingest::source::Access;
use univ_ingest::state::{State, pending};
use univ_ingest::structures::read_all_named;
use univ_ingest::tiles::sha256;
use univ_model::release::{ReleaseManifest, keys};
use wiremock::matchers::query_param;
use wiremock::{Mock, MockServer, ResponseTemplate};

const SOURCES: [&str; 5] = [
    "hipparcos-2007",
    "mcconnachie-2012",
    "updated-nearby-galaxies",
    "twomrs",
    "cosmicflows-4",
];

fn building(definitions: &Path, cache: &Path, state: &Path, out: &Path) -> std::process::Output {
    common::run(
        "build",
        &[
            ("--definitions", definitions),
            ("--cache", cache),
            ("--state", state),
            ("--out", out),
        ],
    )
}

#[tokio::test]
async fn an_offline_build_writes_a_checked_release_and_names_it_in_the_summary() {
    let server = MockServer::start().await;
    let definitions = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    for id in SOURCES {
        let definition = common::definition_of(id);
        let Access::VizierTsv { table, .. } = &definition.access else {
            unreachable!()
        };
        let body = std::fs::read(common::fixture_of(&definition)).unwrap();
        Mock::given(query_param("-source", table.as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
            .mount(&server)
            .await;
    }
    for id in SOURCES {
        let definition = copied(id, definitions.path(), &server.uri());
        let fetcher = &common::rig(false).fetcher;
        acquire(fetcher, &definition, cache.path(), &workspace())
            .await
            .unwrap();
    }
    std::fs::copy(
        workspace().join("sources/researched-structures.yaml"),
        definitions.path().join("researched-structures.yaml"),
    )
    .unwrap();

    let (state, out) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let run = building(definitions.path(), cache.path(), state.path(), out.path());
    let log = String::from_utf8(run.stderr).unwrap();
    assert!(run.status.success(), "{log}");
    let dir = release_dir(out.path(), 1);
    // The repository's structure files: those whose papers place them nowhere are not indexed.
    let structures = read_all_named(&workspace().join("structures")).unwrap();
    let unplaced: Vec<&str> = structures
        .iter()
        .filter(|(_, s)| s.unplaced.is_some())
        .map(|(id, _)| id.as_str())
        .collect();
    let (all, first) = (structures.len(), unplaced[0]);
    for line in [
        "Z Stage tiles: 4 items\n".to_owned(),
        // The build's phases, each a step of its six.
        "Z Stage build: 6 items\n".to_owned(),
        format!(
            "Z Step build: rules: {} files\n",
            univ_ingest::crossmatch::RULES.len()
        ),
        "Z Step build: tiles: 3 tiles and 4 manifests\n".to_owned(),
        "Z Step build: volume grids: 0 grids in 0 files\n".to_owned(),
        "Z Step build: checks: every check passed\n".to_owned(),
        "Z Step build: manifest: 39 files checked against their SHA-256\n".to_owned(),
        "Z Stage build done: ".to_owned(),
        format!(
            "Z Step build: release 1: every check passed; 39 files listed with their SHA-256 in \
             {}\n",
            dir.join("manifest.yaml").display()
        ),
        format!(
            "Z Step build: cell index: 16 placed objects, {} of {all} structures with a bounding \
             box\n",
            all - unplaced.len()
        ),
        format!(
            "Z Step build: cell index: {} structures not indexed: {first} (it publishes no \
             position or geometry); ",
            unplaced.len()
        ),
        "Z Step build: provisional redistribution review, judged again before any public \
         deployment: cosmicflows-4, hipparcos-2007, mcconnachie-2012, researched-structures, \
         twomrs, updated-nearby-galaxies\n"
            .to_owned(),
        "Built release 1\n".to_owned(),
    ] {
        assert!(log.contains(&line), "{line}in\n{log}");
    }
    assert!(log.contains("Z Step build: D1 rows: "), "{log}");
    assert!(log.contains("Z Step build: D1: "), "{log}");
    assert!(log.contains(" of the 10 GB cap ("), "{log}");
    assert!(!dir.with_file_name(".building-1").exists());

    let text = std::fs::read_to_string(dir.join(keys::MANIFEST)).unwrap();
    let manifest: ReleaseManifest = serde_norway::from_str(&text).unwrap();
    assert_eq!(manifest.release, 1);
    assert_eq!(manifest.counts.objects, 25);
    for file in &manifest.files {
        let bytes = std::fs::read(dir.join(&file.path)).unwrap();
        assert_eq!(file.sha256, sha256(&bytes), "{}", file.path);
    }
    assert!(!pending(state.path()).exists());
    assert_eq!(
        State::read(state.path()).unwrap().ledger.releases().len(),
        1
    );

    // The same sources again: the next release, every object keeping its id.
    let again = building(definitions.path(), cache.path(), state.path(), out.path());
    let log = String::from_utf8(again.stderr).unwrap();
    assert!(again.status.success(), "{log}");
    assert!(log.contains("Built release 2\n"), "{log}");
    assert!(
        log.contains("Z Step cross-match: ids: release 2, 0 new, 0 replaced\n"),
        "{log}"
    );
    assert!(release_dir(out.path(), 2).join(keys::MANIFEST).is_file());
}

#[tokio::test]
async fn a_build_in_which_a_source_failed_names_it_and_uses_no_number() {
    let server = MockServer::start().await;
    let definitions = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let (fetched, missing) = ("mcconnachie-2012", "twomrs");
    for id in [fetched, missing] {
        let definition = common::definition_of(id);
        let Access::VizierTsv { table, .. } = &definition.access else {
            unreachable!()
        };
        let body = std::fs::read(common::fixture_of(&definition)).unwrap();
        Mock::given(query_param("-source", table.as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
            .mount(&server)
            .await;
        copied(id, definitions.path(), &server.uri());
    }
    // Only one of the two is on disk, and the build runs offline.
    let definition = common::definition_of(fetched);
    let definition = copied(definition.id.as_str(), definitions.path(), &server.uri());
    acquire(
        &common::rig(false).fetcher,
        &definition,
        cache.path(),
        &workspace(),
    )
    .await
    .unwrap();

    let (state, out) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let run = building(definitions.path(), cache.path(), state.path(), out.path());
    let log = String::from_utf8(run.stderr).unwrap();
    assert!(!run.status.success(), "{log}");
    assert!(
        log.contains("Stopped at build: no release was built, since source(s) failed: twomrs\n"),
        "{log}"
    );
    assert!(!out.path().join("releases").exists());
    assert_eq!(
        State::read(state.path()).unwrap().ledger.releases().len(),
        0
    );
}

#[test]
fn a_second_build_on_one_state_directory_is_refused_by_its_lock() {
    let (definitions, cache) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (state, out) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    // A build already running holds the lock.
    let running = StateLock::take(state.path(), 4242, "2026-11-03T12:00:00Z").unwrap();
    let run = building(definitions.path(), cache.path(), state.path(), out.path());
    let log = String::from_utf8(run.stderr).unwrap();
    assert!(!run.status.success(), "{log}");
    assert!(
        log.contains("the state directory is locked by process 4242 since 2026-11-03T12:00:00Z"),
        "{log}"
    );
    assert!(
        log.contains(&format!("remove {}", running.file().display())),
        "{log}"
    );
    assert!(running.file().exists(), "the lock is not taken over");
    assert!(!out.path().join("releases").exists());
    drop(running);
    // With the lock released, the run goes on (and here stops for want of sources).
    let run = building(definitions.path(), cache.path(), state.path(), out.path());
    let log = String::from_utf8(run.stderr).unwrap();
    assert!(!log.contains("locked"), "{log}");
    assert!(!state.path().join(univ_ingest::lock::FILE).exists());
}
