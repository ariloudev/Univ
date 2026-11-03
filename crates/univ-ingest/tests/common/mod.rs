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
use univ_ingest::normalize::{Normalized, Normalizer};
use univ_ingest::source::{SourceDefinition, read_all};
use univ_model::entry::CatalogueEntry;
use univ_model::ids::SourceId;
use univ_model::redshift::{CmbCorrection, DipoleModel};

/// A clock that never waits, remembers each wait it was asked for, and moves only when a test
/// moves it. Its clones share both.
#[derive(Clone)]
pub struct TestClock {
    waits: Arc<Mutex<Vec<Duration>>>,
    now: Arc<Mutex<SystemTime>>,
}

impl Default for TestClock {
    fn default() -> Self {
        Self {
            waits: Arc::default(),
            // 2026-11-03T00:00:00Z
            now: Arc::new(Mutex::new(UNIX_EPOCH + Duration::from_hours(20_760 * 24))),
        }
    }
}

impl TestClock {
    pub fn waits(&self) -> Vec<Duration> {
        self.waits
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Moves the clock on by `by`.
    pub fn advance(&self, by: Duration) {
        *self.now.lock().unwrap_or_else(PoisonError::into_inner) += by;
    }
}

impl Clock for TestClock {
    fn sleep(&self, duration: Duration) -> impl Future<Output = ()> + Send {
        self.waits
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(duration);
        std::future::ready(())
    }

    fn now(&self) -> SystemTime {
        *self.now.lock().unwrap_or_else(PoisonError::into_inner)
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

/// The source definition `id` from the repository.
pub fn definition_of(id: &str) -> SourceDefinition {
    read_sources()
        .into_iter()
        .find(|d| d.id.as_str() == id)
        .unwrap_or_else(|| panic!("no source {id}"))
}

/// The recorded answer of a query source: `tests/fixtures/<provider>/<id>.<tsv|csv>`.
pub fn fixture_of(definition: &SourceDefinition) -> PathBuf {
    let extension = match definition.access {
        univ_ingest::source::Access::VizierTsv { .. } => "tsv",
        _ => "csv",
    };
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(&definition.provider)
        .join(format!("{}.{extension}", definition.id))
}

/// `body` normalized as an answer of `definition`, with the Planck 2018 dipole.
pub fn normalize_body(definition: &SourceDefinition, body: &[u8]) -> Normalized {
    let correction = CmbCorrection::new(DipoleModel::Planck2018);
    let mut normalizer = Normalizer::new(definition, &correction).unwrap_or_else(|e| panic!("{e}"));
    normalizer
        .table(Path::new("test"), body)
        .unwrap_or_else(|e| panic!("{e}"));
    normalizer.finish()
}

/// The recorded answer of the source `id`, normalized.
pub fn normalized_fixture(id: &str) -> Normalized {
    let definition = definition_of(id);
    let body = std::fs::read(fixture_of(&definition)).expect("a recorded answer");
    normalize_body(&definition, &body)
}

/// The entry of `normalized` whose identifier is `identifier`.
pub fn entry<'a>(normalized: &'a Normalized, identifier: &str) -> &'a CatalogueEntry {
    normalized
        .entries
        .iter()
        .find(|e| e.designation.identifier == identifier)
        .unwrap_or_else(|| panic!("no entry {identifier}"))
}

/// Whether `a` and `b` agree to `tolerance` of the larger.
pub fn near(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance * a.abs().max(b.abs()).max(1e-300)
}

/// The number of rows in the recorded answer of `definition`.
pub fn count_rows(definition: &SourceDefinition) -> u64 {
    let body = std::fs::read(fixture_of(definition)).expect("a recorded answer");
    univ_ingest::table::TableReader::new(&definition.access, &body)
        .and_then(univ_ingest::table::TableReader::count_rows)
        .expect("a table")
}
