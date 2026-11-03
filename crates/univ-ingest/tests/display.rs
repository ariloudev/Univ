//! What a run shows on a terminal and in a log, fed scripted events (F-13).

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use univ_ingest::display::{
    Outcome, PlainSink, RunState, SourceSummary, StatusTarget, Summary, TerminalSink,
};
use univ_ingest::events::{Event, EventSink, RestartReason, Stage};
use univ_ingest::fetch::Clock;
use univ_model::ids::SourceId;

/// A clock the test moves by hand.
#[derive(Clone)]
struct ManualClock(Arc<Mutex<SystemTime>>);

impl ManualClock {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(
            UNIX_EPOCH + Duration::from_hours(20_760 * 24),
        )))
    }
    fn advance(&self, by: Duration) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) += by;
    }
}

impl Clock for ManualClock {
    fn sleep(&self, _: Duration) -> impl Future<Output = ()> + Send {
        std::future::ready(())
    }
    fn now(&self) -> SystemTime {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A terminal that records what it was asked to draw.
#[derive(Clone, Default)]
struct Screen(Arc<Mutex<(Vec<String>, Vec<String>)>>);

impl StatusTarget for Screen {
    fn line(&self, line: &str) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .0
            .push(line.to_owned());
    }
    fn status(&self, status: &str) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .1
            .push(status.to_owned());
    }
    fn width(&self) -> usize {
        40
    }
    fn finish(&self) {}
}

fn src() -> SourceId {
    #[expect(clippy::unwrap_used, reason = "a fixed, valid id")]
    SourceId::new("test-galaxies").unwrap()
}

fn get(item: &str) -> Event {
    Event::Get {
        source: src(),
        item: item.into(),
        url: format!("https://example.org/{item}"),
    }
}

fn done(item: &str, bytes: u64) -> Event {
    Event::Done {
        source: src(),
        item: item.into(),
        bytes,
    }
}

fn lines(events: &[Event]) -> Vec<String> {
    let mut state = RunState::default();
    events.iter().filter_map(|e| state.apply(e)).collect()
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
        source: src(),
        item: "chunk 1 of 2".into(),
    }]);
    assert_eq!(printed, ["Hit:1 test-galaxies chunk 1 of 2"]);
}

#[test]
fn a_failed_request_prints_an_err_line_with_the_mirror_and_reason() {
    let err = |retry_in| Event::Err {
        source: src(),
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
            source: src(),
            item: "grid.npz".into(),
            from: 40,
        },
        Event::Restarted {
            source: src(),
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
            source: src(),
            item: "b".into(),
            bytes: 250,
            total: Some(1_000),
        },
    ] {
        state.apply(&e);
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
    state.apply(&Event::StageStarted {
        stage: Stage::Normalize,
        items: Some(18),
    });
    for n in 0..5 {
        state.apply(&get(&format!("a long chunk name number {n}")));
    }
    let status = state.status(Duration::from_secs(1), 40);
    assert_eq!(status.chars().count(), 40);
    assert!(
        status.ends_with('…') && status.starts_with("0% [normalize 0/18]"),
        "{status}"
    );

    let clock = ManualClock::new();
    let screen = Screen::default();
    let sink = TerminalSink::new(clock.clone(), screen.clone());
    for n in 0..5 {
        sink.emit(get(&format!("c{n}")));
    }
    clock.advance(Duration::from_millis(99));
    sink.emit(get("c5"));
    clock.advance(Duration::from_millis(1));
    sink.emit(get("c6"));
    let draws = screen
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .1
        .len();
    assert_eq!(draws, 2, "one draw at the start, one 100 ms later");
}

#[test]
fn each_event_and_stage_prints_as_one_plain_timestamped_line_without_control_codes() {
    let sink = PlainSink::new(ManualClock::new(), Vec::new());
    for e in [
        Event::StageStarted {
            stage: Stage::Fetch,
            items: Some(1),
        },
        get("chunk 1 of 1"),
        done("chunk 1 of 1", 2_048),
        Event::StageEnded {
            stage: Stage::Fetch,
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
    let clock = ManualClock::new();
    let sink = PlainSink::new(clock.clone(), Vec::new());
    sink.emit(get("grid.npz"));
    for (after, bytes) in [
        (30, 10_000_000),
        (31, 20_000_000),
        (29, 30_000_000),
        (31, 40_000_000),
    ] {
        clock.advance(Duration::from_secs(after));
        sink.emit(Event::Received {
            source: src(),
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
                source: src(),
                published: Some(1_097_100),
                left_out: Some(203),
            },
            SourceSummary {
                source: SourceId::new("cf4-basins").unwrap(),
                published: None,
                left_out: None,
            },
        ],
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
}
