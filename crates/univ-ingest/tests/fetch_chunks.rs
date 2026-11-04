//! Chunked queries are retried, moved across mirrors, kept on disk and never fetched twice (F-2).

mod common;

use std::time::Duration;

use common::{accept, item, keep, rig};
use univ_ingest::events::Event;
use univ_ingest::fetch::{Ask, FetchError, Fetched, Payload};
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
            &Ask::Get("/q?chunk=0".into()),
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
                &Ask::Get(format!("/q?chunk={n}")),
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
                &Ask::Get(format!("/q?chunk={n}")),
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
            &Ask::Get("/q?chunk=0".into()),
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
            &Ask::Get("/q?chunk=0".into()),
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
            &Ask::Get("/q?chunk=1".into()),
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
            &Ask::Get("/q?chunk=0".into()),
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
            &Ask::Get("/q?chunk=1".into()),
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
                &Ask::Get(format!("/q?chunk={n}")),
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
            &Ask::Get("/q".into()),
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
            &Ask::Get("/q".into()),
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

#[tokio::test]
async fn chunks_in_flight_on_a_dead_mirror_skip_it_once_and_stop_trying_it() {
    // Chunk 0 fails fast and uses up its retries while chunks 1 and 2 still wait for their
    // first answer.
    let dead = MockServer::start().await;
    Mock::given(query_param("chunk", "0"))
        .respond_with(always(503))
        .mount(&dead)
        .await;
    Mock::given(method("GET"))
        .respond_with(always(503).set_delay(Duration::from_millis(200)))
        .mount(&dead)
        .await;
    let working = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(always(200).set_body_string("row\n"))
        .mount(&working)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let rig = fetch_three(&dir, &dead, &working).await;
    let events = rig.events();
    let skipped: Vec<usize> = events
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(e, Event::MirrorSkipped { .. }))
        .map(|(n, _)| n)
        .collect();
    assert_eq!(skipped.len(), 1, "{events:#?}");
    let tried_after = events[skipped[0]..]
        .iter()
        .any(|e| matches!(e, Event::Get { url, .. } if url.starts_with(&dead.uri())));
    assert!(!tried_after, "no attempt on the mirror once it is skipped");
    let requests = dead.received_requests().await.unwrap().len();
    assert!(requests <= 5 + 2, "{requests} requests to the dead mirror");
}

#[tokio::test]
async fn chunks_that_use_up_a_mirror_together_report_its_skip_once() {
    let dead = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(always(503))
        .mount(&dead)
        .await;
    let working = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(always(200).set_body_string("row\n"))
        .mount(&working)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let rig = fetch_three(&dir, &dead, &working).await;
    let skipped = rig
        .events()
        .iter()
        .filter(|e| matches!(e, Event::MirrorSkipped { .. }))
        .count();
    assert_eq!(skipped, 1);
}

/// Fetches chunks 0, 1 and 2 into `dir` at once from `dead`, then `working`.
async fn fetch_three(
    dir: &tempfile::TempDir,
    dead: &MockServer,
    working: &MockServer,
) -> common::Rig {
    let rig = rig(false);
    let mirrors = [dead.uri(), working.uri()];
    let dests: Vec<_> = (0..3)
        .map(|n| dir.path().join(format!("chunk-{n}.tsv")))
        .collect();
    let names: Vec<String> = (0..3).map(|n| format!("chunk {n}")).collect();
    let asks: Vec<Ask> = (0..3).map(|n| Ask::Get(format!("/q?chunk={n}"))).collect();
    let fetch = |n: usize| {
        let item = item(&names[n], &dests[n]);
        let fetcher = &rig.fetcher;
        let (mirrors, ask) = (&mirrors, &asks[n]);
        async move { fetcher.chunk(&item, mirrors, ask, &accept, &keep).await }
    };
    let (a, b, c) = tokio::join!(fetch(0), fetch(1), fetch(2));
    assert!(a.is_ok() && b.is_ok() && c.is_ok());
    rig
}
