//! A single file fetched whole, resumed by byte range after a stop.

use std::path::{Path, PathBuf};

use reqwest::StatusCode;
use reqwest::header::{CONTENT_RANGE, ETAG, IF_RANGE, RANGE};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use univ_model::ids::SourceId;

use super::{
    Attempt, Clock, FetchError, Fetched, Fetcher, Item, Record, disk_error, hex, judge_status,
    part_path, with_ending,
};
use crate::events::{Event, RestartReason};

/// Progress is reported each time this many more bytes arrive.
const REPORT_EVERY: u64 = 1 << 20;

/// What a provider publishes about a file, checked once it is complete.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Expected {
    pub size: Option<u64>,
    pub sha256: Option<String>,
}

/// What is known of a partial download, kept beside it.
#[derive(Debug, Serialize, Deserialize)]
struct PartMeta {
    url: String,
    etag: Option<String>,
    length: Option<u64>,
}

/// A download that arrived whole and matched what was published: its URL, SHA-256 and size.
type Complete = (String, Vec<u8>, u64);

impl<C: Clock> Fetcher<C> {
    /// Fetches the file at `path` under each mirror in turn into the item's file. A stopped
    /// download continues by range from the byte it reached. It starts over when the provider
    /// ignores the range or the file changed, and is checked against `expected` once complete.
    /// Its bytes stay in a `.part` file until it is checked and recorded.
    ///
    /// # Errors
    ///
    /// Fails when no mirror gives the whole file, when the run is offline and the file is
    /// missing, or when it or its record cannot be written.
    pub async fn file(
        &self,
        item: &Item<'_>,
        mirrors: &[String],
        path: &str,
        expected: &Expected,
        record: &Record<'_>,
    ) -> Result<Fetched, FetchError> {
        if let Some(on_disk) = self.on_disk(item)? {
            return Ok(on_disk);
        }
        let (url, digest, bytes) = self
            .across_mirrors(item, mirrors, |mirror| {
                let url = format!(
                    "{}/{}",
                    mirror.trim_end_matches('/'),
                    path.trim_start_matches('/')
                );
                self.download(item, url, expected)
            })
            .await?;
        let payload = self.payload(item, url, &digest, bytes, None);
        let disk = |e: std::io::Error| disk_error(item, &e);
        record(&payload).map_err(disk)?;
        tokio::fs::rename(part_path(item.dest), item.dest)
            .await
            .map_err(disk)?;
        let _ = tokio::fs::remove_file(meta_path(item.dest)).await;
        self.sink.emit(Event::Done {
            source: item.source.clone(),
            item: item.name.to_owned(),
            bytes,
        });
        Ok(Fetched::Arrived(payload))
    }

    /// Brings the item's partial file to the whole file at `url` and checks it against
    /// `expected`, leaving it there to be recorded and renamed.
    async fn download(
        &self,
        item: &Item<'_>,
        url: String,
        expected: &Expected,
    ) -> Result<Complete, Attempt> {
        let at = Place {
            item: *item,
            part: part_path(item.dest),
            meta: meta_path(item.dest),
        };
        let have = tokio::fs::metadata(&at.part).await.map_or(0, |m| m.len());
        let before = if have > 0 {
            read_meta(&at.meta).await.filter(|m| m.url == url)
        } else {
            None
        };
        let mut request = self.client.get(&url);
        if let Some(m) = &before {
            request = request.header(RANGE, format!("bytes={have}-"));
            if let Some(etag) = &m.etag {
                request = request.header(IF_RANGE, etag);
            }
        }
        self.sink.emit(Event::Get {
            source: item.source.clone(),
            item: item.name.to_owned(),
            url: url.clone(),
        });
        let response = request
            .send()
            .await
            .map_err(|e| Attempt::Retry(super::with_causes(&e)))?;
        let (offset, length) = self.start(&at, &response, have, before.as_ref()).await?;
        let record = PartMeta {
            url: url.clone(),
            etag: header(&response, ETAG),
            length,
        };
        crate::yaml::write(&at.meta, &record).map_err(|e| at.disk(&e))?;
        let written = self.stream(&at, response, offset, length).await?;
        let digest = sha256_of(&at.part).await.map_err(|e| at.disk(&e))?;
        let wrong_size = expected.size.is_some_and(|s| s != written);
        let wrong_digest = expected
            .sha256
            .as_ref()
            .is_some_and(|d| !d.eq_ignore_ascii_case(&hex(&digest)));
        if wrong_size || wrong_digest {
            discard(&at).await;
            return Err(Attempt::GiveUp(format!(
                "the file of {written} bytes and SHA-256 {} does not match the published {:?} bytes and {:?}",
                hex(&digest),
                expected.size,
                expected.sha256,
            )));
        }
        Ok((url, digest, written))
    }

    /// The byte the answer starts at and the file's length, from the answer's status and how
    /// it compares with the partial download `before`, which may have to start over.
    async fn start(
        &self,
        at: &Place<'_>,
        response: &reqwest::Response,
        have: u64,
        before: Option<&PartMeta>,
    ) -> Result<(u64, Option<u64>), Attempt> {
        let etag = header(response, ETAG);
        let restart = |reason| {
            let (source, item) = at.id();
            self.sink.emit(Event::Restarted {
                source,
                item,
                reason,
            });
        };
        match (response.status(), before) {
            (StatusCode::PARTIAL_CONTENT, Some(m)) => {
                let (start, total) = header(response, CONTENT_RANGE)
                    .and_then(|r| content_range(&r))
                    .ok_or_else(|| {
                        Attempt::Retry("a 206 without a readable Content-Range".to_owned())
                    })?;
                if start != have
                    || differ(etag.as_ref(), m.etag.as_ref())
                    || differ(total.as_ref(), m.length.as_ref())
                {
                    restart(RestartReason::Changed);
                    discard(at).await;
                    return Err(Attempt::Retry(
                        "the file changed since the partial download".to_owned(),
                    ));
                }
                let (source, item) = at.id();
                self.sink.emit(Event::Resumed {
                    source,
                    item,
                    from: have,
                });
                Ok((have, total.or(m.length)))
            }
            (StatusCode::RANGE_NOT_SATISFIABLE, Some(_)) => {
                restart(RestartReason::RangeUnsatisfiable);
                discard(at).await;
                Err(Attempt::Retry("the range was not satisfiable".to_owned()))
            }
            (StatusCode::OK, before) => {
                let length = response.content_length();
                if let Some(m) = before {
                    let changed = differ(etag.as_ref(), m.etag.as_ref())
                        || differ(length.as_ref(), m.length.as_ref());
                    restart(if changed {
                        RestartReason::Changed
                    } else {
                        RestartReason::RangeIgnored
                    });
                }
                Ok((0, length))
            }
            (status, _) => {
                judge_status(response.status())?;
                Err(Attempt::GiveUp(format!("unexpected HTTP {status}")))
            }
        }
    }

    /// Writes the answer's body to the partial file from byte `offset`, keeping what arrived
    /// when the body stops short, and returns the bytes the file then holds.
    async fn stream(
        &self,
        at: &Place<'_>,
        mut response: reqwest::Response,
        offset: u64,
        length: Option<u64>,
    ) -> Result<u64, Attempt> {
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(offset > 0)
            .truncate(offset == 0)
            .open(&at.part)
            .await
            .map_err(|e| at.disk(&e))?;
        let mut written = offset;
        let mut reported = offset;
        let stopped = loop {
            match response.chunk().await {
                Ok(Some(bytes)) => {
                    file.write_all(&bytes).await.map_err(|e| at.disk(&e))?;
                    written += bytes.len() as u64;
                    if written - reported >= REPORT_EVERY {
                        reported = written;
                        let (source, item) = at.id();
                        self.sink.emit(Event::Received {
                            source,
                            item,
                            bytes: written,
                            total: length,
                        });
                    }
                }
                Ok(None) => break None,
                Err(error) => break Some(super::with_causes(&error)),
            }
        };
        file.flush().await.map_err(|e| at.disk(&e))?;
        if let Some(error) = stopped {
            return Err(Attempt::Retry(format!(
                "stopped at byte {written}: {error}"
            )));
        }
        match length {
            Some(length) if length != written => Err(Attempt::Retry(format!(
                "ended at byte {written} of {length}"
            ))),
            _ => Ok(written),
        }
    }
}

/// The item being downloaded and the files that hold it until it is whole.
struct Place<'a> {
    item: Item<'a>,
    part: PathBuf,
    meta: PathBuf,
}

impl Place<'_> {
    fn id(&self) -> (SourceId, String) {
        (self.item.source.clone(), self.item.name.to_owned())
    }

    fn disk(&self, error: &std::io::Error) -> Attempt {
        Attempt::GiveUp(format!("cannot write {}: {error}", self.part.display()))
    }
}

/// Where the record of a partial download is kept.
fn meta_path(dest: &Path) -> PathBuf {
    with_ending(dest, ".part.yaml")
}

async fn read_meta(path: &Path) -> Option<PartMeta> {
    let text = tokio::fs::read_to_string(path).await.ok()?;
    serde_norway::from_str(&text).ok()
}

/// Removes a partial download and its record, so the next attempt starts from the first byte.
async fn discard(at: &Place<'_>) {
    let _ = tokio::fs::remove_file(&at.part).await;
    let _ = tokio::fs::remove_file(&at.meta).await;
}

/// Both known and different.
fn differ<T: PartialEq>(now: Option<&T>, before: Option<&T>) -> bool {
    matches!((now, before), (Some(a), Some(b)) if a != b)
}

fn header(response: &reqwest::Response, name: reqwest::header::HeaderName) -> Option<String> {
    response
        .headers()
        .get(name)?
        .to_str()
        .ok()
        .map(str::to_owned)
}

/// The first byte and total length of `bytes 40-99/100`; the total is `None` for `*`.
fn content_range(value: &str) -> Option<(u64, Option<u64>)> {
    let (range, total) = value.strip_prefix("bytes ")?.split_once('/')?;
    let start = range.split_once('-')?.0.trim().parse().ok()?;
    Some((start, total.trim().parse().ok()))
}

async fn sha256_of(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 1 << 20];
    loop {
        let n = file.read(&mut buffer).await?;
        if n == 0 {
            return Ok(hasher.finalize().to_vec());
        }
        hasher.update(&buffer[..n]);
    }
}

#[cfg(test)]
mod tests {
    use super::content_range;

    #[test]
    fn a_content_range_gives_its_first_byte_and_total() {
        assert_eq!(content_range("bytes 40-99/100"), Some((40, Some(100))));
        assert_eq!(content_range("bytes 0-9/*"), Some((0, None)));
        assert_eq!(content_range("items 0-9/10"), None);
    }
}
