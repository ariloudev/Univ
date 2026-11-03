//! Helpers shared by the fetch tests: a scripted clock and a fetcher that records its events.
#![allow(
    dead_code,
    clippy::expect_used,
    clippy::panic,
    clippy::unnecessary_wraps,
    reason = "test helpers shared by several test crates: each uses a subset, a failed expect or \
              panic is the test failing, and `accept` and `keep` have the shapes of the hooks"
)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use univ_ingest::events::{Event, RecordingSink};
use univ_ingest::fetch::{Clock, Fetcher, Item, Payload, RetryPolicy};
use univ_ingest::source::{SourceDefinition, read_all};
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

static SOURCE: LazyLock<SourceId> =
    LazyLock::new(|| SourceId::new("test-galaxies").expect("a valid id"));

pub fn source() -> SourceId {
    SOURCE.clone()
}

/// The item `name` of `test-galaxies`, kept at `dest`.
pub fn item<'a>(name: &'a str, dest: &'a Path) -> Item<'a> {
    Item {
        source: &SOURCE,
        name,
        dest,
    }
}

/// A body check that takes any body, measuring it as no rows.
pub fn accept(_: &[u8]) -> Result<u64, String> {
    Ok(0)
}

/// A provenance hook that records nothing.
pub fn keep(_: &Payload) -> std::io::Result<()> {
    Ok(())
}

/// The repository's `sources/` directory.
pub fn sources_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../sources")
}

/// Every source definition in the repository.
pub fn read_sources() -> Vec<SourceDefinition> {
    read_all(&sources_dir()).unwrap_or_else(|e| panic!("{e}"))
}

/// A definition of `test-galaxies` with `access`, columns `ra` and `dec`, and one mirror,
/// written to `dir` and read as a run reads it.
pub fn definition(dir: &tempfile::TempDir, access: &str, mirror: &str) -> SourceDefinition {
    let yaml = format!(
        "id: test-galaxies\ntitle: t\nprovider: p\naccess:\n{access}\ncuts: []\ncolumns:\n  - {{ column: ra, field: right_ascension, unit: degree }}\n  - {{ column: dec, field: declination, unit: degree }}\nframe: icrs\nepoch: 2000.0\ncitation: c\nterms: {{ url: u, quote: q, credit: c }}\nredistribution: {{ status: permitted, reasoning: r }}\nscale_domain: cosmic_web\nmirrors: [\"{mirror}\"]\n"
    );
    let path = dir.path().join("test-galaxies.yaml");
    std::fs::write(&path, yaml).expect("written");
    SourceDefinition::read(&path).expect("a valid definition")
}
