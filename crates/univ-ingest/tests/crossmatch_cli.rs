//! `univ-ingest match --offline` run as the owner runs it, on the recorded answers of four
//! `VizieR` sources and the repository's researched structures (F-13).

mod common;

use common::{copied, workspace};

use std::path::Path;

use univ_ingest::acquire::acquire;
use univ_ingest::source::Access;
use univ_ingest::state::{State, pending};
use univ_ingest::structures::{StructureGeometry, read_all_named};
use wiremock::matchers::query_param;
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Sources each fetched in one query, whose recorded answers the test serves.
const SOURCES: [&str; 4] = [
    "mcconnachie-2012",
    "updated-nearby-galaxies",
    "twomrs",
    "cosmicflows-4",
];

fn matching(definitions: &Path, cache: &Path, state: &Path) -> std::process::Output {
    common::run(
        "match",
        &[
            ("--definitions", definitions),
            ("--cache", cache),
            ("--state", state),
        ],
    )
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
    assert!(
        !log.contains("Z Members "),
        "every member list is placed: {log}"
    );
    // Every member list of the repository's structure files is placed from its own rows: no
    // fixture object carries a member.
    let lists: Vec<usize> = read_all_named(&workspace().join("structures"))
        .unwrap()
        .iter()
        .filter_map(|(_, s)| match &s.geometry {
            Some(StructureGeometry::Members(m)) => Some(m.rows.len()),
            _ => None,
        })
        .collect();
    let (count, members) = (lists.len(), lists.iter().sum::<usize>());
    let counted = [
        format!(
            "Z Step cross-match: members: {count} member lists, {members} members placed from \
             their own cited position and redshift\n"
        ),
        format!(
            "{count} member lists placed, 0 member lists not placed, 0 members resolved by name \
             only\n"
        ),
    ];
    for line in &counted {
        assert!(log.contains(line.as_str()), "{line}in\n{log}");
    }
    for line in [
        "Z Stage cross-match: 5 items\n",
        "Z Step cross-match: matching: 20 entries into 20 objects, 0 candidate links\n",
        "Z Step cross-match: ranking: 16 with a preferred distance\n",
        "Z Step cross-match: ids: release 1, 20 new, 0 replaced\n",
        // M81's only distance here is its 2MRS redshift (cz = -34 km/s, about +48 km/s in the
        // CMB frame), which the peculiar velocity makes about 600% uncertain: it is not placed.
        "Z Step cross-match: positions: 15 placed of 20\n",
        "Z cross-match: 20 entries, 20 objects, 0 merges, 0 candidate links, 20 new ids, 0 ids \
         replaced, 15 placed, 1 not placed (distance too uncertain), 4 not placed (no distance), ",
        "Z Stage cross-match done: 20 entries, ",
        "Z Stopped at cross-match: cross-match only; no release was asked for\n",
    ] {
        assert!(log.contains(line), "{line}in\n{log}");
    }
    assert!(!log.contains('\u{1b}'), "no control codes in a log");
    let prepared = State::read(&pending(state.path())).unwrap();
    assert_eq!(prepared.previous.len(), 20);
}
