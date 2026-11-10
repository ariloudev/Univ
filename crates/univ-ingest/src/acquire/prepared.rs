//! A prepared source: one gzipped CSV file that a script of this repository made, found in the
//! source's download directory and checked against its definition before it is read. Nothing
//! is fetched.

use std::io::{BufRead as _, BufReader};
use std::path::{Path, PathBuf};

use flate2::read::MultiGzDecoder;

use super::{AcquireError, disk};
use crate::fetch::Payload;
use crate::normalize::is_layout;
use crate::provenance::Provenance;
use crate::source::{PreparedFile, SourceDefinition};
use crate::tiles::sha256;

/// Finds the prepared file in `dir`, checks its SHA-256 and rows against `prepared`, and
/// records it in the source's provenance, with the time it was last written as its retrieval
/// time, unless the provenance already records these bytes.
pub(super) fn acquire_prepared(
    definition: &SourceDefinition,
    dir: &Path,
    prepared: &PreparedFile,
) -> Result<PathBuf, AcquireError> {
    let source = &definition.id;
    let path = dir.join(&prepared.file);
    let unreadable = |error: String| AcquireError::Unreadable {
        source_id: source.clone(),
        file: path.display().to_string(),
        error,
    };
    let metadata = match std::fs::metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(AcquireError::NotPrepared {
                source_id: source.clone(),
                path,
                script: prepared.script.clone(),
            });
        }
        other => other.map_err(|e| unreadable(e.to_string()))?,
    };
    let (digest, rows) = measure(&path).map_err(|e| unreadable(e.to_string()))?;
    let differs = |what, expected: String, found: String| AcquireError::PreparedDiffers {
        source_id: source.clone(),
        path: path.clone(),
        what,
        expected,
        found,
    };
    if !digest.eq_ignore_ascii_case(&prepared.sha256) {
        return Err(differs("SHA-256", prepared.sha256.clone(), digest));
    }
    if rows != prepared.rows {
        return Err(differs(
            "row count",
            prepared.rows.to_string(),
            rows.to_string(),
        ));
    }
    let known = Provenance::read(dir).map_err(|e| disk(source, &e))?;
    let recorded = known.payload(&prepared.file).is_some_and(|p| {
        p.sha256 == digest
            && p.bytes == metadata.len()
            && p.prepared_by.as_deref() == Some(prepared.script.as_str())
    });
    if !recorded {
        // The file's own time is when the script made it; a file system that keeps none gives
        // no retrieval time to record, and no time is made up for it.
        let written = metadata
            .modified()
            .map_err(|e| unreadable(format!("its modification time cannot be read: {e}")))?;
        let payload = Payload {
            item: prepared.file.clone(),
            url: definition.mirrors.first().cloned().unwrap_or_default(),
            sha256: digest,
            bytes: metadata.len(),
            retrieved: humantime::format_rfc3339_seconds(written).to_string(),
            rows: Some(rows),
            prepared_by: Some(prepared.script.clone()),
        };
        Provenance::record(dir, source, &payload).map_err(|e| disk(source, &e))?;
    }
    Ok(path)
}

/// The SHA-256 of the file, in hexadecimal, and its rows: the lines of its gzipped CSV after
/// the header that are not layout, as the CSV reader takes them.
fn measure(path: &Path) -> std::io::Result<(String, u64)> {
    let bytes = std::fs::read(path)?;
    let mut table_lines = 0_u64;
    for line in BufReader::new(MultiGzDecoder::new(&bytes[..])).split(b'\n') {
        if !is_layout(&line?) {
            table_lines += 1;
        }
    }
    Ok((sha256(&bytes), table_lines.saturating_sub(1)))
}
