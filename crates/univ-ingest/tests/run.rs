//! A fetch reports each source as it ends, so an interrupted run names every source (F-13).
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;

use std::sync::Mutex;
use std::time::Duration;

use common::rig;
use univ_ingest::display::{Outcome, SourceState};
use univ_ingest::run::{FetchOptions, FetchReport, fetch, owner_interrupt};
use univ_ingest::source::SourceDefinition;
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The definition last written to `dir` by `common::definition`, under the id `id`.
fn renamed(dir: &tempfile::TempDir, id: &str) -> SourceDefinition {
    let yaml = std::fs::read_to_string(dir.path().join("test-galaxies.yaml"))
        .unwrap()
        .replace("id: test-galaxies", &format!("id: {id}"));
    let path = dir.path().join(format!("{id}.yaml"));
    std::fs::write(&path, yaml).unwrap();
    SourceDefinition::read(&path).unwrap()
}

#[tokio::test]
async fn an_interrupted_fetch_names_the_sources_it_ended_and_leaves_the_rest_not_reached() {
    let slow = MockServer::start().await;
    Mock::given(wiremock::matchers::any())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("#Table1\nra,dec\n1,2\n")
                .set_delay(Duration::from_secs(3_600)),
        )
        .mount(&slow)
        .await;
    let defs = tempfile::tempdir().unwrap();
    let repository = tempfile::tempdir().unwrap();
    std::fs::write(repository.path().join("walls.yaml"), "walls: []\n").unwrap();
    let access = "  method: skyserver_sql\n  query: \"SELECT ra, dec FROM T\"\n  chunks: 1";
    common::definition(&defs, access, &slow.uri());
    let waiting = renamed(&defs, "slow-galaxies");
    let local = common::definition(
        &defs,
        "  method: repository\n  path: walls.yaml",
        "https://example.org",
    );
    let definitions = [local, waiting];
    let cache = tempfile::tempdir().unwrap();
    let options = FetchOptions {
        definitions: defs.path().to_owned(),
        repository: repository.path().to_owned(),
        cache: cache.path().to_owned(),
        sources: Vec::new(),
    };
    let report = Mutex::new(FetchReport::new(&definitions));
    let rig = rig(false);
    // The owner interrupts the run once the second source is being fetched.
    let second_asked = async {
        while slow
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    };
    tokio::select! {
        () = fetch(&rig.fetcher, &definitions, &options, &report) => {
            panic!("the stalled source finished");
        }
        () = second_asked => {}
    }
    let (sources, outcome) = report.lock().unwrap().interrupted();
    let states: Vec<(&str, &SourceState)> = sources
        .iter()
        .map(|s| (s.source.as_str(), &s.state))
        .collect();
    assert_eq!(
        states,
        [
            ("test-galaxies", &SourceState::Fetched),
            ("slow-galaxies", &SourceState::NotReached),
        ]
    );
    assert!(matches!(outcome, Outcome::Stopped { reason, .. } if reason.contains("interrupted")));
}

#[tokio::test]
async fn a_ctrl_c_that_cannot_be_caught_never_counts_as_an_interrupt() {
    let wait = Duration::from_millis(50);
    let uncatchable = async { Err(std::io::Error::other("no signal handler")) };
    assert!(
        tokio::time::timeout(wait, owner_interrupt(uncatchable))
            .await
            .is_err(),
        "the run goes on"
    );
    let pressed = async { Ok(()) };
    assert!(
        tokio::time::timeout(wait, owner_interrupt(pressed))
            .await
            .is_ok()
    );
}
