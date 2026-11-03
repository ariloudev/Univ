//! `univ-ingest fetch` run as the owner runs it, writing to a pipe (F-13).
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;

use std::process::Command;

use common::rig;
use univ_ingest::acquire::acquire;
use wiremock::matchers::query_param;
use wiremock::{Mock, MockServer, ResponseTemplate};

fn fetch(definitions: &std::path::Path, cache: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_univ-ingest"))
        .args(["fetch", "--offline", "--definitions"])
        .arg(definitions)
        .arg("--cache")
        .arg(cache)
        .arg("--repository")
        .arg(definitions)
        .output()
        .unwrap()
}

#[tokio::test]
async fn an_offline_fetch_prints_plain_lines_and_a_summary_and_fails_on_a_missing_chunk() {
    let server = MockServer::start().await;
    Mock::given(query_param("format", "csv"))
        .respond_with(ResponseTemplate::new(200).set_body_string("#Table1\nra,dec\n1,2\n"))
        .mount(&server)
        .await;
    let definitions = tempfile::tempdir().unwrap();
    let access = "  method: skyserver_sql\n  query: \"SELECT ra, dec FROM T WHERE (id % {chunks}) = {chunk}\"\n  chunks: 2";
    let definition = common::definition(&definitions, access, &server.uri());
    let cache = tempfile::tempdir().unwrap();
    acquire(
        &rig(false).fetcher,
        &definition,
        cache.path(),
        definitions.path(),
    )
    .await
    .unwrap();

    let run = fetch(definitions.path(), cache.path());
    let log = String::from_utf8(run.stderr).unwrap();
    assert!(run.status.success(), "{log}");
    assert!(log.contains("Z Stage fetch: 2 items\n"), "{log}");
    assert!(
        log.contains("Z Hit:1 test-galaxies chunk 1 of 2\n")
            && log.contains("Z Hit:2 test-galaxies chunk 2 of 2\n"),
        "{log}"
    );
    assert!(
        log.contains("Z test-galaxies: not normalized in this run\n")
            && log.contains("Z Stopped at fetch: fetch only; no release was asked for\n"),
        "{log}"
    );
    assert!(!log.contains('\u{1b}'), "no control codes in a log");

    let empty = tempfile::tempdir().unwrap();
    let run = fetch(definitions.path(), empty.path());
    let log = String::from_utf8(run.stderr).unwrap();
    assert!(!run.status.success());
    assert!(
        log.contains(
            "Z test-galaxies: failed: chunk 1 of 2 is not on disk, and an offline run fetches nothing\n"
        ),
        "{log}"
    );
    assert!(
        log.contains("Z Stopped at fetch: 1 of 1 sources failed\n"),
        "{log}"
    );
}
