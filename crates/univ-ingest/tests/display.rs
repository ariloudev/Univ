//! What a run shows on a terminal and in a log, fed scripted events (F-13).

mod common;

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use common::{TestClock, source};
use univ_ingest::display::{
    Outcome, PlainTarget, RunDisplay, RunSink, RunState, SourceState, SourceSummary, StatusTarget,
    Summary,
};
use univ_ingest::events::{Event, EventSink, RestartReason, Stage};
use univ_ingest::fetch::Clock;
use univ_model::ids::SourceId;

/// One draw of a terminal: the lines printed above the status line, and the status line.
type Draw = (Vec<String>, String);

/// A terminal that records each draw it was asked for.
#[derive(Clone, Default)]
struct Screen(Arc<Mutex<Vec<Draw>>>);

impl Screen {
    fn draws(&self) -> Vec<Draw> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl StatusTarget for Screen {
    fn draw(&self, lines: &[String], status: &str) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((lines.to_vec(), status.to_owned()));
    }
    fn width(&self) -> usize {
        80
    }
    fn finish(&self, _: &[String]) {}
}

fn get(item: &str) -> Event {
    Event::Get {
        source: source(),
        item: item.into(),
        url: format!("https://example.org/{item}"),
    }
}

fn done(item: &str, bytes: u64) -> Event {
    Event::Done {
        source: source(),
        item: item.into(),
        bytes,
    }
}

fn now() -> SystemTime {
    TestClock::default().now()
}

fn lines(events: &[Event]) -> Vec<String> {
    let mut state = RunState::default();
    events
        .iter()
        .filter_map(|e| state.apply(e, now()))
        .collect()
}

#[test]
fn a_fetched_chunk_prints_a_get_line_with_its_number_source_chunk_and_size() {
    let printed = lines(&[
        get("chunk 1 of 2"),
        done("chunk 1 of 2", 1_234),
        get("chunk 2 of 2"),
        done("chunk 2 of 2", 56),
    ]);
    assert_eq!(
        printed,
        [
            "Get:1 test-galaxies chunk 1 of 2 [1.2 kB]",
            "Get:2 test-galaxies chunk 2 of 2 [56 B]"
        ]
    );
}

#[test]
fn a_chunk_already_on_disk_prints_a_hit_line() {
    let printed = lines(&[Event::Hit {
        source: source(),
        item: "chunk 1 of 2".into(),
    }]);
    assert_eq!(printed, ["Hit:1 test-galaxies chunk 1 of 2"]);
}

#[test]
fn a_failed_request_prints_an_err_line_with_the_mirror_and_reason() {
    let err = |retry_in| Event::Err {
        source: source(),
        item: "chunk 1 of 2".into(),
        mirror: "https://vizier.cds.unistra.fr/viz-bin/asu-tsv".into(),
        attempt: 1,
        error: "HTTP 503 Service Unavailable".into(),
        retry_in,
    };
    let printed = lines(&[
        get("chunk 1 of 2"),
        err(Some(Duration::from_secs(2))),
        err(None),
    ]);
    assert_eq!(
        printed,
        [
            "Err:1 test-galaxies chunk 1 of 2 https://vizier.cds.unistra.fr/viz-bin/asu-tsv: HTTP 503 Service Unavailable; retrying in 2 s",
            "Err:1 test-galaxies chunk 1 of 2 https://vizier.cds.unistra.fr/viz-bin/asu-tsv: HTTP 503 Service Unavailable; trying the next mirror",
        ]
    );
}

#[test]
fn a_resumed_download_prints_the_byte_it_continued_from_and_a_restart_says_why() {
    let printed = lines(&[
        get("grid.npz"),
        Event::Resumed {
            source: source(),
            item: "grid.npz".into(),
            from: 40,
        },
        Event::Restarted {
            source: source(),
            item: "grid.npz".into(),
            reason: RestartReason::RangeIgnored,
        },
    ]);
    assert_eq!(
        printed,
        [
            "Resume:1 test-galaxies grid.npz from byte 40",
            "Restart:1 test-galaxies grid.npz from the first byte: the provider ignored the range asked for",
        ]
    );
}

#[test]
fn the_status_line_gives_progress_rate_time_left_downloads_and_stage() {
    let mut state = RunState::default();
    for e in [
        Event::StageStarted {
            stage: Stage::Fetch,
            items: Some(4),
        },
        get("a"),
        done("a", 1_000),
        get("b"),
        Event::Received {
            source: source(),
            item: "b".into(),
            bytes: 250,
            total: Some(1_000),
        },
    ] {
        state.apply(&e, now());
    }
    let status = state.status(Duration::from_secs(10), 200);
    assert_eq!(
        status,
        "25% [fetch 1/4] | 1.2 kB of 2.0 kB 125 B/s 6s left | 2:test-galaxies b 250 B 25%"
    );
}

#[test]
fn the_status_line_fits_the_width_and_is_redrawn_at_most_ten_times_a_second() {
    let mut state = RunState::default();
    state.apply(
        &Event::StageStarted {
            stage: Stage::Normalize,
            items: Some(18),
        },
        now(),
    );
    for n in 0..5 {
        state.apply(&get(&format!("a long chunk name number {n}")), now());
    }
    let status = state.status(Duration::from_secs(1), 40);
    assert_eq!(status.chars().count(), 40);
    assert!(
        status.ends_with('…') && status.starts_with("0% [normalize 0/18]"),
        "{status}"
    );

    // Events between two draws are drawn together, in one draw with the status line.
    let clock = TestClock::default();
    let screen = Screen::default();
    let sink = RunSink::new(clock.clone(), screen.clone());
    for n in 0..5 {
        sink.emit(get(&format!("c{n}")));
        sink.emit(done(&format!("c{n}"), 1_000));
    }
    assert!(screen.draws().is_empty(), "an event alone draws nothing");
    clock.advance(Duration::from_secs(1));
    sink.draw();
    // A draw with no new event still brings the status line up to date.
    clock.advance(Duration::from_secs(1));
    sink.draw();
    let draws = screen.draws();
    assert_eq!(draws.len(), 2);
    assert_eq!(draws[0].0.len(), 5, "{draws:?}");
    assert_eq!(draws[0].1, "5.0 kB of 5.0 kB 5.0 kB/s");
    assert_eq!(draws[1].0, Vec::<String>::new());
    assert_eq!(draws[1].1, "5.0 kB of 5.0 kB 2.5 kB/s");
}

#[test]
fn each_event_and_stage_prints_as_one_plain_timestamped_line_without_control_codes() {
    let sink = RunSink::new(TestClock::default(), PlainTarget::new(Vec::new()));
    for e in [
        Event::StageStarted {
            stage: Stage::Fetch,
            items: Some(1),
        },
        get("chunk 1 of 1"),
        done("chunk 1 of 1", 2_048),
        Event::StageEnded {
            stage: Stage::Fetch,
            counts: vec![],
        },
    ] {
        sink.emit(e);
    }
    let text = String::from_utf8(sink.into_inner()).unwrap();
    assert_eq!(
        text,
        "2026-11-03T00:00:00Z Stage fetch: 1 items\n2026-11-03T00:00:00Z Get:1 test-galaxies chunk 1 of 1 [2.0 kB]\n2026-11-03T00:00:00Z Stage fetch done\n"
    );
    assert!(!text.contains('\u{1b}'));
}

#[test]
fn a_download_longer_than_a_minute_prints_one_progress_line_each_minute() {
    let clock = TestClock::default();
    let sink = RunSink::new(clock.clone(), PlainTarget::new(Vec::new()));
    sink.emit(get("grid.npz"));
    for (after, bytes) in [
        (30, 10_000_000),
        (31, 20_000_000),
        (29, 30_000_000),
        (31, 40_000_000),
    ] {
        clock.advance(Duration::from_secs(after));
        sink.emit(Event::Received {
            source: source(),
            item: "grid.npz".into(),
            bytes,
            total: Some(80_000_000),
        });
    }
    let text = String::from_utf8(sink.into_inner()).unwrap();
    let progress: Vec<&str> = text.lines().filter(|l| l.contains("Progress")).collect();
    assert_eq!(
        progress,
        [
            "2026-11-03T00:01:01Z Progress test-galaxies grid.npz 20.0 MB of 80.0 MB",
            "2026-11-03T00:02:01Z Progress test-galaxies grid.npz 40.0 MB of 80.0 MB",
        ]
    );
}

#[test]
fn the_summary_gives_bytes_time_rate_entries_per_source_and_how_the_run_ended() {
    let summary = Summary {
        bytes: 12_500_000,
        elapsed: Duration::from_secs(125),
        sources: vec![
            SourceSummary {
                source: source(),
                state: SourceState::Fetched,
                published: Some(1_097_100),
                left_out: Some(203),
            },
            SourceSummary {
                source: SourceId::new("cf4-basins").unwrap(),
                state: SourceState::Fetched,
                published: None,
                left_out: None,
            },
        ],
        stages: vec![],
        outcome: Outcome::Built(3),
    };
    assert_eq!(
        summary.lines(),
        [
            "Fetched 12.5 MB in 2m05s (100.0 kB/s)",
            "  test-galaxies: 1097100 entries published, 203 left out",
            "  cf4-basins: not normalized in this run",
            "Built release 3",
        ]
    );
    let stopped = Summary {
        outcome: Outcome::Stopped {
            stage: Stage::Fetch,
            reason: "interrupted".into(),
        },
        ..summary
    };
    assert_eq!(
        stopped.lines().last().unwrap(),
        "Stopped at fetch: interrupted"
    );
    let published = Summary {
        outcome: Outcome::Published(3),
        ..stopped
    };
    assert_eq!(published.lines().last().unwrap(), "Published release 3");
    let migrated = |shared, rows| {
        let summary = Summary {
            outcome: Outcome::Migrated { shared, rows },
            sources: vec![],
            ..published.clone()
        };
        summary.lines().last().unwrap().clone()
    };
    assert_eq!(
        [
            migrated(true, true),
            migrated(true, false),
            migrated(false, true),
            migrated(false, false)
        ],
        [
            "Migrated the store to shared rows, then its JSON at scale into rows",
            "Migrated the store to shared rows",
            "Migrated the store's JSON at scale into rows (schema 8)",
            "The store needed no migration",
        ]
    );
}

#[test]
fn the_summary_names_each_source_fetched_failed_or_not_reached() {
    let summary = |state| SourceSummary {
        source: source(),
        state,
        published: None,
        left_out: None,
    };
    let lines = Summary {
        bytes: 0,
        elapsed: Duration::from_secs(1),
        sources: vec![
            summary(SourceState::Fetched),
            summary(SourceState::Failed("chunk 1 of 2: HTTP 404".into())),
            summary(SourceState::NotReached),
        ],
        stages: vec![],
        outcome: Outcome::Stopped {
            stage: Stage::Fetch,
            reason: "interrupted by the owner".into(),
        },
    }
    .lines();
    assert_eq!(
        lines[1..4],
        [
            "  test-galaxies: not normalized in this run",
            "  test-galaxies: failed: chunk 1 of 2: HTTP 404",
            "  test-galaxies: not reached in this run",
        ]
    );
}

#[test]
fn a_resumed_download_counts_only_the_bytes_this_run_fetched() {
    let resumed = |from| Event::Resumed {
        source: source(),
        item: "grid.npz".into(),
        from,
    };
    let received = |bytes| Event::Received {
        source: source(),
        item: "grid.npz".into(),
        bytes,
        total: Some(100),
    };
    let ten = Duration::from_secs(10);
    let mut state = RunState::default();
    for e in [get("grid.npz"), resumed(60), received(80)] {
        state.apply(&e, now());
    }
    assert_eq!(
        state.status(ten, 200),
        "20 B of 40 B 2 B/s 10s left | 1:test-galaxies grid.npz 80 B 80%"
    );
    state.apply(&done("grid.npz", 100), now());
    assert_eq!(state.status(ten, 200), "40 B of 40 B 4 B/s");

    let mut state = RunState::default();
    let restarted = Event::Restarted {
        source: source(),
        item: "grid.npz".into(),
        reason: RestartReason::RangeIgnored,
    };
    for e in [get("grid.npz"), resumed(60), restarted, received(50)] {
        state.apply(&e, now());
    }
    assert!(
        state.status(ten, 200).starts_with("50 B of 100 B "),
        "{}",
        state.status(ten, 200)
    );
}

#[test]
fn a_stage_with_counts_prints_them_at_its_end_and_in_the_summary() {
    let sink = RunSink::new(TestClock::default(), PlainTarget::new(Vec::new()));
    let counts = vec![
        univ_ingest::events::Count::new(98, "objects"),
        univ_ingest::events::Count::new(3, "merges"),
    ];
    for e in [
        Event::StageStarted {
            stage: Stage::Match,
            items: Some(2),
        },
        Event::Stepped {
            stage: Stage::Match,
            step: "matching: 101 entries into 98 objects".into(),
        },
        Event::StageEnded {
            stage: Stage::Match,
            counts: counts.clone(),
        },
    ] {
        sink.emit(e);
    }
    sink.finish(
        vec![],
        Outcome::Stopped {
            stage: Stage::Match,
            reason: "cross-match only; no release was asked for".into(),
        },
    );
    let text = String::from_utf8(sink.into_inner()).unwrap();
    for line in [
        "Z Stage cross-match: 2 items\n",
        "Z Step cross-match: matching: 101 entries into 98 objects\n",
        "Z Stage cross-match done: 98 objects, 3 merges\n",
        "Z cross-match: 98 objects, 3 merges\n",
        "Z Stopped at cross-match: cross-match only; no release was asked for\n",
    ] {
        assert!(text.contains(line), "{line} in\n{text}");
    }

    let mut state = RunState::default();
    state.apply(
        &Event::StageStarted {
            stage: Stage::Match,
            items: Some(4),
        },
        now(),
    );
    state.apply(
        &Event::Stepped {
            stage: Stage::Match,
            step: "ranking".into(),
        },
        now(),
    );
    assert!(
        state
            .status(Duration::from_secs(1), 80)
            .starts_with("25% [cross-match 1/4]")
    );
}
