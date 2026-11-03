//! A source is acquired whole or not at all: its rows must match its count (F-2).

mod common;

use common::{rig, source};
use univ_ingest::acquire::{AcquireError, acquire};
use univ_ingest::provenance::Provenance;
use univ_ingest::source::SourceDefinition;
use wiremock::matchers::query_param;
use wiremock::{Mock, MockServer, ResponseTemplate};

const QUERY: &str = "SELECT ra, dec FROM T WHERE (id % {chunks}) = {chunk}";
const COUNT: &str = "SELECT COUNT(*) FROM T";

fn definition(dir: &tempfile::TempDir, mirror: &str) -> SourceDefinition {
    let access =
        format!("  method: skyserver_sql\n  query: \"{QUERY}\"\n  chunks: 2\n  count: \"{COUNT}\"");
    common::definition(dir, &access, mirror)
}

async fn serve(server: &MockServer, count: u64) {
    let answers = [
        (
            QUERY.replace("{chunks}", "2").replace("{chunk}", "0"),
            "#Table1\nra,dec\n1,2\n3,4\n".to_owned(),
        ),
        (
            QUERY.replace("{chunks}", "2").replace("{chunk}", "1"),
            "#Table1\nra,dec\n5,6\n".to_owned(),
        ),
        (COUNT.to_owned(), format!("#Table1\nColumn1\n{count}\n")),
    ];
    for (sql, body) in answers {
        Mock::given(query_param("cmd", sql.as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(server)
            .await;
    }
}

#[tokio::test]
async fn a_source_whose_rows_match_its_count_is_acquired_and_every_payload_recorded() {
    let server = MockServer::start().await;
    serve(&server, 3).await;
    let defs = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let acquired = acquire(
        &rig(false).fetcher,
        &definition(&defs, &server.uri()),
        cache.path(),
    )
    .await
    .unwrap();
    assert_eq!(acquired.files.len(), 2);
    let provenance = Provenance::read(&cache.path().join("test-galaxies")).unwrap();
    let items: Vec<&str> = provenance
        .payloads
        .iter()
        .map(|p| p.item.as_str())
        .collect();
    assert_eq!(items, ["chunk 1 of 2", "chunk 2 of 2", "row count"]);
    assert_eq!(provenance.source, Some(source()));
}

#[tokio::test]
async fn a_source_whose_chunks_fall_short_of_its_count_fails_and_keeps_its_chunks() {
    let server = MockServer::start().await;
    serve(&server, 500_000).await;
    let defs = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let error = acquire(
        &rig(false).fetcher,
        &definition(&defs, &server.uri()),
        cache.path(),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(
            error,
            AcquireError::Incomplete {
                expected: 500_000,
                found: 3,
                ..
            }
        ),
        "{error}"
    );
    assert!(error.to_string().contains("no release may hold it"));
    let kept = std::fs::read_dir(cache.path().join("test-galaxies"))
        .unwrap()
        .count();
    assert!(kept >= 3, "the chunks and count stay on disk");
}

#[tokio::test]
async fn an_answer_that_lacks_a_column_is_not_kept_as_a_chunk() {
    let server = MockServer::start().await;
    Mock::given(query_param("format", "csv"))
        .respond_with(ResponseTemplate::new(200).set_body_string("#Table1\nra\n1\n"))
        .mount(&server)
        .await;
    let defs = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let error = acquire(
        &rig(false).fetcher,
        &definition(&defs, &server.uri()),
        cache.path(),
    )
    .await
    .unwrap_err();
    assert!(
        error.to_string().contains("lacks the columns dec"),
        "{error}"
    );
}
