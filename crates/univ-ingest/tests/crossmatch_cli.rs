//! `univ-ingest match --offline` run as the owner runs it, on the recorded answers of four
//! `VizieR` sources and the repository's researched structures (F-13).
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a failed unwrap or a panic is the test failing"
)]

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use univ_ingest::acquire::acquire;
use univ_ingest::source::{Access, SourceDefinition};
use univ_ingest::state::{State, pending};
use wiremock::matchers::query_param;
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Sources each fetched in one query, whose recorded answers the test serves.
const SOURCES: [&str; 4] = [
    "mcconnachie-2012",
    "updated-nearby-galaxies",
    "twomrs",
    "cosmicflows-4",
];

fn workspace() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The repository's definition of `id`, written to `dir` with `mirror` as its one mirror and
/// the rows of its recorded answer as the rows it must give.
fn copied(id: &str, dir: &Path, mirror: &str) -> SourceDefinition {
    let definition = common::definition_of(id);
    let rows = common::count_rows(&definition);
    let original = workspace().join("sources").join(format!("{id}.yaml"));
    let text = std::fs::read_to_string(original).unwrap();
    let Access::VizierTsv {
        rows: Some(all), ..
    } = &definition.access
    else {
        panic!("{id} is not one VizieR query")
    };
    let text = text
        .replace(&format!("  rows: {all}\n"), &format!("  rows: {rows}\n"))
        .replace(
            "\"https://vizier.cds.unistra.fr/viz-bin/asu-tsv\"",
            &format!("\"{mirror}/viz-bin/asu-tsv\""),
        );
    let path = dir.join(format!("{id}.yaml"));
    std::fs::write(&path, text).unwrap();
    SourceDefinition::read(&path).unwrap()
}

fn matching(definitions: &Path, cache: &Path, state: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_univ-ingest"))
        .args(["match", "--offline", "--definitions"])
        .arg(definitions)
        .arg("--cache")
        .arg(cache)
        .arg("--repository")
        .arg(workspace())
        .arg("--state")
        .arg(state)
        .output()
        .unwrap()
}

#[tokio::test]
async fn an_offline_match_of_recorded_answers_prints_the_stage_and_its_counts() {
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
    let structures = workspace().join("sources/researched-structures.yaml");
    std::fs::copy(
        structures,
        definitions.path().join("researched-structures.yaml"),
    )
    .unwrap();

    let state = tempfile::tempdir().unwrap();
    let run = matching(definitions.path(), cache.path(), state.path());
    let log = String::from_utf8(run.stderr).unwrap();
    assert!(run.status.success(), "{log}");
    for line in [
        "Z Stage cross-match: 5 items\n",
        "Z Step cross-match: matching: 19 entries into 18 objects, 0 candidate links\n",
        "Z Step cross-match: ranking: 15 with a preferred distance\n",
        "Z Step cross-match: ids: release 1, 18 new, 0 replaced\n",
        "Z Step cross-match: positions: 15 placed of 18\n",
        "Z Members Clowes-Campusano LQG: 34 of 34 cannot be placed (34 name no matched object, 0 \
         an unplaced one); it publishes no position, so it is not placed\n",
        "Z Step cross-match: members: 6 member lists\n",
        "Z cross-match: 19 entries, 18 objects, 1 merges, 0 candidate links, 18 new ids, 0 ids \
         replaced, 15 placed, 3 not placed (no distance), 0 member lists placed, 6 member lists \
         not placed\n",
        "Z Stopped at cross-match: cross-match only; no release was asked for\n",
    ] {
        assert!(log.contains(line), "{line}in\n{log}");
    }
    assert!(!log.contains('\u{1b}'), "no control codes in a log");
    let prepared = State::read(&pending(state.path())).unwrap();
    assert_eq!(prepared.previous.len(), 19);
}
