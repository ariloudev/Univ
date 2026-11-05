//! A TAP query run as a job is created, started through its phase, polled until it completes,
//! its result kept and the job deleted; a job kept on disk is not run again (F-2). The services
//! here refuse a job created with its phase, as Astro Data Lab does.

mod common;

use std::path::Path;
use std::time::Duration;

use common::{accept, item, keep, rig};
use univ_ingest::acquire::acquire;
use univ_ingest::events::Event;
use univ_ingest::fetch::{Ask, FetchError, Fetched};
use wiremock::matchers::{body_string_contains, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const FORM: &str = "REQUEST=doQuery&QUERY=SELECT+1&PHASE=RUN";

fn ok() -> ResponseTemplate {
    ResponseTemplate::new(200)
}

/// Mounts on `server` a TAP service's taking of `job`: a submission that sets a phase refused
/// as Astro Data Lab refuses it, one without redirected to the job, and the job started once by
/// `PHASE=RUN` sent to its phase.
async fn takes(server: &MockServer, job: &str) {
    Mock::given(method("POST"))
        .and(path("/tap/async"))
        .and(body_string_contains("PHASE"))
        .respond_with(
            ResponseTemplate::new(400).set_body_string("PHASE: Execution Phase is mandatory."),
        )
        .with_priority(1)
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/tap/async"))
        .respond_with(
            ResponseTemplate::new(303).insert_header("Location", format!("/tap/async/{job}")),
        )
        .expect(1)
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/tap/async/{job}/phase")))
        .and(body_string_contains("PHASE=RUN"))
        .respond_with(
            ResponseTemplate::new(303).insert_header("Location", format!("/tap/async/{job}")),
        )
        .expect(1)
        .mount(server)
        .await;
}

/// A TAP service that takes one job: it redirects the submission to the job, reports it
/// executing once and then completed, serves its result and deletes it once asked.
async fn job_service(result: &str) -> MockServer {
    let server = MockServer::start().await;
    takes(&server, "job-1").await;
    Mock::given(method("GET"))
        .and(path("/tap/async/job-1"))
        .respond_with(ok().set_body_string("<uws:job/>"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/tap/async/job-1/phase"))
        .respond_with(ok().set_body_string("EXECUTING"))
        .up_to_n_times(1)
        .with_priority(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/tap/async/job-1/phase"))
        .respond_with(ok().set_body_string("COMPLETED\n"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/tap/async/job-1/results/result"))
        .respond_with(ok().set_body_string(result))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/tap/async/job-1"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    server
}

/// Fetches the job of [`FORM`] across `mirrors` into `dir`'s `chunk-0.csv`.
async fn fetch_job(
    rig: &common::Rig,
    mirrors: &[String],
    dir: &tempfile::TempDir,
) -> Result<Fetched, FetchError> {
    let dest = dir.path().join("chunk-0.csv");
    rig.fetcher
        .chunk(
            &item("chunk 0", &dest),
            mirrors,
            &Ask::Job(FORM.to_owned()),
            &accept,
            &keep,
        )
        .await
}

fn tap(server: &MockServer) -> String {
    format!("{}/tap", server.uri())
}

fn waits(rig: &common::Rig) -> Vec<u64> {
    rig.clock.waits().iter().map(Duration::as_secs).collect()
}

#[tokio::test]
async fn a_query_run_as_a_job_is_polled_until_it_completes_and_its_result_kept() {
    let server = job_service("source_id,bp_rp\n1,0.5\n").await;
    let rig = rig(false);
    let dir = tempfile::tempdir().unwrap();
    let fetched = fetch_job(&rig, &[tap(&server)], &dir).await.unwrap();
    let Fetched::Arrived(payload) = fetched else {
        panic!("fetched")
    };
    // Recorded by the request that ran it, which outlasts the job's own address.
    assert_eq!(payload.url, format!("{}/async?{FORM}", tap(&server)));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("chunk-0.csv")).unwrap(),
        "source_id,bp_rp\n1,0.5\n"
    );
    // A wait before each of the two polls.
    assert_eq!(waits(&rig), [5, 5]);
}

#[tokio::test]
async fn a_job_that_ends_in_error_is_not_submitted_again_and_the_next_mirror_runs_it() {
    let failing = MockServer::start().await;
    takes(&failing, "job-9").await;
    Mock::given(method("GET"))
        .and(path("/tap/async/job-9"))
        .respond_with(ok())
        .mount(&failing)
        .await;
    Mock::given(method("GET"))
        .and(path("/tap/async/job-9/phase"))
        .respond_with(ok().set_body_string("ERROR"))
        .mount(&failing)
        .await;
    Mock::given(method("GET"))
        .and(path("/tap/async/job-9/error"))
        .respond_with(ok().set_body_string("Unknown column bp_rq"))
        .mount(&failing)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/tap/async/job-9"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&failing)
        .await;
    let working = job_service("source_id\n1\n").await;
    let rig = rig(false);
    let dir = tempfile::tempdir().unwrap();
    fetch_job(&rig, &[tap(&failing), tap(&working)], &dir)
        .await
        .unwrap();
    let said = rig.sink.events().iter().any(|e| {
        matches!(e, Event::Err { error, .. } if error.contains("the job ended ERROR: Unknown column bp_rq"))
    });
    assert!(said, "{:?}", rig.sink.events());
}

#[tokio::test]
async fn a_poll_that_fails_is_asked_again_and_the_job_is_not_submitted_again() {
    let server = job_service("source_id\n1\n").await;
    // The first poll fails, as a dropped connection does.
    Mock::given(method("GET"))
        .and(path("/tap/async/job-1/phase"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    let rig = rig(false);
    let dir = tempfile::tempdir().unwrap();
    fetch_job(&rig, &[tap(&server)], &dir).await.unwrap();
    // The submission was made once (the service expects one), and nothing was reported as a
    // failed attempt.
    assert!(
        !rig.sink
            .events()
            .iter()
            .any(|e| matches!(e, Event::Err { .. })),
        "{:?}",
        rig.sink.events()
    );
    assert_eq!(waits(&rig), [5, 5, 5]);
}

#[tokio::test]
async fn a_job_that_does_not_complete_in_half_an_hour_is_deleted_and_the_next_mirror_runs_it() {
    let slow = MockServer::start().await;
    takes(&slow, "job-5").await;
    Mock::given(method("GET"))
        .and(path("/tap/async/job-5"))
        .respond_with(ok())
        .mount(&slow)
        .await;
    Mock::given(method("GET"))
        .and(path("/tap/async/job-5/phase"))
        .respond_with(ok().set_body_string("EXECUTING"))
        .mount(&slow)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/tap/async/job-5"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&slow)
        .await;
    let working = job_service("source_id\n1\n").await;
    let rig = rig(false);
    let dir = tempfile::tempdir().unwrap();
    fetch_job(&rig, &[tap(&slow), tap(&working)], &dir)
        .await
        .unwrap();
    let waited: u64 = waits(&rig).iter().sum();
    assert_eq!(waited, 30 * 60 + 2 * 5);
}

#[tokio::test]
async fn a_job_kept_on_disk_with_its_record_is_not_run_again() {
    let server = job_service("ra,dec\n1,2\n3,4\n").await;
    Mock::given(method("GET"))
        .and(path("/tap/sync"))
        .and(query_param("QUERY", "SELECT COUNT(*) AS n FROM T"))
        .respond_with(ok().set_body_string("n\n2\n"))
        .expect(1)
        .mount(&server)
        .await;
    let defs = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let d = common::definition(
        &defs,
        "  method: tap\n  query: \"SELECT ra, dec FROM T\"\n  chunks: 1\n  count: \"SELECT COUNT(*) AS n FROM T\"\n  run: async",
        &tap(&server),
    );
    for run in 0..2 {
        let rig = rig(false);
        acquire(&rig.fetcher, &d, cache.path(), Path::new("/nonexistent"))
            .await
            .unwrap();
        if run == 0 {
            server.verify().await;
            server.reset().await;
        }
    }
    assert_eq!(server.received_requests().await.unwrap().len(), 0);
}
