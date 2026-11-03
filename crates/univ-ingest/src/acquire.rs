//! Acquiring a whole source: every chunk or file fetched and recorded, then checked against the
//! number of rows the source must hold, so a provider's silent row limit never passes for a
//! complete source.

use std::path::{Path, PathBuf};

use thiserror::Error;
use univ_model::ids::SourceId;

use crate::access::{check_answer, chunk_requests, count_request, read_count, read_table};
use crate::fetch::{Clock, Expected, FetchError, Fetched, Fetcher};
use crate::provenance::Provenance;
use crate::source::{Access, SourceDefinition};

/// A source on disk, ready to be read: its chunks or file, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Acquired {
    pub files: Vec<PathBuf>,
}

/// A source that could not be acquired whole. What is on disk stays for the next run.
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
}

/// Fetches every chunk or the file of `definition` into its directory under `cache`, records
/// each payload's provenance before anything parses it, and checks the rows against the
/// source's count where it has one.
///
/// # Errors
///
/// Fails when a chunk or file cannot be fetched, provenance cannot be written, a chunk cannot be
/// read, or the rows fall short of or exceed the count.
pub async fn acquire<C: Clock>(
    fetcher: &Fetcher<C>,
    definition: &SourceDefinition,
    cache: &Path,
) -> Result<Acquired, AcquireError> {
    let source = &definition.id;
    let dir = cache.join(source.as_str());
    let disk = |e: std::io::Error| AcquireError::Disk {
        source_id: source.clone(),
        error: e.to_string(),
    };
    std::fs::create_dir_all(&dir).map_err(disk)?;
    let record = |fetched: &Fetched| match fetched {
        Fetched::Arrived(payload) => Provenance::record(&dir, source, payload).map_err(disk),
        Fetched::OnDisk(_) => Ok(()),
    };

    match &definition.access {
        Access::Files { files } => {
            let mut on_disk = Vec::new();
            for file in files {
                let name = Path::new(&file.path)
                    .file_name()
                    .map_or_else(|| "file".into(), ToOwned::to_owned);
                let dest = dir.join(name);
                let expected = Expected {
                    size: file.size,
                    sha256: file.sha256.clone(),
                };
                let outcome = fetcher
                    .file(
                        source,
                        &file.path,
                        &definition.mirrors,
                        &file.path,
                        &dest,
                        &expected,
                    )
                    .await?;
                record(&outcome)?;
                on_disk.push(dest);
            }
            return Ok(Acquired { files: on_disk });
        }
        Access::Repository { path } => {
            return Ok(Acquired {
                files: vec![PathBuf::from(path)],
            });
        }
        _ => {}
    }

    let check = |body: &[u8]| check_answer(definition, body);
    let mut files = Vec::new();
    for request in chunk_requests(definition) {
        let dest = dir.join(&request.file);
        let outcome = fetcher
            .chunk(
                source,
                &request.item,
                &definition.mirrors,
                &request.suffix,
                &dest,
                &check,
            )
            .await?;
        record(&outcome)?;
        files.push(dest);
    }

    let expected = if let Some(request) = count_request(definition) {
        let dest = dir.join(&request.file);
        let count = |body: &[u8]| read_count(&definition.access, body).map(|_| ());
        let outcome = fetcher
            .chunk(
                source,
                &request.item,
                &definition.mirrors,
                &request.suffix,
                &dest,
                &count,
            )
            .await?;
        record(&outcome)?;
        let body = std::fs::read(&dest).map_err(disk)?;
        let n = read_count(&definition.access, &body)
            .map_err(|error| unreadable(source, &dest, error))?;
        Some((n, "the count query"))
    } else if let Access::VizierTsv { rows: Some(n), .. } = &definition.access {
        Some((*n, "the source definition's row count"))
    } else {
        None
    };
    check_rows(definition, &files, expected)?;
    Ok(Acquired { files })
}

/// Refuses chunks whose rows differ from the `expected` count, read on the given basis.
fn check_rows(
    definition: &SourceDefinition,
    files: &[PathBuf],
    expected: Option<(u64, &'static str)>,
) -> Result<(), AcquireError> {
    let Some((expected, basis)) = expected else {
        return Ok(());
    };
    let source = &definition.id;
    let mut found = 0;
    for file in files {
        let body = std::fs::read(file).map_err(|e| unreadable(source, file, e.to_string()))?;
        let table = read_table(&definition.access, &body)
            .map_err(|error| unreadable(source, file, error))?;
        found += table.rows.len() as u64;
    }
    if found == expected {
        Ok(())
    } else {
        Err(AcquireError::Incomplete {
            source_id: source.clone(),
            expected,
            found,
            basis,
        })
    }
}

fn unreadable(source: &SourceId, file: &Path, error: String) -> AcquireError {
    AcquireError::Unreadable {
        source_id: source.clone(),
        file: file.display().to_string(),
        error,
    }
}
