//! The durable fetcher: retries with doubling waits, mirrors skipped once they fail, chunks kept
//! on disk through an atomic rename, and an offline mode that makes no request.

mod file;

use std::collections::HashSet;
use std::fmt::Write as _;
use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use univ_model::ids::SourceId;

use crate::events::{Event, EventSink};

pub use file::Expected;

/// How often and how patiently a request is retried on one mirror.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Retries after the first attempt.
    pub retries: u32,
    /// The wait before the first retry; each later wait is twice the one before.
    pub first_wait: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            retries: 4,
            first_wait: Duration::from_secs(2),
        }
    }
}

/// Time for the fetcher: real in a run, scripted in tests.
pub trait Clock: Send + Sync {
    fn sleep(&self, duration: Duration) -> impl Future<Output = ()> + Send;
    fn now(&self) -> SystemTime;
}

/// The system's clock.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn sleep(&self, duration: Duration) -> impl Future<Output = ()> + Send {
        tokio::time::sleep(duration)
    }

    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// A payload that arrived, as the source's provenance records it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Payload {
    pub item: String,
    pub url: String,
    pub sha256: String,
    pub bytes: u64,
    /// The retrieval time, RFC 3339 in UTC.
    pub retrieved: String,
    /// What the body check measured: the rows of a table chunk, or the number a count query
    /// gave. None for a file, which has no body check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<u64>,
}

/// What a fetch came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fetched {
    /// The item arrived now.
    Arrived(Payload),
    /// The item was already on disk.
    OnDisk,
}

/// An item of a source and the file it is kept in.
#[derive(Clone, Copy, Debug)]
pub struct Item<'a> {
    pub source: &'a SourceId,
    /// The item's name in events and provenance.
    pub name: &'a str,
    pub dest: &'a Path,
}

/// A fetch that failed. Whatever is already on disk stays there for the next run.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum FetchError {
    #[error("{source_id}: {item} could not be fetched from any mirror: {}", list(.errors))]
    AllMirrorsFailed {
        source_id: SourceId,
        item: String,
        errors: Vec<(String, String)>,
    },
    #[error("{source_id}: {item} is not on disk, and an offline run fetches nothing")]
    MissingOffline { source_id: SourceId, item: String },
    #[error("{source_id}: {item}: {error}")]
    Disk {
        source_id: SourceId,
        item: String,
        error: String,
    },
}

fn list(errors: &[(String, String)]) -> String {
    errors
        .iter()
        .map(|(mirror, error)| format!("{mirror}: {error}"))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Why one attempt failed.
#[derive(Debug)]
pub(crate) enum Attempt {
    /// Worth trying again on the same mirror, such as a timeout or a 5xx.
    Retry(String),
    /// Not worth trying again on this mirror, such as a 404, a body that fails its check or a
    /// file that does not match its published digest. The next mirror is tried, and this one is
    /// not skipped for the rest of the run.
    GiveUp(String),
}

/// Checks a body before it is kept, such as a provider's error page sent with status 200, and
/// gives its measure: the rows of a table chunk, or the number a count query gave.
pub type Check<'a> = dyn Fn(&[u8]) -> Result<u64, String> + Send + Sync + 'a;

/// Records a payload's provenance once every check has passed and before its file is renamed
/// into place, so that no payload is ever visible without its record. An error fails the item.
pub type Record<'a> = dyn Fn(&Payload) -> io::Result<()> + Send + Sync + 'a;

/// Fetches the chunks and files of sources.
pub struct Fetcher<C: Clock> {
    client: reqwest::Client,
    clock: C,
    sink: Arc<dyn EventSink>,
    policy: RetryPolicy,
    offline: bool,
    skipped: Mutex<HashSet<String>>,
}

impl<C: Clock> Fetcher<C> {
    /// # Errors
    ///
    /// Fails when the HTTP client cannot be built.
    pub fn new(
        clock: C,
        sink: Arc<dyn EventSink>,
        policy: RetryPolicy,
        offline: bool,
    ) -> Result<Self, reqwest::Error> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("univ-ingest/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(30))
            .read_timeout(Duration::from_secs(120))
            .build()?;
        Ok(Self {
            client,
            clock,
            sink,
            policy,
            offline,
            skipped: Mutex::default(),
        })
    }

    /// Fetches one chunk of a query: `suffix` is appended to each mirror in turn. The chunk is
    /// written to its file with `.part` added, recorded, then renamed, so a partial chunk is
    /// never read as one. A chunk already on disk is not fetched again.
    ///
    /// # Errors
    ///
    /// Fails when no mirror gives a body that passes `check`, when the run is offline and the
    /// chunk is missing, or when the chunk or its record cannot be written.
    pub async fn chunk(
        &self,
        item: &Item<'_>,
        mirrors: &[String],
        suffix: &str,
        check: &Check<'_>,
        record: &Record<'_>,
    ) -> Result<Fetched, FetchError> {
        if let Some(on_disk) = self.on_disk(item)? {
            return Ok(on_disk);
        }
        let (url, body, rows) = self
            .across_mirrors(item, mirrors, |mirror| {
                self.get_chunk(item, format!("{mirror}{suffix}"), check)
            })
            .await?;
        let bytes = body.len() as u64;
        let payload = self.payload(item, url, &Sha256::digest(&body), bytes, Some(rows));
        write_via_part(item.dest, &body, || record(&payload)).map_err(|e| disk_error(item, &e))?;
        self.sink.emit(Event::Done {
            source: item.source.clone(),
            item: item.name.to_owned(),
            bytes,
        });
        Ok(Fetched::Arrived(payload))
    }

    /// `Fetched::OnDisk` when the item's file is there; an error when the run is offline and
    /// it is not.
    fn on_disk(&self, item: &Item<'_>) -> Result<Option<Fetched>, FetchError> {
        if item.dest.is_file() {
            self.sink.emit(Event::Hit {
                source: item.source.clone(),
                item: item.name.to_owned(),
            });
            return Ok(Some(Fetched::OnDisk));
        }
        if self.offline {
            return Err(FetchError::MissingOffline {
                source_id: item.source.clone(),
                item: item.name.to_owned(),
            });
        }
        Ok(None)
    }

    async fn get_chunk(
        &self,
        item: &Item<'_>,
        url: String,
        check: &Check<'_>,
    ) -> Result<(String, Vec<u8>, u64), Attempt> {
        self.sink.emit(Event::Get {
            source: item.source.clone(),
            item: item.name.to_owned(),
            url: url.clone(),
        });
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| Attempt::Retry(e.to_string()))?;
        judge_status(response.status())?;
        let body = response
            .bytes()
            .await
            .map_err(|e| Attempt::Retry(e.to_string()))?;
        let rows = check(&body).map_err(Attempt::GiveUp)?;
        Ok((url, body.into(), rows))
    }

    /// Tries `attempt` on each mirror in order, with retries, skipping mirrors that failed all
    /// their retries earlier in the run.
    async fn across_mirrors<T, F, Fut>(
        &self,
        item: &Item<'_>,
        mirrors: &[String],
        attempt: F,
    ) -> Result<T, FetchError>
    where
        F: Fn(String) -> Fut,
        Fut: Future<Output = Result<T, Attempt>>,
    {
        let mut errors = Vec::new();
        for mirror in mirrors {
            if self.is_skipped(mirror) {
                errors.push((
                    mirror.clone(),
                    "skipped: it failed all its retries earlier in this run".to_owned(),
                ));
                continue;
            }
            let mut wait = self.policy.first_wait;
            let mut last = String::new();
            let mut exhausted = true;
            for n in 0..=self.policy.retries {
                let (error, retry) = match attempt(mirror.clone()).await {
                    Ok(value) => return Ok(value),
                    Err(Attempt::Retry(error)) => (error, n < self.policy.retries),
                    Err(Attempt::GiveUp(error)) => {
                        exhausted = false;
                        (error, false)
                    }
                };
                self.sink.emit(Event::Err {
                    source: item.source.clone(),
                    item: item.name.to_owned(),
                    mirror: mirror.clone(),
                    attempt: n + 1,
                    error: error.clone(),
                    retry_in: retry.then_some(wait),
                });
                last = error;
                if !retry {
                    break;
                }
                self.clock.sleep(wait).await;
                wait *= 2;
            }
            if exhausted {
                self.skip(mirror);
                self.sink.emit(Event::MirrorSkipped {
                    source: item.source.clone(),
                    mirror: mirror.clone(),
                });
            }
            errors.push((mirror.clone(), last));
        }
        Err(FetchError::AllMirrorsFailed {
            source_id: item.source.clone(),
            item: item.name.to_owned(),
            errors,
        })
    }

    fn is_skipped(&self, mirror: &str) -> bool {
        self.skipped
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(mirror)
    }

    fn skip(&self, mirror: &str) {
        self.skipped
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(mirror.to_owned());
    }

    fn payload(
        &self,
        item: &Item<'_>,
        url: String,
        digest: &[u8],
        bytes: u64,
        rows: Option<u64>,
    ) -> Payload {
        Payload {
            item: item.name.to_owned(),
            url,
            sha256: hex(digest),
            bytes,
            retrieved: humantime::format_rfc3339_seconds(self.clock.now()).to_string(),
            rows,
        }
    }
}

/// A status that is not success, as an attempt worth retrying or not.
fn judge_status(status: reqwest::StatusCode) -> Result<(), Attempt> {
    if status.is_success() {
        Ok(())
    } else if status.is_server_error()
        || status == StatusCode::REQUEST_TIMEOUT
        || status == StatusCode::TOO_MANY_REQUESTS
    {
        Err(Attempt::Retry(format!("HTTP {status}")))
    } else {
        Err(Attempt::GiveUp(format!("HTTP {status}")))
    }
}

/// `dest` with `ending`, such as `.part`, added to its name.
fn with_ending(dest: &Path, ending: &str) -> PathBuf {
    let mut name = dest.as_os_str().to_owned();
    name.push(ending);
    PathBuf::from(name)
}

/// `dest` with `.part` added to its name.
fn part_path(dest: &Path) -> PathBuf {
    with_ending(dest, ".part")
}

/// Writes `bytes` to `dest` through `dest.part` and a rename, so that `dest` is never half
/// written. `before` runs once the partial file is whole; an error from it stops the rename.
pub(crate) fn write_via_part(
    dest: &Path,
    bytes: &[u8],
    before: impl FnOnce() -> io::Result<()>,
) -> io::Result<()> {
    let part = part_path(dest);
    std::fs::write(&part, bytes)?;
    before()?;
    std::fs::rename(part, dest)
}

fn disk_error(item: &Item<'_>, error: &io::Error) -> FetchError {
    FetchError::Disk {
        source_id: item.source.clone(),
        item: item.name.to_owned(),
        error: error.to_string(),
    }
}

/// Lower-case hexadecimal.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        })
}
