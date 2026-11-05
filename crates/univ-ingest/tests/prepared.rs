//! A prepared source is read from the file a repository script made, once the file matches its
//! definition, and nothing is fetched for it.
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;

use std::io::Write as _;
use std::path::Path;

use common::rig;
use flate2::Compression;
use flate2::write::GzEncoder;
use sha2::{Digest as _, Sha256};
use univ_ingest::acquire::{AcquireError, Acquired, acquire};
use univ_ingest::fetch::hex;
use univ_ingest::provenance::Provenance;
use univ_ingest::source::SourceDefinition;
use wiremock::MockServer;

const SCRIPT: &str = "scripts/fetch-test.py";

/// A gzipped CSV of a header, a comment, a blank line and three rows.
fn table() -> Vec<u8> {
    let mut gz = GzEncoder::new(Vec::new(), Compression::default());
    gz.write_all(b"ra,dec\n# made by hand\n1,2\n\n3,4\n5,6\n")
        .unwrap();
    gz.finish().unwrap()
}

fn definition(dir: &tempfile::TempDir, mirror: &str, sha256: &str, rows: u64) -> SourceDefinition {
    let access = format!(
        "  method: prepared\n  file: test.csv.gz\n  script: {SCRIPT}\n  sha256: \"{sha256}\"\n  rows: {rows}"
    );
    common::definition(dir, &access, mirror)
}

/// Acquires `definition` from `cache`, and checks that no request reached `server`.
async fn run(
    server: &MockServer,
    definition: &SourceDefinition,
    cache: &Path,
) -> Result<Acquired, AcquireError> {
    let acquired = acquire(
        &rig(false).fetcher,
        definition,
        cache,
        Path::new("/nonexistent"),
    )
    .await;
    assert_eq!(
        server.received_requests().await.unwrap_or_default().len(),
        0
    );
    acquired
}

/// A cache holding the prepared file.
fn cache_with(body: &[u8]) -> tempfile::TempDir {
    let cache = tempfile::tempdir().unwrap();
    let dir = cache.path().join("test-galaxies");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("test.csv.gz"), body).unwrap();
    cache
}

#[tokio::test]
async fn a_prepared_file_that_matches_its_definition_is_read_and_recorded_without_a_request() {
    let server = MockServer::start().await;
    let body = table();
    let digest = hex(&Sha256::digest(&body));
    let defs = tempfile::tempdir().unwrap();
    let cache = cache_with(&body);
    let d = definition(&defs, &server.uri(), &digest, 3);
    let acquired = run(&server, &d, cache.path()).await.unwrap();
    let dir = cache.path().join("test-galaxies");
    assert_eq!(acquired.files, [dir.join("test.csv.gz")]);
    let provenance = Provenance::read(&dir).unwrap();
    let payload = provenance.payload("test.csv.gz").unwrap();
    assert_eq!(payload.sha256, digest);
    assert_eq!(payload.rows, Some(3));
    assert_eq!(payload.bytes, body.len() as u64);
    assert_eq!(payload.prepared_by.as_deref(), Some(SCRIPT));
    assert_eq!(payload.url, server.uri());
}

#[tokio::test]
async fn a_prepared_file_whose_digest_differs_is_refused() {
    let server = MockServer::start().await;
    let defs = tempfile::tempdir().unwrap();
    let cache = cache_with(&table());
    let d = definition(&defs, &server.uri(), &"0".repeat(64), 3);
    let error = run(&server, &d, cache.path()).await.unwrap_err();
    let message = error.to_string();
    assert!(
        matches!(
            error,
            AcquireError::PreparedDiffers {
                what: "SHA-256",
                ..
            }
        ),
        "{message}"
    );
    assert!(message.contains("test.csv.gz"), "{message}");
    assert!(message.contains(&"0".repeat(64)), "{message}");
    assert!(
        message.contains(&hex(&Sha256::digest(table()))),
        "{message}"
    );
}

#[tokio::test]
async fn a_prepared_file_whose_rows_differ_is_refused() {
    let server = MockServer::start().await;
    let body = table();
    let defs = tempfile::tempdir().unwrap();
    let cache = cache_with(&body);
    let d = definition(&defs, &server.uri(), &hex(&Sha256::digest(&body)), 4);
    let error = run(&server, &d, cache.path()).await.unwrap_err();
    let message = error.to_string();
    assert!(
        matches!(
            error,
            AcquireError::PreparedDiffers {
                what: "row count",
                ..
            }
        ),
        "{message}"
    );
    assert!(
        message.contains("row count 3 where its definition gives 4"),
        "{message}"
    );
}

#[tokio::test]
async fn a_missing_prepared_file_fails_naming_its_script() {
    let server = MockServer::start().await;
    let defs = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let d = definition(&defs, &server.uri(), &"0".repeat(64), 3);
    let error = run(&server, &d, cache.path()).await.unwrap_err();
    assert!(
        matches!(error, AcquireError::NotPrepared { ref script, .. } if script == SCRIPT),
        "{error}"
    );
    assert!(error.to_string().contains(SCRIPT), "{error}");
}

#[test]
fn a_prepared_file_is_named_as_a_gzipped_csv_file_with_a_sha256() {
    let defs = tempfile::tempdir().unwrap();
    let write = |access: &str| common::try_definition(&defs, access, "https://example.org").is_ok();
    let good = |file: &str, sha: &str| {
        format!(
            "  method: prepared\n  file: {file}\n  script: {SCRIPT}\n  sha256: \"{sha}\"\n  rows: 1"
        )
    };
    let digest = "a".repeat(64);
    assert!(write(&good("t.csv.gz", &digest)));
    assert!(!write(&good("t.csv", &digest)));
    assert!(!write(&good("sub/t.csv.gz", &digest)));
    assert!(!write(&good("t.csv.gz", "abc")));
}
