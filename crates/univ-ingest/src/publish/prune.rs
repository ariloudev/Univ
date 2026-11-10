//! Pruning release n: the R2 files and the rows of the tables of releases that n holds and no
//! release left unpruned shares deleted, and its pruning time recorded in the list of releases, which keeps it (AC-100), beside the id
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
use univ_model::release::{ReleaseManifest, Scope, TABLES, keys, valid_in};

use super::check::{
    Status, counts_of, execute_sql, kept_releases, published_status, r2_manifest, sha256s,
    sql_text, tables_in,
};
use super::journal::{self, Journal};
use super::progress::{Lines, Progress};
use super::{Latest, Store, StoreOptions};
use crate::release::{read_manifest, release_dir};
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
    if let Some(manifest) = r2_manifest(store, number)? {
        return Ok(Some(manifest));
    }
    let dir = release_dir(out, number);
    if !dir.join(keys::MANIFEST).exists() {
        return Ok(None);
    }
    read_manifest(&dir, number)
        .map(|(m, _)| Some(m))
        .map_err(|e| e.to_string())
}

/// The R2 keys of release `number` that its manifest and its publish journal name and that no
/// other release left unpruned holds, or none when neither is left; and whether the journal
/// named any.
fn r2_keys(
    store: &mut dyn Store,
    (number, status): (u32, &Status),
    options: &StoreOptions<'_>,
) -> Result<(Option<BTreeSet<String>>, bool), String> {
    let manifest = manifest_of(store, number, options.out)?;
    let journal = Journal::read(options.journals, number).map_err(|e| e.to_string())?;
    // The files its map of paths names: a release brought under shared rows has a manifest
    // that names their keys under its prefix.
    let mapped = if status.file_map {
        let sql = format!(
            "SELECT DISTINCT sha256 FROM release_files WHERE {}",
            valid_in("", number)
        );
        sha256s(store, &sql)?
    } else {
        BTreeSet::new()
    };
    if manifest.is_none() && journal.is_none() && mapped.is_empty() {
        return Ok((None, false));
    }
    let mut found: BTreeSet<String> = mapped.iter().map(|s| keys::file(s)).collect();
    if let Some(m) = &manifest {
        let r2 = m.files.iter().filter(|f| f.target == Target::R2);
        found.extend(r2.filter_map(|f| f.key.clone()));
        found.insert(keys::manifest(number));
    }
    let journaled: Vec<String> = journal
        .iter()
        .flat_map(|j| &j.done)
        .filter(|d| d.target == Target::R2)
        .filter_map(|d| d.key.clone())
        .collect();
    let any_journaled = !journaled.is_empty();
    found.extend(journaled);
    let mut held = held_elsewhere(store, number)?;
    held.extend(journaled_elsewhere(options.journals, number)?);
    let manifest = keys::manifest(number);
    found.retain(|k| *k == manifest || (k.starts_with(keys::FILES) && !held.contains(k)));
    Ok((Some(found), any_journaled))
}

/// The R2 keys that the publish journal of a release other than `number` records as uploaded:
/// a release part way through publishing holds its files before its `release_files` are in D1.
fn journaled_elsewhere(journals: &Path, number: u32) -> Result<BTreeSet<String>, String> {
    let Ok(entries) = std::fs::read_dir(journals) else {
        return Ok(BTreeSet::new());
    };
    let mut keys = BTreeSet::new();
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name();
        let other = name
            .to_str()
            .and_then(|n| n.strip_prefix("publish-")?.strip_suffix(".yaml"))
            .and_then(|n| n.parse::<u32>().ok())
            .filter(|&n| n != number);
        let Some(other) = other else { continue };
        let journal = Journal::read(journals, other).map_err(|e| e.to_string())?;
        keys.extend(
            journal
                .iter()
                .flat_map(|j| &j.done)
                .filter(|d| d.target == Target::R2)
                .filter_map(|d| d.key.clone()),
        );
    }
    Ok(keys)
}

/// The R2 keys of the files that a release left unpruned, other than `number`, maps a path to.
fn held_elsewhere(store: &mut dyn Store, number: u32) -> Result<BTreeSet<String>, String> {
    let sql = format!(
        "SELECT DISTINCT f.sha256 AS sha256 FROM release_files f JOIN releases r \
         ON r.pruned_at IS NULL AND r.number != {number} AND {}",
        valid_in("f.", "r.number")
    );
    Ok(sha256s(store, &sql)?
        .iter()
        .map(|s| keys::file(s))
        .collect())
}

/// Deletes `keys`: all but the release's manifest in one batch, then the manifest.
fn delete_all(store: &mut dyn Store, number: u32, keys: &BTreeSet<String>) -> Result<(), String> {
    let manifest = keys::manifest(number);
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

/// The databases that hold release `number`'s rows, each with the tables of one release it
/// holds: the object shards the catalogue's index names for it, then the catalogue. A shard is
/// given the whole schema; the catalogue may hold an older one, without the shards' index.
fn databases(
    store: &mut dyn Store,
    number: u32,
    status: &Status,
) -> Result<Vec<(Database, Vec<&'static str>)>, String> {
    let per_release = || TABLES.iter().filter(|t| t.scope == Scope::PerRelease);
    let mut held = Vec::new();
    if status.shard_index {
        let sql = format!(
            "SELECT shard FROM release_shards WHERE {}",
            valid_in("", number)
        );
        for row in store.query(Database::Catalogue, &sql)? {
            let shard = row
                .get("shard")
                .and_then(serde_json::Value::as_u64)
                .and_then(|s| u32::try_from(s).ok())
                .ok_or_else(|| format!("release_shards names no shard of release {number}"))?;
            held.push((
                Database::Shard(shard),
                per_release().map(|t| t.name).collect(),
            ));
        }
    }
    let catalogue = tables_in(store, Database::Catalogue)?;
    let tables = per_release()
        .map(|t| t.name)
        .filter(|name| catalogue.contains(*name))
        .collect();
    held.push((Database::Catalogue, tables));
    Ok(held)
}

/// The SQL condition selecting the rows a prune of release `number` deletes: those valid in it
/// and in none of the releases `kept`. A row is valid in an unbroken run of releases, from its
/// `release` until its `retired`, so it is valid in no kept release when that run lies after
/// the last kept release before `number` and ends by the first kept release after it.
fn pruned_rows(number: u32, kept: &[u32]) -> String {
    let before = kept.iter().filter(|&&k| k < number).max();
    let after = kept.iter().filter(|&&k| k > number).min();
    std::iter::once(valid_in("", number))
        .chain(before.map(|b| format!("release > {b}")))
        .chain(after.map(|a| format!("retired IS NOT NULL AND retired <= {a}")))
        .collect::<Vec<_>>()
        .join(" AND ")
}

/// The rows `filter` selects in `tables` of `database`, summed.
fn rows_of(
    store: &mut dyn Store,
    database: Database,
    tables: &[&str],
    filter: &str,
) -> Result<u64, String> {
    let each: Vec<(&str, String)> = tables.iter().map(|t| (*t, filter.to_owned())).collect();
    Ok(counts_of(store, database, &each)?.iter().sum())
}

/// Deletes the rows of `tables` from `database` that `filter` selects, and runs `then` after,
/// in one file written to `journals`, named for release `number`.
fn delete_rows(
    store: &mut dyn Store,
    (database, tables): (Database, &[&str]),
    (number, filter): (u32, &str),
    journals: &Path,
    then: Option<String>,
) -> Result<(), String> {
    let mut statements: Vec<String> = tables
        .iter()
        .map(|t| format!("DELETE FROM {t} WHERE {filter};"))
        .collect();
    statements.extend(then);
    let sql = statements.join("\n") + "\n";
    let file = journals.join(format!("prune-{number}-{}.sql", database.dir()));
    execute_sql(store, database, &file, &sql)
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
    let (listing, journaled) = r2_keys(store, (number, &status), options).map_err(failed)?;
    if !status.listed {
        let Some(keys) = listing.filter(|_| journaled) else {
            return Err(PruneError::NotListed { number });
        };
        // The R2 files, then the local build.
        progress.started(2);
        delete_all(store, number, &keys).map_err(failed)?;
        progress.line(format!(
            "release {number}: never listed in D1, but its journal records R2 uploads: the {} \
             keys its journal and manifest name that no other release holds deleted",
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
        "release {number}: {files} R2 files that no release left unpruned holds deleted"
    ));
    if progress.stopping() {
        return Err(failed(STOPPED.to_owned()));
    }
    // Each object shard's rows, then the catalogue's with the pruning time, last: a prune
    // stopped part way still finds the shards by the catalogue's index.
    let held = databases(store, number, &status).map_err(failed)?;
    let kept = kept_releases(store, number).map_err(failed)?;
    let filter = pruned_rows(number, &kept);
    let mut rows = 0;
    for (database, tables) in &held {
        rows += rows_of(store, *database, tables, &filter).map_err(failed)?;
        let then = (*database == Database::Catalogue).then(|| {
            format!(
                "UPDATE releases SET pruned_at = {} WHERE number = {number} AND pruned_at IS \
                 NULL;",
                sql_text(&(options.now)())
            )
        });
        delete_rows(
            store,
            (*database, tables),
            (number, &filter),
            options.journals,
            then,
        )
        .map_err(failed)?;
        let left = rows_of(store, *database, tables, &filter).map_err(failed)?;
        if left > 0 {
            return Err(failed(format!(
                "{database} still holds {left} rows of release {number} that no release kept \
                 holds"
            )));
        }
    }
    let shards = held.len().saturating_sub(1);
    progress.line(format!(
        "release {number}: {rows} rows that no release left unpruned holds deleted, in the \
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
