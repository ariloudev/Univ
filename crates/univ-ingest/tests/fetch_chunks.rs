//! Chunked queries are retried, moved across mirrors, kept on disk and never fetched twice (F-2).

mod common;

use std::time::Duration;

use common::{accept, item, keep, rig};
use univ_ingest::events::Event;
use univ_ingest::fetch::{FetchError, Fetched, Payload};
use wiremock::matchers::{method, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn always(status: u16) -> ResponseTemplate {
    ResponseTemplate::new(status)
}

#[tokio::test]
async fn a_request_is_retried_four_times_with_waits_doubling_from_two_seconds() {
    let failing = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(always(503))
        .expect(5)
        .mount(&failing)
        .await;
    let working = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(always(200).set_body_string("ra\tdec\n"))
        .expect(1)
        .mount(&working)
        .await;
    let rig = rig(false);
    let dir = tempfile::tempdir().unwrap();
    let mirrors = [failing.uri(), working.uri()];
    let dest = dir.path().join("chunk-0.tsv");
    let fetched = rig
        .fetcher
        .chunk(
            &item("chunk 0", &dest),
            &mirrors,
            "/q?chunk=0",
            &accept,
            &keep,
        )
        .await
        .unwrap();
    assert!(matches!(fetched, Fetched::Arrived(_)));
    let waits: Vec<u64> = rig.clock.waits().iter().map(Duration::as_secs).collect();
    assert_eq!(waits, [2, 4, 8, 16]);
}

#[tokio::test]
async fn a_mirror_that_fails_all_its_retries_is_skipped_for_the_rest_of_the_run() {
    let failing = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(always(500))
        .expect(5)
        .mount(&failing)
        .await;
    let working = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(always(200).set_body_string("row\n"))
        .expect(2)
        .mount(&working)
        .await;
    let rig = rig(false);
    let dir = tempfile::tempdir().unwrap();
    let mirrors = [failing.uri(), working.uri()];
    for n in 0..2 {
        let dest = dir.path().join(format!("chunk-{n}.tsv"));
        rig.fetcher
            .chunk(
                &item(&format!("chunk {n}"), &dest),
                &mirrors,
                &format!("/q?chunk={n}"),
                &accept,
                &keep,
            )
            .await
            .unwrap();
    }
    let skipped = rig
        .events()
        .iter()
        .filter(|e| matches!(e, Event::MirrorSkipped { .. }))
        .count();
    assert_eq!(skipped, 1);
}

#[tokio::test]
async fn a_run_resumes_from_the_first_chunk_not_on_disk_and_fetches_no_chunk_twice() {
    let server = MockServer::start().await;
    for n in 0..4 {
        let expected = u64::from(n >= 2);
        Mock::given(query_param("chunk", n.to_string()))
            .respond_with(always(200).set_body_string(format!("chunk {n}\n")))
            .expect(expected)
            .mount(&server)
            .await;
    }
    let rig = rig(false);
    let dir = tempfile::tempdir().unwrap();
    for n in 0..2 {
        std::fs::write(
            dir.path().join(format!("chunk-{n}.tsv")),
            format!("kept {n}\n"),
        )
        .unwrap();
    }
    for n in 0..4 {
        let dest = dir.path().join(format!("chunk-{n}.tsv"));
        rig.fetcher
            .chunk(
                &item(&format!("chunk {n}"), &dest),
                &[server.uri()],
                &format!("/q?chunk={n}"),
                &accept,
                &keep,
            )
            .await
            .unwrap();
    }
    assert_eq!(
        std::fs::read_to_string(dir.path().join("chunk-1.tsv")).unwrap(),
        "kept 1\n"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("chunk-3.tsv")).unwrap(),
        "chunk 3\n"
    );
    let hits = rig
        .events()
        .iter()
        .filter(|e| matches!(e, Event::Hit { .. }))
        .count();
    assert_eq!(hits, 2);
}

#[tokio::test]
async fn a_chunk_is_renamed_into_place_only_once_complete_and_a_partial_file_is_never_read() {
    let server = MockServer::start().await;
    Mock::given(query_param("chunk", "0"))
        .respond_with(always(200).set_body_string("<html>Server overloaded</html>"))
        .mount(&server)
        .await;
    let rig = rig(false);
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("chunk-0.tsv");
    // A partial file left by an earlier run is not a chunk: the chunk is fetched.
    std::fs::write(dir.path().join("chunk-0.tsv.part"), "half a chu").unwrap();
    let tsv_only = |body: &[u8]| {
        if body.starts_with(b"<html>") {
            Err("an error page".to_owned())
        } else {
            Ok(0)
        }
    };
    let refused = rig
        .fetcher
        .chunk(
            &item("chunk 0", &dest),
            &[server.uri()],
            "/q?chunk=0",
            &tsv_only,
            &keep,
        )
        .await;
    assert!(refused.is_err());
    assert!(
        !dest.exists(),
        "a body that failed its check must not become a chunk"
    );
    // The next run, once the provider recovers, fetches the chunk whole.
    server.reset().await;
    Mock::given(query_param("chunk", "0"))
        .respond_with(always(200).set_body_string("ra\tdec\n"))
        .mount(&server)
        .await;
    let next_run = common::rig(false);
    next_run
        .fetcher
        .chunk(
            &item("chunk 0", &dest),
            &[server.uri()],
            "/q?chunk=0",
            &tsv_only,
            &keep,
        )
        .await
        .unwrap();
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "ra\tdec\n");
    assert!(!dir.path().join("chunk-0.tsv.part").exists());
}

#[tokio::test]
async fn a_chunk_no_mirror_can_give_fails_naming_the_source_the_chunk_and_each_mirrors_last_error()
{
    let busy = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(always(503))
        .mount(&busy)
        .await;
    let missing = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(always(404))
        .expect(1)
        .mount(&missing)
        .await;
    let rig = rig(false);
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("chunk-0.tsv"), "kept\n").unwrap();
    let dest = dir.path().join("chunk-1.tsv");
    let error = rig
        .fetcher
        .chunk(
            &item("chunk 1", &dest),
            &[busy.uri(), missing.uri()],
            "/q?chunk=1",
            &accept,
            &keep,
        )
        .await
        .unwrap_err();
    let FetchError::AllMirrorsFailed { errors, .. } = &error else {
        panic!("{error}")
    };
    assert_eq!(errors.len(), 2);
    let shown = error.to_string();
    assert!(
        shown.starts_with("test-galaxies: chunk 1 could not be fetched"),
        "{shown}"
    );
    assert!(
        shown.contains(&format!("{}: HTTP 503", busy.uri()))
            && shown.contains(&format!("{}: HTTP 404", missing.uri())),
        "{shown}"
    );
    // The chunks already on disk stay for the next run.
    assert_eq!(
        std::fs::read_to_string(dir.path().join("chunk-0.tsv")).unwrap(),
        "kept\n"
    );
}

#[tokio::test]
async fn an_offline_run_makes_no_request_and_fails_on_the_first_missing_chunk() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(always(200))
        .expect(0)
        .mount(&server)
        .await;
    let rig = rig(true);
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("chunk-0.tsv"), "kept\n").unwrap();
    let on_disk = rig
        .fetcher
        .chunk(
            &item("chunk 0", &dir.path().join("chunk-0.tsv")),
            &[server.uri()],
            "/q?chunk=0",
            &accept,
            &keep,
        )
        .await
        .unwrap();
    assert!(matches!(on_disk, Fetched::OnDisk));
    let missing = rig
        .fetcher
        .chunk(
            &item("chunk 1", &dir.path().join("chunk-1.tsv")),
            &[server.uri()],
            "/q?chunk=1",
            &accept,
            &keep,
        )
        .await
        .unwrap_err();
    assert!(
        matches!(missing, FetchError::MissingOffline { .. }),
        "{missing}"
    );
}

/// Answers chunk 0 with an error page and chunk 1 with a table.
async fn error_page_for_chunk_0() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(query_param("chunk", "0"))
        .respond_with(always(200).set_body_string("<html>Server overloaded</html>"))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(query_param("chunk", "1"))
        .respond_with(always(200).set_body_string("ra\tdec\n"))
        .expect(1)
        .mount(&server)
        .await;
    server
}

fn tsv_only(body: &[u8]) -> Result<u64, String> {
    if body.starts_with(b"<html>") {
        Err("an error page".to_owned())
    } else {
        Ok(1)
    }
}

#[tokio::test]
async fn a_body_that_fails_its_check_moves_to_the_next_mirror_without_skipping_this_one() {
    let flaky = error_page_for_chunk_0().await;
    let working = MockServer::start().await;
    Mock::given(query_param("chunk", "0"))
        .respond_with(always(200).set_body_string("ra\tdec\n"))
        .expect(1)
        .mount(&working)
        .await;
    let rig = rig(false);
    let dir = tempfile::tempdir().unwrap();
    let mirrors = [flaky.uri(), working.uri()];
    for n in 0..2 {
        let dest = dir.path().join(format!("chunk-{n}.tsv"));
        rig.fetcher
            .chunk(
                &item(&format!("chunk {n}"), &dest),
                &mirrors,
                &format!("/q?chunk={n}"),
                &tsv_only,
                &keep,
            )
            .await
            .unwrap();
    }
    assert!(
        rig.clock.waits().is_empty(),
        "a failed body check is not retried"
    );
    assert!(
        !rig.events()
            .iter()
            .any(|e| matches!(e, Event::MirrorSkipped { .. })),
        "the mirror stays in use for chunk 1"
    );
}

#[tokio::test]
async fn a_chunk_is_recorded_before_it_is_renamed_and_a_failed_record_leaves_no_chunk() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(always(200).set_body_string("ra\tdec\n"))
        .mount(&server)
        .await;
    let rig = rig(false);
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("chunk-0.tsv");
    let seen = |payload: &Payload| {
        assert!(!dest.exists(), "recorded before the rename");
        assert_eq!(payload.rows, Some(1));
        Ok(())
    };
    rig.fetcher
        .chunk(
            &item("chunk 0", &dest),
            &[server.uri()],
            "/q",
            &tsv_only,
            &seen,
        )
        .await
        .unwrap();
    assert!(dest.exists());
    let dest = dir.path().join("chunk-1.tsv");
    let failing = |_: &Payload| Err(std::io::Error::other("disk full"));
    let error = rig
        .fetcher
        .chunk(
            &item("chunk 1", &dest),
            &[server.uri()],
            "/q",
            &tsv_only,
            &failing,
        )
        .await
        .unwrap_err();
    assert!(matches!(error, FetchError::Disk { .. }), "{error}");
    assert!(
        !dest.exists(),
        "a chunk without its record is not renamed into place"
    );
}
