//! The durable fetcher: retries with doubling waits, mirrors skipped once they fail, chunks kept
//! on disk through an atomic rename, and an offline mode that makes no request.

mod file;

use std::collections::HashSet;
use std::fmt::Write as _;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Payload {
    pub item: String,
    pub url: String,
    pub sha256: String,
    pub bytes: u64,
    /// The retrieval time, RFC 3339 in UTC.
    pub retrieved: String,
}

/// What a fetch came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fetched {
    /// The item arrived now.
    Arrived(Payload),
    /// The item was already on disk.
    OnDisk(PathBuf),
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
    /// Not worth trying again on this mirror, such as a 404.
    GiveUp(String),
}

/// Checks a body before it is kept, such as a provider's error page sent with status 200.
pub type Validate<'a> = dyn Fn(&[u8]) -> Result<(), String> + Send + Sync + 'a;

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
    /// written to `dest` with `.part` added, then renamed, so a partial chunk is never read as
    /// one. A chunk already at `dest` is not fetched again.
    ///
    /// # Errors
    ///
    /// Fails when no mirror gives a body that passes `validate`, when the run is offline and the
    /// chunk is missing, or when the chunk cannot be written.
    pub async fn chunk(
        &self,
        source: &SourceId,
        item: &str,
        mirrors: &[String],
        suffix: &str,
        dest: &Path,
        validate: &Validate<'_>,
    ) -> Result<Fetched, FetchError> {
        if let Some(on_disk) = self.on_disk(source, item, dest)? {
            return Ok(on_disk);
        }
        let (url, body) = self
            .across_mirrors(source, item, mirrors, |mirror| {
                self.get_chunk(source, item, format!("{mirror}{suffix}"), validate)
            })
            .await?;
        let disk = |e: std::io::Error| disk_error(source, item, &e);
        let part = part_path(dest);
        tokio::fs::write(&part, &body).await.map_err(disk)?;
        tokio::fs::rename(&part, dest).await.map_err(disk)?;
        let bytes = body.len() as u64;
        self.sink.emit(Event::Done {
            source: source.clone(),
            item: item.to_owned(),
            bytes,
        });
        Ok(Fetched::Arrived(self.payload(
            item,
            url,
            &Sha256::digest(&body),
            bytes,
        )))
    }

    /// The item at `dest` when it is there; an error when the run is offline and it is not.
    fn on_disk(
        &self,
        source: &SourceId,
        item: &str,
        dest: &Path,
    ) -> Result<Option<Fetched>, FetchError> {
        if dest.is_file() {
            self.sink.emit(Event::Hit {
                source: source.clone(),
                item: item.to_owned(),
            });
            return Ok(Some(Fetched::OnDisk(dest.to_path_buf())));
        }
        if self.offline {
            return Err(FetchError::MissingOffline {
                source_id: source.clone(),
                item: item.to_owned(),
            });
        }
        Ok(None)
    }

    async fn get_chunk(
        &self,
        source: &SourceId,
        item: &str,
        url: String,
        validate: &Validate<'_>,
    ) -> Result<(String, Vec<u8>), Attempt> {
        self.sink.emit(Event::Get {
            source: source.clone(),
            item: item.to_owned(),
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
        validate(&body).map_err(Attempt::Retry)?;
        Ok((url, body.to_vec()))
    }

    /// Tries `attempt` on each mirror in order, with retries, skipping mirrors that failed all
    /// their retries earlier in the run.
    async fn across_mirrors<T, F, Fut>(
        &self,
        source: &SourceId,
        item: &str,
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
                    source: source.clone(),
                    item: item.to_owned(),
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
                    source: source.clone(),
                    mirror: mirror.clone(),
                });
            }
            errors.push((mirror.clone(), last));
        }
        Err(FetchError::AllMirrorsFailed {
            source_id: source.clone(),
            item: item.to_owned(),
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

    fn payload(&self, item: &str, url: String, digest: &[u8], bytes: u64) -> Payload {
        Payload {
            item: item.to_owned(),
            url,
            sha256: hex(digest),
            bytes,
            retrieved: humantime::format_rfc3339_seconds(self.clock.now()).to_string(),
        }
    }
}

/// A status that is not success, as an attempt worth retrying or not.
fn judge_status(status: reqwest::StatusCode) -> Result<(), Attempt> {
    if status.is_success() {
        Ok(())
    } else if status.is_server_error() || status.as_u16() == 408 || status.as_u16() == 429 {
        Err(Attempt::Retry(format!("HTTP {status}")))
    } else {
        Err(Attempt::GiveUp(format!("HTTP {status}")))
    }
}

/// `dest` with `.part` added to its name.
fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    dest.with_file_name(name)
}

fn disk_error(source: &SourceId, item: &str, error: &std::io::Error) -> FetchError {
    FetchError::Disk {
        source_id: source.clone(),
        item: item.to_owned(),
        error: error.to_string(),
    }
}

/// Lower-case hexadecimal.
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        })
}
