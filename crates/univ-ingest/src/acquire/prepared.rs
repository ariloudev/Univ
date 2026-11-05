//! A prepared source: one gzipped CSV file that a script of this repository made, found in the
//! source's download directory and checked against its definition before it is read. Nothing
//! is fetched.

use std::io::{BufRead as _, BufReader};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use flate2::read::MultiGzDecoder;
use sha2::{Digest as _, Sha256};

use super::{AcquireError, disk};
use crate::fetch::{Payload, hex};
use crate::provenance::Provenance;
use crate::source::SourceDefinition;

/// What a prepared source's definition says of its file.
pub(super) struct Prepared<'a> {
    pub file: &'a str,
    pub script: &'a str,
    pub sha256: &'a str,
    pub rows: u64,
}

/// Finds the prepared file in `dir`, checks its SHA-256 and rows against `prepared`, and
/// records it in the source's provenance, with the time it was last written as its retrieval
/// time.
pub(super) fn acquire_prepared(
    definition: &SourceDefinition,
    dir: &Path,
    prepared: &Prepared<'_>,
) -> Result<Vec<PathBuf>, AcquireError> {
    let source = &definition.id;
    let path = dir.join(prepared.file);
    let unreadable = |e: &std::io::Error| AcquireError::Unreadable {
        source_id: source.clone(),
        file: path.display().to_string(),
        error: e.to_string(),
    };
    let metadata = match std::fs::metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(AcquireError::NotPrepared {
                source_id: source.clone(),
                path,
                script: prepared.script.to_owned(),
            });
        }
        other => other.map_err(|e| unreadable(&e))?,
    };
    let (digest, rows) = measure(&path).map_err(|e| unreadable(&e))?;
    let differs = |what, expected: String, found: String| AcquireError::PreparedDiffers {
        source_id: source.clone(),
        path: path.clone(),
        what,
        expected,
        found,
    };
    if !digest.eq_ignore_ascii_case(prepared.sha256) {
        return Err(differs("SHA-256", prepared.sha256.to_owned(), digest));
    }
    if rows != prepared.rows {
        return Err(differs(
            "row count",
            prepared.rows.to_string(),
            rows.to_string(),
        ));
    }
    let written = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    let payload = Payload {
        item: prepared.file.to_owned(),
        url: definition.mirrors.first().cloned().unwrap_or_default(),
        sha256: digest,
        bytes: metadata.len(),
        retrieved: humantime::format_rfc3339_seconds(written).to_string(),
        rows: Some(rows),
        prepared_by: Some(prepared.script.to_owned()),
    };
    let known = Provenance::read(dir).map_err(|e| disk(source, &e))?;
    if known.payload(prepared.file) != Some(&payload) {
        Provenance::record(dir, source, &payload).map_err(|e| disk(source, &e))?;
    }
    Ok(vec![path])
}

/// The SHA-256 of the file, in hexadecimal, and its rows: the lines of its gzipped CSV after
/// the header that are neither blank nor `#` comments, as the CSV reader takes them.
fn measure(path: &Path) -> std::io::Result<(String, u64)> {
    let bytes = std::fs::read(path)?;
    let digest = Sha256::digest(&bytes);
    let mut lines = BufReader::new(MultiGzDecoder::new(&bytes[..]));
    let mut line = Vec::new();
    let mut table_lines = 0_u64;
    loop {
        line.clear();
        if lines.read_until(b'\n', &mut line)? == 0 {
            break;
        }
        if !(line.starts_with(b"#") || line.iter().all(u8::is_ascii_whitespace)) {
            table_lines += 1;
        }
    }
    Ok((hex(&digest), table_lines.saturating_sub(1)))
}
