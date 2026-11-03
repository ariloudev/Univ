//! Helpers shared by the fetch tests: a scripted clock and a fetcher that records its events.
#![allow(
    dead_code,
    clippy::expect_used,
    clippy::unnecessary_wraps,
    reason = "test helpers shared by several test crates: each uses a subset, a failed expect is \
              the test failing, and `accept` has the shape of a body check"
)]

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use univ_ingest::events::{Event, RecordingSink};
use univ_ingest::fetch::{Clock, Fetcher, RetryPolicy};
use univ_model::ids::SourceId;

/// A clock that never waits and remembers each wait it was asked for.
#[derive(Clone, Default)]
pub struct TestClock(Arc<Mutex<Vec<Duration>>>);

impl TestClock {
    pub fn waits(&self) -> Vec<Duration> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Clock for TestClock {
    fn sleep(&self, duration: Duration) -> impl Future<Output = ()> + Send {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(duration);
        std::future::ready(())
    }

    fn now(&self) -> SystemTime {
        // 2026-11-03T00:00:00Z
        UNIX_EPOCH + Duration::from_hours(20_760 * 24)
    }
}

pub struct Rig {
    pub fetcher: Fetcher<TestClock>,
    pub clock: TestClock,
    pub sink: Arc<RecordingSink>,
}

impl Rig {
    pub fn events(&self) -> Vec<Event> {
        self.sink.events()
    }
}

pub fn rig(offline: bool) -> Rig {
    let clock = TestClock::default();
    let sink = Arc::new(RecordingSink::default());
    let fetcher = Fetcher::new(clock.clone(), sink.clone(), RetryPolicy::default(), offline)
        .expect("an HTTP client");
    Rig {
        fetcher,
        clock,
        sink,
    }
}

pub fn source() -> SourceId {
    SourceId::new("test-galaxies").expect("a valid id")
}

pub fn accept(_: &[u8]) -> Result<(), String> {
    Ok(())
}
