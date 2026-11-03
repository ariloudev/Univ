//! Pruning release n: its R2 files and its rows of the tables of one release deleted, and its
//! pruning time recorded in the list of releases, which keeps it (AC-100), beside the id
//! ledger, which is never pruned (AC-99). The release `latest` points to is refused (AC-101).
//!
//! The steps can run again: R2 files are deleted first, the manifest last among them, then
//! the rows and the pruning time in one file, the time last; an interrupted prune is finished by
//! the next, which keeps the first pruning time recorded.

use std::fmt;
use std::path::Path;

use univ_model::release::manifest::Target;
use univ_model::release::{ReleaseManifest, Scope, TABLES, keys};

use super::check::{published_status, sql_text};
use super::journal;
use super::{Latest, Store};
use crate::release::release_dir;

/// Where pruning finds a release's files beside R2, and its clock.
pub struct PruneOptions<'a> {
    /// The build output, whose manifest of the release names its R2 files when R2 no longer
    /// holds the manifest.
    pub out: &'a Path,
    /// The directory of the publish journals, of which the release's is removed.
    pub journals: &'a Path,
    /// The time now, RFC 3339 in UTC.
    pub now: &'a dyn Fn() -> String,
}

/// A release pruned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pruned {
    pub number: u32,
    /// When it was first pruned, RFC 3339 in UTC.
    pub pruned_at: String,
    /// Whether an earlier prune had already recorded it.
    pub already: bool,
    /// The R2 files deleted, the manifest among them.
    pub files: usize,
    /// The rows of the release deleted from the tables of one release.
    pub rows: u64,
    /// Where `latest` points.
    pub latest: Latest,
}

/// A prune refused or stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PruneError {
    /// `latest` points to the release.
    Latest { number: u32 },
    /// Local D1 does not list the release.
    NotListed { number: u32 },
    /// A step failed; pruning again finishes it.
    Failed { number: u32, problem: String },
}

impl fmt::Display for PruneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Latest { number } => write!(
                f,
                "release {number} is not pruned: latest points to release {number}, and reads \
                 that name no release answer from it; publish a newer release first"
            ),
            Self::NotListed { number } => write!(
                f,
                "release {number} is not pruned: local D1 does not list it"
            ),
            Self::Failed { number, problem } => write!(
                f,
                "release {number} is not completely pruned: {problem}; pruning it again \
                 finishes it"
            ),
        }
    }
}

impl std::error::Error for PruneError {}

/// The manifest of release `number`: R2's, else the build output's.
fn manifest_of(
    store: &mut dyn Store,
    number: u32,
    out: &Path,
) -> Result<Option<ReleaseManifest>, String> {
    let key = keys::key(number, keys::MANIFEST);
    let bytes = match store.get(&key)? {
        Some(bytes) => bytes,
        None => match std::fs::read(release_dir(out, number).join(keys::MANIFEST)) {
            Ok(bytes) => bytes,
            Err(_) => return Ok(None),
        },
    };
    serde_norway::from_slice(&bytes)
        .map(Some)
        .map_err(|e| format!("the manifest of release {number}: {e}"))
}

/// The rows of release `number` in the tables of one release, as one sum of counts (D1 limits
/// the terms of a compound `SELECT`, so no `UNION`).
fn rows_of(store: &mut dyn Store, number: u32) -> Result<u64, String> {
    let counts: Vec<String> = TABLES
        .iter()
        .filter(|t| t.scope == Scope::PerRelease)
        .map(|t| format!("(SELECT COUNT(*) FROM {} WHERE release = {number})", t.name))
        .collect();
    let rows = store.query(&format!("SELECT {} AS n", counts.join(" + ")))?;
    rows.first()
        .and_then(|r| r.get("n"))
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| format!("D1 gave no count of the rows of release {number}"))
}

/// Prunes release `number` from `store`. `progress` is given a line for each step.
///
/// # Errors
///
/// Refuses, changing nothing, the release `latest` points to and one local D1 does not list.
/// Fails part way when a step fails; pruning again finishes it.
pub fn prune(
    store: &mut dyn Store,
    number: u32,
    options: &PruneOptions<'_>,
    progress: &mut dyn FnMut(String),
) -> Result<Pruned, PruneError> {
    let failed = |problem: String| PruneError::Failed { number, problem };
    let status = published_status(store, number).map_err(failed)?;
    let latest = Latest(status.latest.as_ref().map(|l| l.0));
    if latest.0 == Some(number) {
        return Err(PruneError::Latest { number });
    }
    if !status.listed {
        return Err(PruneError::NotListed { number });
    }
    let prefix = keys::prefix(number);
    let mut files = 0;
    if let Some(manifest) = manifest_of(store, number, options.out).map_err(failed)? {
        let r2 = manifest.files.iter().filter(|f| f.target == Target::R2);
        for key in r2.filter_map(|f| f.key.as_deref()) {
            if key.starts_with(&prefix) {
                store.delete(key).map_err(failed)?;
                files += 1;
            }
        }
        store
            .delete(&keys::key(number, keys::MANIFEST))
            .map_err(failed)?;
        files += 1;
    }
    progress(format!(
        "release {number}: {files} R2 files under {prefix} deleted"
    ));
    let rows = rows_of(store, number).map_err(failed)?;
    let mut statements: Vec<String> = TABLES
        .iter()
        .filter(|t| t.scope == Scope::PerRelease)
        .map(|t| format!("DELETE FROM {} WHERE release = {number};", t.name))
        .collect();
    statements.push(format!(
        "UPDATE releases SET pruned_at = {} WHERE number = {number} AND pruned_at IS NULL;",
        sql_text(&(options.now)())
    ));
    let sql = statements.join("\n") + "\n";
    std::fs::create_dir_all(options.journals).map_err(|e| failed(e.to_string()))?;
    let file = options.journals.join(format!("prune-{number}.sql"));
    std::fs::write(&file, sql).map_err(|e| failed(format!("{}: {e}", file.display())))?;
    let executed = store.execute_file(&file);
    let _ = std::fs::remove_file(&file);
    executed.map_err(failed)?;
    let left = rows_of(store, number).map_err(failed)?;
    if left > 0 {
        return Err(failed(format!(
            "D1 still holds {left} rows of release {number}"
        )));
    }
    progress(format!(
        "release {number}: {rows} rows deleted from the tables of one release; the list of \
         releases and the id ledger are kept"
    ));
    let after = published_status(store, number).map_err(failed)?;
    let pruned_at = after
        .pruned_at
        .ok_or_else(|| failed("D1 records no pruning time".to_owned()))?;
    let journal = journal::file(options.journals, number);
    if journal.exists() {
        std::fs::remove_file(&journal).map_err(|e| failed(e.to_string()))?;
    }
    Ok(Pruned {
        number,
        pruned_at,
        already: status.pruned_at.is_some(),
        files,
        rows,
        latest,
    })
}
