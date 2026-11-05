//! Acquiring a whole source: every chunk or file fetched and recorded, then checked against the
//! number of rows the source must hold, so a provider's silent row limit never passes for a
//! complete source.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};

use futures_util::{StreamExt as _, stream};
use thiserror::Error;
use univ_model::ids::SourceId;

use crate::access::{ChunkRequest, check_answer, chunk_requests, count_request, read_count};
use crate::fetch::{Check, Clock, Expected, FetchError, Fetched, Fetcher, Item, Payload};
use crate::provenance::Provenance;
use crate::source::{Access, RemoteFile, SourceDefinition};

/// Chunks of one source asked for at once: enough to hide a slow provider's latency, few
/// enough to stay polite.
const CHUNKS_AT_ONCE: usize = 3;

/// A source on disk, ready to be read: its chunks or files, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Acquired {
    pub files: Vec<PathBuf>,
}

/// A source that could not be acquired whole. What is on disk stays for the next run, except
/// the chunks of a source found incomplete.
#[derive(Debug, Error)]
pub enum AcquireError {
    #[error(transparent)]
    Fetch(#[from] FetchError),
    #[error("{source_id}: {error}")]
    Disk { source_id: SourceId, error: String },
    #[error("{source_id}: {file} cannot be read: {error}")]
    Unreadable {
        source_id: SourceId,
        file: String,
        error: String,
    },
    #[error(
        "{source_id}: the chunks hold {found} rows but {basis} gives {expected}, so the source is \
         incomplete and no release may hold it"
    )]
    Incomplete {
        source_id: SourceId,
        expected: u64,
        found: u64,
        basis: &'static str,
    },
    #[error("{source_id}: {} is not in the repository", path.display())]
    NotInRepository { source_id: SourceId, path: PathBuf },
}

/// Fetches every chunk or file of `definition` into its directory under `cache`, records each
/// payload's provenance before it is renamed into place, and checks the rows against the
/// source's count where it has one. A source kept in the repository is found under
/// `repository`.
///
/// # Errors
///
/// Fails when a chunk or file cannot be fetched, provenance cannot be written, the rows fall
/// short of or exceed the count, or a repository source's path does not exist.
pub async fn acquire<C: Clock>(
    fetcher: &Fetcher<C>,
    definition: &SourceDefinition,
    cache: &Path,
    repository: &Path,
) -> Result<Acquired, AcquireError> {
    let source = &definition.id;
    if let Access::Repository { path, .. } = &definition.access {
        let path = repository.join(path);
        if !path.exists() {
            return Err(AcquireError::NotInRepository {
                source_id: source.clone(),
                path,
            });
        }
        return Ok(Acquired { files: vec![path] });
    }
    let dir = cache.join(source.as_str());
    std::fs::create_dir_all(&dir).map_err(|e| disk(source, &e))?;
    let known = Provenance::read(&dir).map_err(|e| disk(source, &e))?;
    let run = Run {
        fetcher,
        definition,
        dir,
        provenance: Mutex::new(known.clone()),
        known,
    };
    let files = match &definition.access {
        Access::Files { files } => acquire_files(&run, files).await?,
        _ => acquire_chunks(&run).await?,
    };
    Ok(Acquired { files })
}

/// The acquisition of one source: its definition, directory and provenance as the run began.
struct Run<'a, C: Clock> {
    fetcher: &'a Fetcher<C>,
    definition: &'a SourceDefinition,
    dir: PathBuf,
    known: Provenance,
    /// The provenance as it stands, written whole after each change; held while it is
    /// changed and written, as chunks arrive together.
    provenance: Mutex<Provenance>,
}

/// Fetches each file of a file source. A file on disk without a record, or whose record or size
/// differs from what its provider publishes, is fetched again.
async fn acquire_files<C: Clock>(
    run: &Run<'_, C>,
    files: &[RemoteFile],
) -> Result<Vec<PathBuf>, AcquireError> {
    let mut on_disk = Vec::new();
    for file in files {
        let name = Path::new(&file.path)
            .file_name()
            .map_or_else(|| "file".into(), ToOwned::to_owned);
        let dest = run.dir.join(name);
        let current = run.known.payload(&file.path).is_some_and(|p| {
            file.sha256
                .as_ref()
                .is_none_or(|d| d.eq_ignore_ascii_case(&p.sha256))
        }) && file
            .size
            .is_none_or(|s| std::fs::metadata(&dest).is_ok_and(|m| m.len() == s));
        if !current {
            run.remove(&dest)?;
        }
        let expected = Expected {
            size: file.size,
            sha256: file.sha256.clone(),
        };
        run.fetcher
            .file(
                &run.item(&file.path, &dest),
                &run.definition.mirrors,
                &file.path,
                &expected,
                &|p| run.record(p),
            )
            .await?;
        on_disk.push(dest);
    }
    run.prune(&on_disk)?;
    Ok(on_disk)
}

/// Fetches each chunk of a query source and its count, and checks the rows the chunks hold, as
/// their records give them, against the count. Chunks of a source found incomplete are removed
/// so that the next run fetches them again.
async fn acquire_chunks<C: Clock>(run: &Run<'_, C>) -> Result<Vec<PathBuf>, AcquireError> {
    let definition = run.definition;
    let check = |body: &[u8]| check_answer(definition, body);
    let requests = chunk_requests(definition);
    // After the first failure no further chunk starts, but the chunks in flight finish, so
    // that their events and records land before the error is returned.
    let failed = AtomicBool::new(false);
    let outcomes: Vec<Option<Result<(PathBuf, u64), AcquireError>>> = stream::iter(&requests)
        .map(|request| {
            let failed = &failed;
            let check = &check;
            async move {
                if failed.load(Ordering::Relaxed) {
                    return None;
                }
                let outcome = run.measured(request, check).await;
                if outcome.is_err() {
                    failed.store(true, Ordering::Relaxed);
                }
                Some(outcome)
            }
        })
        .buffered(CHUNKS_AT_ONCE)
        .collect()
        .await;
    let fetched = outcomes
        .into_iter()
        .flatten()
        .collect::<Result<Vec<_>, _>>()?;
    let found = fetched.iter().map(|(_, rows)| rows).sum();
    let files: Vec<PathBuf> = fetched.into_iter().map(|(dest, _)| dest).collect();
    let mut kept = files.clone();
    let expected = if let Some(request) = count_request(definition) {
        let count = |body: &[u8]| read_count(&definition.access, body);
        let (dest, n) = run.measured(&request, &count).await?;
        kept.push(dest);
        Some((n, "the count query"))
    } else if let Access::VizierTsv { rows: Some(n), .. } = &definition.access {
        Some((*n, "the source definition's row count"))
    } else {
        None
    };
    if let Some((expected, basis)) = expected
        && expected != found
    {
        for file in &kept {
            run.remove(file)?;
        }
        return Err(AcquireError::Incomplete {
            source_id: definition.id.clone(),
            expected,
            found,
            basis,
        });
    }
    run.prune(&kept)?;
    Ok(files)
}

impl<C: Clock> Run<'_, C> {
    fn item<'a>(&'a self, name: &'a str, dest: &'a Path) -> Item<'a> {
        Item {
            source: &self.definition.id,
            name,
            dest,
        }
    }

    fn record(&self, payload: &Payload) -> std::io::Result<()> {
        let mut provenance = self
            .provenance
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        provenance.recorded(&self.definition.id, payload);
        provenance.write(&self.dir)
    }

    /// Fetches `request` unless its file is on disk with a record of the same request, and
    /// gives the file and the measure its body check found, from the check or the record.
    async fn measured(
        &self,
        request: &ChunkRequest,
        check: &Check<'_>,
    ) -> Result<(PathBuf, u64), AcquireError> {
        let dest = self.dir.join(&request.file);
        let recorded = self
            .known
            .payload(&request.item)
            .filter(|p| p.url.ends_with(&request.ask.suffix()))
            .and_then(|p| p.rows);
        if recorded.is_none() {
            self.remove(&dest)?;
        }
        let fetched = self
            .fetcher
            .chunk(
                &self.item(&request.item, &dest),
                &self.definition.mirrors,
                &request.ask,
                check,
                &|p| self.record(p),
            )
            .await?;
        match (fetched, recorded) {
            (Fetched::Arrived(Payload { rows: Some(n), .. }), _) | (Fetched::OnDisk, Some(n)) => {
                Ok((dest, n))
            }
            _ => Err(AcquireError::Unreadable {
                source_id: self.definition.id.clone(),
                file: dest.display().to_string(),
                error: "no row count is recorded for it".to_owned(),
            }),
        }
    }

    /// Removes `file` if it is there.
    fn remove(&self, file: &Path) -> Result<(), AcquireError> {
        match std::fs::remove_file(file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(disk(&self.definition.id, &e))
            }
            _ => Ok(()),
        }
    }

    /// Removes the `chunk-*` files of the source's directory that are not `kept`, such as the
    /// chunks of a query the definition no longer makes. No other file is touched.
    fn prune(&self, kept: &[PathBuf]) -> Result<(), AcquireError> {
        let kept: HashSet<&OsStr> = kept.iter().filter_map(|p| p.file_name()).collect();
        let entries = std::fs::read_dir(&self.dir).map_err(|e| disk(&self.definition.id, &e))?;
        for entry in entries {
            let entry = entry.map_err(|e| disk(&self.definition.id, &e))?;
            let name = entry.file_name();
            let chunk = name.to_str().is_some_and(|n| n.starts_with("chunk-"));
            if chunk && !kept.contains(name.as_os_str()) && entry.path().is_file() {
                self.remove(&entry.path())?;
            }
        }
        Ok(())
    }
}

fn disk(source: &SourceId, error: &std::io::Error) -> AcquireError {
    AcquireError::Disk {
        source_id: source.clone(),
        error: error.to_string(),
    }
}
