//! Publishing release n: every file uploaded and checked, entered in the journal, and only then
//! `latest` moved to it in one write.

use std::path::{Path, PathBuf};

use univ_model::release::manifest::{D1Estimate, MANIFEST_VERSION, ManifestFile, Target};
use univ_model::release::shards::Database;
use univ_model::release::{ReleaseManifest, TABLES, Table, keys};

use super::check::{check_rows, schema_version, sql_text};
use super::journal::{Done, Journal};
use super::progress::{FileDone, Lines, Progress};
use super::refuse::{Refused, refuse_published, unbound, within_caps};
use super::{Latest, PublishError, Store, StoreOptions};
use crate::release::{RELEASES, read_manifest, release_dir};
use crate::state::State;
use crate::tiles::sha256;
use crate::watch::STOPPED;

/// A release published.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Published {
    pub number: u32,
    /// When `latest` moved to it, RFC 3339 in UTC.
    pub published_at: String,
    /// Where `latest` pointed before.
    pub previous: Latest,
    /// The files uploaded by this run, the manifest among them.
    pub uploaded: usize,
    /// The files the journal listed as uploaded and checked by an earlier run.
    pub journaled: usize,
}

/// A unit uploaded and checked as one: a database's schema, an R2 file, or a table's D1 files
/// in one database.
enum Unit<'m> {
    Schema(Database, &'m ManifestFile),
    R2(&'m ManifestFile),
    Table(Database, &'static Table, Vec<&'m ManifestFile>),
}

/// The database and table a D1 file `d1/{dir}/{nnnn}-{table}-{part}.sql` loads, no table for a
/// schema's file.
fn file_of(path: &str) -> Result<(Database, Option<&'static Table>), String> {
    let not_named = || format!("{path} is not named as a D1 file");
    let (dir, file) = path
        .strip_prefix(&format!("{}/", keys::D1_DIR))
        .and_then(|rest| rest.split_once('/'))
        .ok_or_else(not_named)?;
    let database = Database::from_dir(dir).ok_or_else(not_named)?;
    let name = file
        .strip_suffix(".sql")
        .and_then(|p| p.split_once('-'))
        .map(|(_, rest)| rest.rsplit_once('-').map_or(rest, |(table, _)| table))
        .ok_or_else(not_named)?;
    if name == "schema" {
        return Ok((database, None));
    }
    let table = TABLES
        .iter()
        .find(|t| t.name == name)
        .ok_or_else(|| format!("{path} loads no table of the schema"))?;
    Ok((database, Some(table)))
}

/// The release's units in the order they are uploaded: each database's schema, the R2 files,
/// then each database's tables' D1 files, the catalogue's first, each in the order of the
/// schema's tables.
fn units(manifest: &ReleaseManifest) -> Result<Vec<Unit<'_>>, String> {
    let mut schemas = Vec::new();
    let mut r2 = Vec::new();
    let mut tables: Vec<(Database, &'static Table, Vec<&ManifestFile>)> = Vec::new();
    for f in &manifest.files {
        match f.target {
            Target::R2 => r2.push(Unit::R2(f)),
            Target::D1 => match file_of(&f.path)? {
                (db, None) => schemas.push((db, f)),
                (db, Some(t)) => match tables
                    .iter_mut()
                    .find(|(d, u, _)| *d == db && u.name == t.name)
                {
                    Some((_, _, files)) => files.push(f),
                    None => tables.push((db, t, vec![f])),
                },
            },
        }
    }
    schemas.sort_by_key(|(db, _)| *db);
    tables.sort_by_key(|(db, t, _)| (*db, TABLES.iter().position(|u| u.name == t.name)));
    let mut units: Vec<Unit<'_>> = schemas
        .into_iter()
        .map(|(db, f)| Unit::Schema(db, f))
        .collect();
    units.extend(r2);
    units.extend(tables.into_iter().map(|(db, t, f)| Unit::Table(db, t, f)));
    Ok(units)
}

/// The manifest's estimate of the rows of `database`.
fn estimate_of(manifest: &ReleaseManifest, database: Database) -> Option<&D1Estimate> {
    match database {
        Database::Catalogue => Some(&manifest.d1),
        Database::Shard(i) => manifest.shards.get(i as usize).map(|s| &s.d1),
    }
}

/// The complete release `number` in the build output: its directory, its manifest and the
/// manifest's bytes. Each file is checked against its SHA-256 as it is uploaded, not before.
fn read_release(
    options: &StoreOptions<'_>,
    number: u32,
) -> Result<(PathBuf, ReleaseManifest, Vec<u8>), String> {
    let dir = release_dir(options.out, number);
    if !dir.join(keys::MANIFEST).exists() {
        let building = options
            .out
            .join(RELEASES)
            .join(format!(".building-{number}"));
        return Err(if building.exists() {
            format!("its build in {} did not finish", building.display())
        } else {
            format!("{} holds no complete release", dir.display())
        });
    }
    let (manifest, bytes) = read_manifest(&dir, number).map_err(|e| e.to_string())?;
    if manifest.manifest_version < MANIFEST_VERSION {
        return Err(format!(
            "it was built under manifest version {}, before object shards; build it again",
            manifest.manifest_version
        ));
    }
    let state = State::read(options.state).map_err(|e| e.to_string())?;
    let name = number.to_string();
    if !state.ledger.releases().iter().any(|r| r.name == name) {
        return Err(format!(
            "the state in {} does not record it: its build stopped before it recorded the \
             release, or another state built it",
            options.state.display()
        ));
    }
    Ok((dir, manifest, bytes))
}

/// Checks the local file at `path` against its SHA-256 in the manifest once more.
fn local(path: &Path, f: &ManifestFile) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if sha256(&bytes) == f.sha256 {
        Ok(())
    } else {
        Err(format!("{} no longer matches its SHA-256", f.path))
    }
}

/// Writes `file` to R2 at `key` and reads it back, checking its size and SHA-256.
fn put_checked(
    store: &mut dyn Store,
    key: &str,
    file: &Path,
    bytes: u64,
    sha: &str,
) -> Result<(), String> {
    let back = store
        .put_read_back(key, file)?
        .ok_or_else(|| format!("R2 holds nothing at {key} once it was written"))?;
    if back.bytes == bytes && back.sha256 == sha {
        Ok(())
    } else {
        Err(format!(
            "R2 gives {key} back with SHA-256 {}, not the manifest's {sha}",
            back.sha256
        ))
    }
}

/// Uploads one unit and checks it, telling `progress` of each file as soon as it is done, and
/// giving each as the journal enters it.
fn upload(
    store: &mut dyn Store,
    dir: &Path,
    manifest: &ReleaseManifest,
    unit: &Unit<'_>,
    at: &str,
    progress: &mut dyn Progress,
) -> Result<Vec<Done>, String> {
    let n = manifest.release;
    let done = |f: &ManifestFile, checked: String| Done {
        path: f.path.clone(),
        target: f.target,
        key: f.key.clone(),
        sha256: f.sha256.clone(),
        checked,
        at: at.to_owned(),
    };
    let mut told = |f: &ManifestFile, said: &str, rows| {
        progress.file(FileDone {
            path: &f.path,
            bytes: f.bytes,
            line: format!("release {n}: {}: {said}", f.path),
            rows,
        });
    };
    match unit {
        Unit::Schema(db, f) => {
            let path = dir.join(&f.path);
            local(&path, f)?;
            store.execute_file(*db, &path)?;
            let want = i64::from(manifest.versions.d1_schema);
            match schema_version(store, *db)? {
                Some(v) if v >= want => {
                    let checked = format!("schema version {v} in {db}");
                    told(f, &checked, None);
                    Ok(vec![done(f, checked)])
                }
                v => Err(format!(
                    "{db} records schema version {v:?} once {} ran, not {want}",
                    f.path
                )),
            }
        }
        Unit::R2(f) => {
            // Each file lies under its SHA-256; the release manifest alone under the release.
            let key = if f.path == keys::MANIFEST {
                keys::key(n, keys::MANIFEST)
            } else {
                keys::file(&f.sha256)
            };
            if f.key.as_deref() != Some(key.as_str()) {
                return Err(format!("{} is not keyed as {key} in the manifest", f.path));
            }
            let path = dir.join(&f.path);
            local(&path, f)?;
            put_checked(store, &key, &path, f.bytes, &f.sha256)?;
            let checked = "read back with its SHA-256".to_owned();
            told(f, &checked, None);
            Ok(vec![done(f, checked)])
        }
        Unit::Table(db, table, files) => {
            let Some((last, before)) = files.split_last() else {
                return Ok(vec![]);
            };
            // Each file's line as its import finishes; the last's once the table is checked.
            for (k, f) in before.iter().enumerate() {
                let path = dir.join(&f.path);
                local(&path, f)?;
                store.execute_file(*db, &path)?;
                let said = format!(
                    "imported, {} of {} files of {} in {db}",
                    k + 1,
                    files.len(),
                    table.name
                );
                told(f, &said, None);
            }
            let path = dir.join(&last.path);
            local(&path, last)?;
            store.execute_file(*db, &path)?;
            let want = estimate_of(manifest, *db)
                .and_then(|e| e.tables.iter().find(|t| t.table == table.name))
                .ok_or_else(|| {
                    format!(
                        "the manifest gives no fingerprint of {} in {db}",
                        table.name
                    )
                })?;
            let rows = check_rows(store, *db, table, n, want)?;
            // The count and fingerprint both matched.
            let shard = match db {
                Database::Catalogue => String::new(),
                Database::Shard(i) => format!(" in shard {i}"),
            };
            let checked = format!("{}: {rows} rows of release {n}{shard}", table.name);
            told(
                last,
                &format!("imported; {checked}"),
                Some((table.name, rows)),
            );
            Ok(files.iter().map(|f| done(f, checked.clone())).collect())
        }
    }
}

fn unit_files<'u>(unit: &'u Unit<'_>) -> Vec<&'u ManifestFile> {
    match unit {
        Unit::Schema(_, f) | Unit::R2(f) => vec![f],
        Unit::Table(_, _, files) => files.clone(),
    }
}

/// The single write that moves `latest` to release `number` (AC-104).
#[must_use]
pub fn move_latest(number: u32, at: &str) -> String {
    format!(
        "INSERT INTO latest (one, release, moved_at) VALUES (1, {number}, {}) ON CONFLICT (one) \
         DO UPDATE SET release = excluded.release, moved_at = excluded.moved_at",
        sql_text(at)
    )
}

/// Records when release `number` was published, once `latest` has moved to it.
pub(super) fn record_published(store: &mut dyn Store, number: u32, at: &str) -> Result<(), String> {
    store
        .query(
            Database::Catalogue,
            &format!(
                "UPDATE releases SET published_at = {} WHERE number = {number} AND published_at IS \
             NULL",
                sql_text(at)
            ),
        )
        .map(drop)
}

/// Publishes release `number` of the build output to `store`, continuing from its journal, and
/// moves `latest` to it once every file is uploaded and checked. `progress` is given a line for
/// each step.
///
/// # Errors
///
/// Refuses, changing nothing, a release that is not complete, already published or pruned, or
/// older than the one `latest` points to, or whose journal is of another manifest. Fails part
/// way, leaving `latest` where it was, when an upload or check fails.
pub fn publish(
    store: &mut dyn Store,
    number: u32,
    options: &StoreOptions<'_>,
    progress: &mut dyn FnMut(String),
) -> Result<Published, PublishError> {
    publish_with(store, number, options, &mut Lines(progress))
}

/// [`publish`], telling `progress` how many files it will upload and each file as soon as it is
/// done. Once `progress` says the owner asked it to stop, it stops after the unit in hand: the
/// journal lists every unit done, `latest` is where it was, and publishing the release again
/// continues from there.
///
/// # Errors
///
/// Fails as [`publish`] does, and as interrupted when the owner asked it to stop.
pub fn publish_with(
    store: &mut dyn Store,
    number: u32,
    options: &StoreOptions<'_>,
    progress: &mut dyn Progress,
) -> Result<Published, PublishError> {
    let (dir, manifest, manifest_bytes) = read_release(options, number)
        .map_err(|problem| PublishError::NotBuilt { number, problem })?;
    let total = manifest.files.len() + 1;
    let stopped = |latest: Latest, done: usize, problem: String| PublishError::Interrupted {
        number,
        latest,
        done,
        total,
        problem,
    };
    // The manifest itself goes last, read back against its local bytes.
    let itself = ManifestFile {
        path: keys::MANIFEST.to_owned(),
        target: Target::R2,
        key: Some(keys::key(number, keys::MANIFEST)),
        bytes: manifest_bytes.len() as u64,
        sha256: sha256(&manifest_bytes),
    };
    let mut journal = open_journal(options, number, &itself.sha256)?;
    let mut units = units(&manifest).map_err(|p| PublishError::NotBuilt { number, problem: p })?;
    units.push(Unit::R2(&itself));
    let all_done = units.iter().all(|u| {
        unit_files(u)
            .iter()
            .all(|f| journal.has(&f.path, &f.sha256))
    });
    // Once every unit is uploaded and checked, the release is whole: when its publication time
    // was not recorded, the time latest moved to it (or its last unit's) is.
    let done_at = all_done
        .then(|| {
            journal
                .latest_moved_at
                .clone()
                .or_else(|| journal.done.last().map(|d| d.at.clone()))
        })
        .flatten();
    let (latest, holds) = refuse_published(store, number, done_at).map_err(|e| match e {
        Refused::Error(e) => e,
        Refused::Query(p) => stopped(Latest(None), 0, p),
    })?;
    unbound(store, &manifest)
        .and_then(|()| within_caps(store, &manifest, holds))
        .map_err(|e| match e {
            Refused::Error(e) => e,
            Refused::Query(p) => stopped(latest, 0, p),
        })?;
    let journaled = journal.done.len();
    progress.started(total - journaled);
    if journaled > 0 {
        progress.note(format!(
            "release {number}: {journaled} of {total} files already uploaded and checked, by \
             the journal"
        ));
    }
    for unit in &units {
        if unit_files(unit)
            .iter()
            .all(|f| journal.has(&f.path, &f.sha256))
        {
            continue;
        }
        if progress.stopping() {
            return Err(stopped(latest, journal.done.len(), STOPPED.to_owned()));
        }
        let done = upload(store, &dir, &manifest, unit, &(options.now)(), progress)
            .map_err(|p| stopped(latest, journal.done.len(), p))?;
        journal.done.extend(done);
        journal
            .write(options.journals)
            .map_err(|e| stopped(latest, journal.done.len(), format!("the journal: {e}")))?;
    }
    let at = (options.now)();
    store
        .query(Database::Catalogue, &move_latest(number, &at))
        .map_err(|p| stopped(latest, journal.done.len(), p))?;
    journal.latest_moved_at = Some(at.clone());
    let moved = Latest(Some(number));
    journal
        .write(options.journals)
        .and(record_published(store, number, &at).map_err(std::io::Error::other))
        .map_err(|e| stopped(moved, total, e.to_string()))?;
    Ok(Published {
        number,
        published_at: at,
        previous: latest,
        uploaded: total - journaled,
        journaled,
    })
}

/// The journal of release `number` under the manifest with SHA-256 `manifest_sha`: the one an
/// earlier run left, or a new one.
fn open_journal(
    options: &StoreOptions<'_>,
    number: u32,
    manifest_sha: &str,
) -> Result<Journal, PublishError> {
    let journal_error = |problem: String| PublishError::Journal { number, problem };
    match Journal::read(options.journals, number) {
        Ok(Some(j)) if j.manifest_sha256 != manifest_sha => Err(journal_error(format!(
            "its journal {} is of another manifest of release {number}; a different build of \
             the number was partly published, which this one cannot continue",
            super::journal::file(options.journals, number).display()
        ))),
        Ok(Some(j)) => Ok(j),
        Ok(None) => Ok(Journal::new(number, manifest_sha, &(options.now)())),
        Err(e) => Err(journal_error(e.to_string())),
    }
}
