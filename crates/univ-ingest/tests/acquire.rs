//! A source is acquired whole or not at all: its rows must match its count (F-2).

mod common;

use std::path::Path;
use std::time::Duration;

use common::{rig, source};
use sha2::{Digest, Sha256};
use univ_ingest::acquire::{AcquireError, Acquired, acquire};
use univ_ingest::display::RunState;
use univ_ingest::fetch::{Clock, Payload, hex};
use univ_ingest::provenance::Provenance;
use univ_ingest::source::SourceDefinition;
use wiremock::matchers::{method, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const QUERY: &str = "SELECT ra, dec FROM T WHERE (id % {chunks}) = {chunk}";
const COUNT: &str = "SELECT COUNT(*) FROM T";

fn definition(dir: &tempfile::TempDir, mirror: &str) -> SourceDefinition {
    let access =
        format!("  method: skyserver_sql\n  query: \"{QUERY}\"\n  chunks: 2\n  count: \"{COUNT}\"");
    common::definition(dir, &access, mirror)
}

/// Acquires `definition` with a fresh fetcher, from a repository with nothing in it.
async fn run(definition: &SourceDefinition, cache: &Path) -> Result<Acquired, AcquireError> {
    acquire(
        &rig(false).fetcher,
        definition,
        cache,
        Path::new("/nonexistent"),
    )
    .await
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
        Path::new("/nonexistent"),
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
async fn a_source_whose_chunks_fall_short_of_its_count_fails_and_drops_them_to_fetch_again() {
    let server = MockServer::start().await;
    serve(&server, 500_000).await;
    let defs = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let error = acquire(
        &rig(false).fetcher,
        &definition(&defs, &server.uri()),
        cache.path(),
        Path::new("/nonexistent"),
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
    let left: Vec<String> = std::fs::read_dir(cache.path().join("test-galaxies"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("chunk-"))
        .collect();
    assert_eq!(
        left,
        Vec::<String>::new(),
        "the next run fetches them again"
    );
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
        Path::new("/nonexistent"),
    )
    .await
    .unwrap_err();
    assert!(
        error.to_string().contains("lacks the columns dec"),
        "{error}"
    );
}

fn chunk_files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("chunk-"))
        .collect();
    names.sort();
    names
}

#[tokio::test]
async fn chunks_on_disk_are_counted_from_their_records_and_never_parsed_again() {
    let server = MockServer::start().await;
    serve(&server, 3).await;
    let defs = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let d = definition(&defs, &server.uri());
    let acquired = run(&d, cache.path()).await.unwrap();
    // A chunk that would no longer parse is still counted by its record's rows.
    std::fs::write(&acquired.files[0], "not a table").unwrap();
    server.reset().await;
    run(&d, cache.path()).await.unwrap();
    assert_eq!(server.received_requests().await.unwrap().len(), 0);
}

#[tokio::test]
async fn a_chunk_on_disk_without_a_record_is_fetched_again() {
    let server = MockServer::start().await;
    serve(&server, 3).await;
    let defs = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let d = definition(&defs, &server.uri());
    let acquired = run(&d, cache.path()).await.unwrap();
    let dir = cache.path().join("test-galaxies");
    std::fs::remove_file(dir.join("provenance.yaml")).unwrap();
    std::fs::write(&acquired.files[0], "unrecorded").unwrap();
    server.reset().await;
    serve(&server, 3).await;
    run(&d, cache.path()).await.unwrap();
    assert_eq!(server.received_requests().await.unwrap().len(), 3);
    assert_eq!(
        std::fs::read_to_string(&acquired.files[0]).unwrap(),
        "#Table1\nra,dec\n1,2\n3,4\n"
    );
    assert_eq!(Provenance::read(&dir).unwrap().payloads.len(), 3);
}

#[tokio::test]
async fn chunks_the_definition_no_longer_asks_for_are_removed_and_other_files_kept() {
    let server = MockServer::start().await;
    serve(&server, 3).await;
    let defs = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let dir = cache.path().join("test-galaxies");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("chunk-0000-deadbeef.csv"), "old split\n").unwrap();
    std::fs::write(dir.join("notes.txt"), "mine\n").unwrap();
    let acquired = run(&definition(&defs, &server.uri()), cache.path())
        .await
        .unwrap();
    let mut wanted: Vec<String> = acquired
        .files
        .iter()
        .map(|f| f.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    wanted.push(
        chunk_files(&dir)
            .into_iter()
            .find(|n| n.starts_with("chunk-count"))
            .unwrap(),
    );
    wanted.sort();
    assert_eq!(chunk_files(&dir), wanted);
    assert!(dir.join("notes.txt").exists() && dir.join("provenance.yaml").exists());
}

fn repository_definition(defs: &tempfile::TempDir) -> SourceDefinition {
    common::definition(
        defs,
        "  method: repository\n  path: structures/walls.yaml",
        "https://example.org",
    )
}

#[tokio::test]
async fn a_repository_source_is_found_under_the_repository_root() {
    let defs = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let repository = tempfile::tempdir().unwrap();
    let d = repository_definition(&defs);
    let error = acquire(&rig(false).fetcher, &d, cache.path(), repository.path())
        .await
        .unwrap_err();
    assert!(
        matches!(error, AcquireError::NotInRepository { .. })
            && error.to_string().contains("structures/walls.yaml"),
        "{error}"
    );
    let path = repository.path().join("structures/walls.yaml");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "walls: []\n").unwrap();
    let acquired = acquire(&rig(false).fetcher, &d, cache.path(), repository.path())
        .await
        .unwrap();
    assert_eq!(acquired.files, [path]);
}

/// The 100 bytes of a file source's file.
fn grid() -> Vec<u8> {
    (0u8..100).collect()
}

#[tokio::test]
async fn a_file_on_disk_whose_record_differs_from_the_published_digest_is_fetched_again() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(grid()))
        .expect(1)
        .mount(&server)
        .await;
    let digest = hex(&Sha256::digest(grid()));
    let defs = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let access = format!(
        "  method: files\n  files: [{{ path: data/grid.npz, size: 100, sha256: {digest} }}]"
    );
    let d = common::definition(&defs, &access, &server.uri());
    let dir = cache.path().join("test-galaxies");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("grid.npz"), vec![7; 100]).unwrap();
    let stale = Payload {
        item: "data/grid.npz".to_owned(),
        url: format!("{}/data/grid.npz", server.uri()),
        sha256: "0".repeat(64),
        bytes: 100,
        retrieved: "2026-01-01T00:00:00Z".to_owned(),
        rows: None,
    };
    Provenance::record(&dir, &source(), &stale).unwrap();
    run(&d, cache.path()).await.unwrap();
    assert_eq!(std::fs::read(dir.join("grid.npz")).unwrap(), grid());
    let recorded = Provenance::read(&dir).unwrap();
    assert_eq!(recorded.payload("data/grid.npz").unwrap().sha256, digest);
    // Now the record matches, so the next run fetches nothing.
    run(&d, cache.path()).await.unwrap();
}

#[tokio::test]
async fn a_failing_chunk_lets_the_chunks_in_flight_finish_and_report_before_the_source_fails() {
    let server = MockServer::start().await;
    let sql = |chunk: u32| {
        QUERY
            .replace("{chunks}", "3")
            .replace("{chunk}", &chunk.to_string())
    };
    Mock::given(query_param("cmd", sql(0).as_str()))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    for chunk in [1, 2] {
        Mock::given(query_param("cmd", sql(chunk).as_str()))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("#Table1\nra,dec\n1,2\n")
                    .set_delay(Duration::from_millis(300)),
            )
            .mount(&server)
            .await;
    }
    let defs = tempfile::tempdir().unwrap();
    let access = format!("  method: skyserver_sql\n  query: \"{QUERY}\"\n  chunks: 3");
    let definition = common::definition(&defs, &access, &server.uri());
    let cache = tempfile::tempdir().unwrap();
    let rig = rig(false);
    let error = acquire(
        &rig.fetcher,
        &definition,
        cache.path(),
        Path::new("/nonexistent"),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("chunk 1 of 3"), "{error}");

    let mut state = RunState::default();
    for event in rig.events() {
        state.apply(&event, rig.clock.now());
    }
    let status = state.status(Duration::from_secs(1), 200);
    assert!(!status.contains(" | "), "no download in progress: {status}");
    let provenance = Provenance::read(&cache.path().join("test-galaxies")).unwrap();
    let mut items: Vec<&str> = provenance
        .payloads
        .iter()
        .map(|p| p.item.as_str())
        .collect();
    items.sort_unstable();
    assert_eq!(items, ["chunk 2 of 3", "chunk 3 of 3"]);
}
