//! Pruning release n: its R2 files and its rows of the tables of one release deleted, and its
//! pruning time recorded in the list of releases, which keeps it (AC-100), beside the id
//! ledger, which is never pruned (AC-99). The release `latest` points to is refused (AC-101).
//!
//! The steps can run again: R2 files are deleted first, all but the manifest in one batch and
//! the manifest after them, then the rows and the pruning time in one file, the time last, then
//! the release's local build in the build output, `releases/{n}`, and last its publish journal,
//! which names its R2 files should the build's deletion fail. An interrupted prune is finished
//! by the next, which keeps the first pruning time recorded. A local build already gone is no
//! error.
//!
//! # Which R2 files a release has
//!
//! The release's manifest names them: R2's, else the build output's. The publish journal's list
//! of files uploaded and checked names them too, and is used beside the manifest or without it.
//! When none of these is left, nothing tells which files R2 holds, and pruning refuses rather
//! than record a pruning time with no file deleted. A release D1 never listed, whose publish
//! stopped before its tables were loaded, has its journaled R2 files deleted and reported.

use std::collections::BTreeSet;
use std::fmt;
use std::path::{Path, PathBuf};

use univ_model::release::manifest::Target;
use univ_model::release::shards::Database;
use univ_model::release::{ReleaseManifest, Scope, TABLES, keys};

use super::check::{published_status, sql_text};
use super::journal::{self, Journal};
use super::progress::{Lines, Progress};
use super::{Latest, Store, StoreOptions};
use crate::release::{parse_manifest, read_manifest, release_dir};
use crate::watch::STOPPED;

/// A release pruned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pruned {
    pub number: u32,
    /// When it was first pruned, RFC 3339 in UTC; for a release D1 never listed, when its
    /// files were deleted.
    pub pruned_at: String,
    /// Whether D1 listed the release; one it never listed has no pruning time recorded.
    pub listed: bool,
    /// Whether an earlier prune had already recorded it.
    pub already: bool,
    /// The R2 files deleted, the manifest among them.
    pub files: usize,
    /// The rows of the release deleted from the tables of one release.
    pub rows: u64,
    /// The release's local build in the build output, deleted; none when it was already gone.
    pub build: Option<PathBuf>,
    /// Where `latest` points.
    pub latest: Latest,
}

/// A prune refused or stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PruneError {
    /// `latest` points to the release.
    Latest { number: u32 },
    /// Local D1 does not list the release, and no publish journal lists files of it.
    NotListed { number: u32 },
    /// Nothing tells which R2 files the release has: no manifest and no journal.
    NoFileList { number: u32 },
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
                "release {number} is not pruned: local D1 does not list it, and no publish \
                 journal lists files of it"
            ),
            Self::NoFileList { number } => write!(
                f,
                "release {number} is not pruned, and nothing was deleted: neither R2 nor the \
                 build output holds its manifest, and no publish journal lists its files, so \
                 nothing tells which R2 files it has; put its manifest back in the build \
                 output's releases/{number}/ and prune again"
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
    if let Some(bytes) = store.get(&key)? {
        let text = String::from_utf8_lossy(&bytes);
        return parse_manifest(Path::new(&key), &text, number)
            .map(Some)
            .map_err(|e| e.to_string());
    }
    let dir = release_dir(out, number);
    if !dir.join(keys::MANIFEST).exists() {
        return Ok(None);
    }
    read_manifest(&dir, number)
        .map(|(m, _)| Some(m))
        .map_err(|e| e.to_string())
}

/// The R2 keys of release `number` that its manifest and its publish journal name, or none
/// when neither is left; and whether the journal named any.
fn r2_keys(
    store: &mut dyn Store,
    number: u32,
    options: &StoreOptions<'_>,
) -> Result<(Option<BTreeSet<String>>, bool), String> {
    let prefix = keys::prefix(number);
    let manifest = manifest_of(store, number, options.out)?;
    let journal = Journal::read(options.journals, number).map_err(|e| e.to_string())?;
    if manifest.is_none() && journal.is_none() {
        return Ok((None, false));
    }
    let mut found = BTreeSet::new();
    if let Some(m) = &manifest {
        let r2 = m.files.iter().filter(|f| f.target == Target::R2);
        found.extend(r2.filter_map(|f| f.key.clone()));
        found.insert(keys::key(number, keys::MANIFEST));
    }
    let journaled: Vec<String> = journal
        .iter()
        .flat_map(|j| &j.done)
        .filter(|d| d.target == Target::R2)
        .filter_map(|d| d.key.clone())
        .collect();
    let any_journaled = !journaled.is_empty();
    found.extend(journaled);
    found.retain(|k| k.starts_with(&prefix));
    Ok((Some(found), any_journaled))
}

/// Deletes `keys`: all but the release's manifest in one batch, then the manifest.
fn delete_all(store: &mut dyn Store, number: u32, keys: &BTreeSet<String>) -> Result<(), String> {
    let manifest = keys::key(number, keys::MANIFEST);
    let rest: Vec<String> = keys.iter().filter(|k| **k != manifest).cloned().collect();
    store.delete_keys(&rest)?;
    if keys.contains(&manifest) {
        store.delete(&manifest)?;
    }
    Ok(())
}

/// Deletes release `number`'s local build in the build output `out`, giving its directory, or
/// none when it was already gone.
fn remove_build(out: &Path, number: u32) -> Result<Option<PathBuf>, String> {
    let dir = release_dir(out, number);
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => Ok(Some(dir)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("its local build {}: {e}", dir.display())),
    }
}

/// Deletes release `number`'s local build, then its publish journal, with a line for the build.
fn remove_build_and_journal(
    options: &StoreOptions<'_>,
    number: u32,
    progress: &mut dyn Progress,
) -> Result<Option<PathBuf>, String> {
    let build = remove_build(options.out, number)?;
    progress.line(match &build {
        Some(dir) => format!(
            "release {number}: its local build {} deleted",
            dir.display()
        ),
        None => format!(
            "release {number}: no local build was left at {}",
            release_dir(options.out, number).display()
        ),
    });
    remove_journal(options.journals, number)?;
    Ok(build)
}

/// The databases that hold the schema, each object shard Univ Server binds then the catalogue
/// database: a shard no release was ever loaded into holds nothing to prune.
fn databases(store: &mut dyn Store) -> Result<Vec<Database>, String> {
    let shards = store.bound_shards()?;
    let mut held = Vec::new();
    for database in (0..shards)
        .map(Database::Shard)
        .chain([Database::Catalogue])
    {
        let schema = store.query(
            database,
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'schema_version'",
        )?;
        if !schema.is_empty() {
            held.push(database);
        }
    }
    Ok(held)
}

/// The rows of release `number` in the tables of one release in `database`, as one sum of
/// counts (D1 limits the terms of a compound `SELECT`, so no `UNION`).
fn rows_of(store: &mut dyn Store, database: Database, number: u32) -> Result<u64, String> {
    let counts: Vec<String> = TABLES
        .iter()
        .filter(|t| t.scope == Scope::PerRelease)
        .map(|t| format!("(SELECT COUNT(*) FROM {} WHERE release = {number})", t.name))
        .collect();
    let rows = store.query(database, &format!("SELECT {} AS n", counts.join(" + ")))?;
    rows.first()
        .and_then(|r| r.get("n"))
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| format!("{database} gave no count of the rows of release {number}"))
}

/// Deletes release `number`'s rows of the tables of one release from `database`, and runs
/// `then` after, in one file written to `journals`.
fn delete_rows(
    store: &mut dyn Store,
    database: Database,
    number: u32,
    journals: &Path,
    then: Option<String>,
) -> Result<(), String> {
    let mut statements: Vec<String> = TABLES
        .iter()
        .filter(|t| t.scope == Scope::PerRelease)
        .map(|t| format!("DELETE FROM {} WHERE release = {number};", t.name))
        .collect();
    statements.extend(then);
    let sql = statements.join("\n") + "\n";
    std::fs::create_dir_all(journals).map_err(|e| e.to_string())?;
    let file = journals.join(format!("prune-{number}-{}.sql", database.dir()));
    std::fs::write(&file, sql).map_err(|e| format!("{}: {e}", file.display()))?;
    let executed = store.execute_file(database, &file);
    let _ = std::fs::remove_file(&file);
    executed
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
    options: &StoreOptions<'_>,
    progress: &mut dyn FnMut(String),
) -> Result<Pruned, PruneError> {
    prune_with(store, number, options, &mut Lines(progress))
}

/// [`prune`], telling `progress` how many steps it takes once it is not refused. Once `progress`
/// says the owner asked it to stop, it stops after the step in hand; pruning again finishes it.
///
/// # Errors
///
/// As [`prune`], and as failed part way when the owner asked it to stop.
pub fn prune_with(
    store: &mut dyn Store,
    number: u32,
    options: &StoreOptions<'_>,
    progress: &mut dyn Progress,
) -> Result<Pruned, PruneError> {
    let failed = |problem: String| PruneError::Failed { number, problem };
    let status = published_status(store, number).map_err(failed)?;
    let latest = status.latest();
    if latest.0 == Some(number) {
        return Err(PruneError::Latest { number });
    }
    let (listing, journaled) = r2_keys(store, number, options).map_err(failed)?;
    let prefix = keys::prefix(number);
    if !status.listed {
        let Some(keys) = listing.filter(|_| journaled) else {
            return Err(PruneError::NotListed { number });
        };
        // The R2 files, then the local build.
        progress.started(2);
        delete_all(store, number, &keys).map_err(failed)?;
        progress.line(format!(
            "release {number}: never listed in D1, but its journal records R2 uploads: the {} \
             keys under {prefix} its journal and manifest name deleted",
            keys.len()
        ));
        let pruned_at = (options.now)();
        let build = remove_build_and_journal(options, number, progress).map_err(failed)?;
        return Ok(Pruned {
            number,
            pruned_at,
            listed: false,
            already: false,
            files: keys.len(),
            rows: 0,
            build,
            latest,
        });
    }
    // The R2 files, the rows, then the local build.
    progress.started(3);
    let files = match listing {
        Some(keys) => {
            delete_all(store, number, &keys).map_err(failed)?;
            keys.len()
        }
        // An earlier prune deleted the files, then the journal: nothing is left to delete.
        None if status.pruned_at.is_some() => 0,
        None => return Err(PruneError::NoFileList { number }),
    };
    progress.line(format!(
        "release {number}: {files} R2 files under {prefix} deleted"
    ));
    if progress.stopping() {
        return Err(failed(STOPPED.to_owned()));
    }
    let held = databases(store).map_err(failed)?;
    let mut rows = 0;
    for &database in &held {
        rows += rows_of(store, database, number).map_err(failed)?;
    }
    // Each object shard's rows, then the catalogue's with the pruning time, last.
    for &database in &held {
        let then = (database == Database::Catalogue).then(|| {
            format!(
                "UPDATE releases SET pruned_at = {} WHERE number = {number} AND pruned_at IS \
                 NULL;",
                sql_text(&(options.now)())
            )
        });
        delete_rows(store, database, number, options.journals, then).map_err(failed)?;
    }
    for &database in &held {
        let left = rows_of(store, database, number).map_err(failed)?;
        if left > 0 {
            return Err(failed(format!(
                "{database} still holds {left} rows of release {number}"
            )));
        }
    }
    let shards = held.len().saturating_sub(1);
    progress.line(format!(
        "release {number}: {rows} rows deleted from the tables of one release, in the \
         catalogue database and {shards} object shard(s); the list of releases and the id \
         ledger are kept"
    ));
    if progress.stopping() {
        return Err(failed(STOPPED.to_owned()));
    }
    let after = published_status(store, number).map_err(failed)?;
    let pruned_at = after
        .pruned_at
        .ok_or_else(|| failed("D1 records no pruning time".to_owned()))?;
    let build = remove_build_and_journal(options, number, progress).map_err(failed)?;
    Ok(Pruned {
        number,
        pruned_at,
        listed: true,
        already: status.pruned_at.is_some(),
        files,
        rows,
        build,
        latest,
    })
}

/// Removes release `number`'s publish journal, if there is one.
fn remove_journal(journals: &Path, number: u32) -> Result<(), String> {
    let file = journal::file(journals, number);
    if file.exists() {
        std::fs::remove_file(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    }
    Ok(())
}
