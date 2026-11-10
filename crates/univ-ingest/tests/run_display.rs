//! Every stage of a run on the one run display: the fetch and normalize stages end with their
//! counts, a source that is no table says what became of it, a long stage prints a progress line
//! once a minute in a log, and a publish or prune shows a stage, a step per file or step, and a
//! summary of what it uploaded or deleted (F-13).
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod memory_store;
mod release_fixture;

use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use common::{TestClock, rig};
use memory_store::{Clock, MemoryStore, publishing};
use release_fixture::{build_next, fixture};
use univ_ingest::display::{
    Outcome, PlainTarget, RunDisplay, RunSink, SourceState, SourceSummary, Summary, size,
};
use univ_ingest::events::{Count, Event, EventSink, RecordingSink, Stage};
use univ_ingest::fetch::Clock as _;
use univ_ingest::publish::StoreOptions;
use univ_ingest::run::{
    FetchOptions, RunReport, Shown, fetch, normalize_stage, prune_command, publish_command,
};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ANSWER: &str = "#Table1\nra,dec\n1,2\n";

fn counts_at_end(events: &[Event], stage: Stage) -> Vec<Count> {
    events
        .iter()
        .find_map(|e| match e {
            Event::StageEnded { stage: s, counts } if *s == stage => Some(counts.clone()),
            _ => None,
        })
        .unwrap()
}

#[tokio::test]
async fn the_fetch_ends_with_what_it_got_hit_failed_and_fetched() {
    let server = MockServer::start().await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(200).set_body_string(ANSWER))
        .mount(&server)
        .await;
    let defs = tempfile::tempdir().unwrap();
    let access = "  method: skyserver_sql\n  query: \"SELECT ra, dec FROM T\"\n  chunks: 1";
    let definitions = [common::definition(&defs, access, &server.uri())];
    let cache = tempfile::tempdir().unwrap();
    let options = FetchOptions {
        definitions: defs.path().to_owned(),
        repository: defs.path().to_owned(),
        cache: cache.path().to_owned(),
        sources: Vec::new(),
    };
    let first = rig(false);
    let report = Mutex::new(RunReport::new(&definitions));
    fetch(&first.fetcher, &definitions, &options, &report).await;
    let bytes = ANSWER.len() as u64;
    assert_eq!(
        counts_at_end(&first.events(), Stage::Fetch),
        [
            Count::new(1, "got"),
            Count::new(0, "hit"),
            Count::new(0, "sources failed"),
            Count::new(bytes, "bytes fetched"),
        ]
    );
    let next = rig(true);
    fetch(&next.fetcher, &definitions, &options, &report).await;
    assert_eq!(
        counts_at_end(&next.events(), Stage::Fetch),
        [
            Count::new(0, "got"),
            Count::new(1, "hit"),
            Count::new(0, "sources failed"),
            Count::new(0, "bytes fetched"),
        ]
    );
}

#[tokio::test]
async fn a_source_that_is_no_table_says_what_became_of_it_rather_than_not_normalized() {
    let defs = tempfile::tempdir().unwrap();
    let repository = tempfile::tempdir().unwrap();
    std::fs::write(repository.path().join("walls.yaml"), "walls: []\n").unwrap();
    let access = "  method: repository\n  path: walls.yaml";
    let definitions = [common::definition(&defs, access, "https://example.org")];
    let cache = tempfile::tempdir().unwrap();
    let options = FetchOptions {
        definitions: defs.path().to_owned(),
        repository: repository.path().to_owned(),
        cache: cache.path().to_owned(),
        sources: Vec::new(),
    };
    let rig = rig(true);
    let report = Mutex::new(RunReport::new(&definitions));
    let acquired = fetch(&rig.fetcher, &definitions, &options, &report).await;
    normalize_stage(
        rig.sink.as_ref(),
        &definitions,
        &acquired,
        &options,
        &report,
    )
    .await;
    let sources = report.lock().unwrap().sources().to_vec();
    let used = SourceState::Used("structure file, not normalized: read by a cross-match".into());
    assert_eq!(sources[0].state, used);
    let summary = Summary {
        bytes: 0,
        elapsed: Duration::from_secs(1),
        sources: vec![SourceSummary {
            state: SourceState::Used("volume grid, read as a grid into release 6".into()),
            ..sources[0].clone()
        }],
        stages: vec![],
        outcome: Outcome::Built(6),
    };
    assert_eq!(
        summary.lines()[1],
        "  test-galaxies: volume grid, read as a grid into release 6"
    );
}

#[test]
fn a_long_stage_prints_a_progress_line_each_minute_it_prints_nothing_else() {
    let clock = TestClock::default();
    let sink = RunSink::new(clock.clone(), PlainTarget::new(Vec::new()));
    let tick = |secs| {
        clock.advance(Duration::from_secs(secs));
        sink.draw();
    };
    sink.emit(Event::StageStarted {
        stage: Stage::Publish,
        items: Some(3),
    });
    tick(59);
    tick(2);
    sink.emit(Event::Uploaded {
        stage: Stage::Publish,
        step: "release 1: d1/0001-objects-001.sql: imported, 1 of 2 files of objects".into(),
        bytes: 2_000,
    });
    tick(59);
    tick(10);
    sink.emit(Event::StageEnded {
        stage: Stage::Publish,
        counts: vec![],
    });
    tick(120);
    let text = String::from_utf8(sink.into_inner()).unwrap();
    let progress: Vec<&str> = text.lines().filter(|l| l.contains("Progress")).collect();
    assert_eq!(
        progress,
        [
            "2026-11-03T00:01:01Z Progress publish: 0 of 3 items done, 1m01s in the stage",
            "2026-11-03T00:02:10Z Progress publish: 1 of 3 items done, 2.0 kB uploaded, 2m10s \
             in the stage",
        ]
    );
}

/// The objects of release 1 the publish's steps say were loaded, summed over the shards.
fn objects_loaded(uploaded: &[(&str, u64)]) -> u64 {
    uploaded
        .iter()
        .filter_map(|(step, _)| {
            let rest = step.split("imported; objects: ").nth(1)?;
            let (n, shard) = rest.split_once(" rows of release 1 in shard ")?;
            shard.parse::<u32>().ok()?;
            n.parse::<u64>().ok()
        })
        .sum()
}

#[test]
fn a_publish_and_a_prune_show_a_stage_a_step_per_file_and_a_summary_of_what_they_did() {
    let f = fixture();
    let built = build_next(&f);
    let (journals, clock) = (tempfile::tempdir().unwrap(), Clock::new());
    let mut store = MemoryStore::default();
    let now = || clock.now();
    let options = StoreOptions {
        out: f.out.path(),
        state: f.state.path(),
        journals: journals.path(),
        now: &now,
    };
    let (events, never) = (RecordingSink::default(), AtomicBool::new(false));
    let mut shown = Shown::new(&events, Stage::Publish, &never);
    let outcome = publish_command(&mut store, 1, &options, &mut shown).unwrap();
    assert_eq!(outcome, Outcome::Published(1));
    let events = events.events();
    let total = built.manifest.files.len() + 1;
    assert_eq!(
        events[0],
        Event::StageStarted {
            stage: Stage::Publish,
            items: Some(total as u64)
        }
    );
    let uploaded: Vec<(&str, u64)> = events
        .iter()
        .filter_map(|e| match e {
            Event::Uploaded { step, bytes, .. } => Some((step.as_str(), *bytes)),
            _ => None,
        })
        .collect();
    assert_eq!(uploaded.len(), total, "a step for each file");
    // The objects are loaded shard by shard, 25 in all.
    assert_eq!(objects_loaded(&uploaded), 25, "{uploaded:?}");
    let counts = counts_at_end(&events, Stage::Publish);
    for what in ["D1 files", "rules files", "tile files", "manifest files"] {
        assert!(
            counts.iter().any(|c| c.what == what),
            "{what} in {counts:?}"
        );
    }
    assert!(
        counts.contains(&Count::new(25, "objects rows")),
        "{counts:?}"
    );

    // On the run display: the bytes uploaded and the release published.
    let sink = RunSink::new(TestClock::default(), PlainTarget::new(Vec::new()));
    for e in &events {
        sink.emit(e.clone());
    }
    sink.finish(vec![], outcome);
    let text = String::from_utf8(sink.into_inner()).unwrap();
    let bytes: u64 = uploaded.iter().map(|(_, b)| b).sum();
    for line in [
        format!("Z Stage publish: {total} items\n"),
        format!("Z Uploaded {} in 0s (0 B/s)\n", size(bytes)),
        "Z Published release 1\n".to_owned(),
    ] {
        assert!(text.contains(&line), "{line} in\n{text}");
    }
    assert!(
        text.contains("Z Step publish: Published release 1 at "),
        "{text}"
    );

    let second = build_next(&f);
    let (out, state) = (f.out.path(), f.state.path());
    publishing(&mut store, 2, out, state, journals.path(), &clock).unwrap();
    let events = RecordingSink::default();
    let mut shown = Shown::new(&events, Stage::Prune, &never);
    let outcome = prune_command(&mut store, 1, &options, &mut shown).unwrap();
    assert_eq!(outcome, Outcome::Pruned(1));
    let sink = RunSink::new(TestClock::default(), PlainTarget::new(Vec::new()));
    for e in events.events() {
        sink.emit(e);
    }
    sink.finish(vec![], outcome);
    let text = String::from_utf8(sink.into_inner()).unwrap();
    for line in [
        "Z Stage prune: 3 items\n",
        "Z Ran for 0s\n",
        "Z Pruned release 1\n",
    ] {
        assert!(text.contains(line), "{line} in\n{text}");
    }
    // Release 1's files that release 2 does not share, and its manifest.
    let keys_of = |m: &univ_model::release::ReleaseManifest| -> std::collections::BTreeSet<String> {
        m.files.iter().filter_map(|f| f.key.clone()).collect()
    };
    let r2 = keys_of(&built.manifest)
        .difference(&keys_of(&second.manifest))
        .count()
        + 1;
    let deleted = format!("Z Stage prune done: {r2} R2 files deleted, ");
    assert!(text.contains(&deleted), "{deleted} in\n{text}");
    assert!(
        text.contains(" rows deleted, 1 local builds deleted\n"),
        "{text}"
    );
}

#[test]
fn a_note_of_a_stage_counts_as_no_item_of_it() {
    let mut state = univ_ingest::display::RunState::default();
    let now = TestClock::default().now();
    for e in [
        Event::StageStarted {
            stage: Stage::Publish,
            items: Some(2),
        },
        Event::Noted {
            stage: Stage::Publish,
            line: "release 2: 174 of 204 files already uploaded and checked, by the journal".into(),
        },
        Event::Uploaded {
            stage: Stage::Publish,
            step: "release 2: manifest.yaml: read back with its SHA-256".into(),
            bytes: 1_000,
        },
    ] {
        state.apply(&e, now);
    }
    let status = state.status(Duration::from_secs(1), 80);
    assert!(
        status.starts_with("50% [publish 1/2] | 1.0 kB "),
        "{status}"
    );
}
