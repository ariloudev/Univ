//! Publishing release n: every file uploaded and checked, entered in the journal, and only then
//! `latest` moved to it in one write.

use std::path::{Path, PathBuf};

use univ_model::release::manifest::{ManifestFile, Target};
use univ_model::release::{D1_CAP_BYTES, ReleaseManifest, TABLES, Table, keys};

use super::check::{check_rows, published_status, schema_version, sql_text};
use super::journal::{Done, Journal};
use super::{Latest, PublishError, Store};
use crate::release::{RELEASES, release_dir, verify};
use crate::tiles::sha256;

/// Where publishing reads the release and keeps its journal, and its clock.
pub struct PublishOptions<'a> {
    /// The build output, whose `releases/{n}` holds the release.
    pub out: &'a Path,
    /// The directory of the publish journals.
    pub journals: &'a Path,
    /// The time now, RFC 3339 in UTC.
    pub now: &'a dyn Fn() -> String,
}

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

/// A unit uploaded and checked as one: the schema, an R2 file, or a table's D1 files.
enum Unit<'m> {
    Schema(&'m ManifestFile),
    R2(&'m ManifestFile),
    Table(&'static Table, Vec<&'m ManifestFile>),
}

/// The table a D1 file `d1/{nnnn}-{table}-{part}.sql` loads, or none for the schema's file.
fn table_of(path: &str) -> Result<Option<&'static Table>, String> {
    let name = path
        .strip_prefix(&format!("{}/", keys::D1_DIR))
        .and_then(|p| p.strip_suffix(".sql"))
        .and_then(|p| p.split_once('-'))
        .map(|(_, rest)| rest.rsplit_once('-').map_or(rest, |(table, _)| table));
    match name {
        Some("schema") => Ok(None),
        Some(name) => TABLES
            .iter()
            .find(|t| t.name == name)
            .map(Some)
            .ok_or_else(|| format!("{path} loads no table of the schema")),
        None => Err(format!("{path} is not named as a D1 file")),
    }
}

/// The release's units in the order they are uploaded: the schema, the R2 files, then each
/// table's D1 files in the order of the schema's tables.
fn units(manifest: &ReleaseManifest) -> Result<Vec<Unit<'_>>, String> {
    let mut schema = Vec::new();
    let mut r2 = Vec::new();
    let mut tables: Vec<(&'static Table, Vec<&ManifestFile>)> = Vec::new();
    for f in &manifest.files {
        match f.target {
            Target::R2 => r2.push(Unit::R2(f)),
            Target::D1 => match table_of(&f.path)? {
                None => schema.push(Unit::Schema(f)),
                Some(t) => match tables.iter_mut().find(|(u, _)| u.name == t.name) {
                    Some((_, files)) => files.push(f),
                    None => tables.push((t, vec![f])),
                },
            },
        }
    }
    tables.sort_by_key(|(t, _)| TABLES.iter().position(|u| u.name == t.name));
    schema.extend(r2);
    schema.extend(tables.into_iter().map(|(t, f)| Unit::Table(t, f)));
    Ok(schema)
}

/// The complete release `number` in the build output: its manifest, read and with every file
/// checked against it, and the manifest's bytes.
fn read_release(out: &Path, number: u32) -> Result<(PathBuf, ReleaseManifest, Vec<u8>), String> {
    let dir = release_dir(out, number);
    let path = dir.join(keys::MANIFEST);
    let Ok(bytes) = std::fs::read(&path) else {
        let building = out.join(RELEASES).join(format!(".building-{number}"));
        return Err(if building.exists() {
            format!("its build in {} did not finish", building.display())
        } else {
            format!("{} holds no complete release", dir.display())
        });
    };
    let manifest: ReleaseManifest =
        serde_norway::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    if manifest.release != number {
        return Err(format!(
            "{} is the manifest of release {}",
            path.display(),
            manifest.release
        ));
    }
    verify(&dir, &manifest).map_err(|f| f.cause)?;
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
    store.put(key, file)?;
    let back = store
        .get(key)?
        .ok_or_else(|| format!("R2 holds nothing at {key} once it was written"))?;
    let got = sha256(&back);
    if back.len() as u64 == bytes && got == sha {
        Ok(())
    } else {
        Err(format!(
            "R2 gives {key} back with SHA-256 {got}, not the manifest's {sha}"
        ))
    }
}

/// Uploads one unit and checks it, giving each of its files as the journal enters it.
fn upload(
    store: &mut dyn Store,
    dir: &Path,
    manifest: &ReleaseManifest,
    unit: &Unit<'_>,
    at: &str,
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
    match unit {
        Unit::Schema(f) => {
            let path = dir.join(&f.path);
            local(&path, f)?;
            store.execute_file(&path)?;
            let want = i64::from(manifest.versions.d1_schema);
            match schema_version(store)? {
                Some(v) if v >= want => Ok(vec![done(f, format!("schema version {v}"))]),
                v => Err(format!(
                    "D1 records schema version {v:?} once {} ran, not {want}",
                    f.path
                )),
            }
        }
        Unit::R2(f) => {
            let key = f
                .key
                .as_deref()
                .filter(|k| k.starts_with(&keys::prefix(n)))
                .ok_or_else(|| format!("{} has no key under {}", f.path, keys::prefix(n)))?;
            let path = dir.join(&f.path);
            local(&path, f)?;
            put_checked(store, key, &path, f.bytes, &f.sha256)?;
            Ok(vec![done(f, "read back with its SHA-256".to_owned())])
        }
        Unit::Table(table, files) => {
            for f in files {
                let path = dir.join(&f.path);
                local(&path, f)?;
                store.execute_file(&path)?;
            }
            let want = manifest
                .d1
                .tables
                .iter()
                .find(|t| t.table == table.name)
                .ok_or_else(|| format!("the manifest gives no fingerprint of {}", table.name))?;
            let rows = check_rows(store, table, n, want)?;
            // The count and fingerprint both matched.
            let checked = format!("{}: {rows} rows of release {n}", table.name);
            Ok(files.iter().map(|f| done(f, checked.clone())).collect())
        }
    }
}

fn unit_files<'u>(unit: &'u Unit<'_>) -> Vec<&'u ManifestFile> {
    match unit {
        Unit::Schema(f) | Unit::R2(f) => vec![f],
        Unit::Table(_, files) => files.clone(),
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
fn record_published(store: &mut dyn Store, number: u32, at: &str) -> Result<(), String> {
    store
        .query(&format!(
            "UPDATE releases SET published_at = {} WHERE number = {number} AND published_at IS \
             NULL",
            sql_text(at)
        ))
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
    options: &PublishOptions<'_>,
    progress: &mut dyn FnMut(String),
) -> Result<Published, PublishError> {
    let (dir, manifest, manifest_bytes) = read_release(options.out, number)
        .map_err(|problem| PublishError::NotBuilt { number, problem })?;
    let total = manifest.files.len() + 1;
    let stopped = |latest: Latest, done: usize, problem: String| PublishError::Interrupted {
        number,
        latest,
        done,
        total,
        problem,
    };
    let latest = refuse_published(store, number).map_err(|e| match e {
        Refused::Error(e) => e,
        Refused::Query(p) => stopped(Latest(None), 0, p),
    })?;
    within_cap(store, number, manifest.d1.estimated_bytes).map_err(|e| match e {
        Refused::Error(e) => e,
        Refused::Query(p) => stopped(latest, 0, p),
    })?;
    // The manifest itself goes last, read back against its local bytes.
    let itself = ManifestFile {
        path: keys::MANIFEST.to_owned(),
        target: Target::R2,
        key: Some(keys::key(number, keys::MANIFEST)),
        bytes: manifest_bytes.len() as u64,
        sha256: sha256(&manifest_bytes),
    };
    let mut journal = open_journal(options, number, &itself.sha256)?;
    let journaled = journal.done.len();
    if journaled > 0 {
        progress(format!(
            "release {number}: {journaled} of {total} files already uploaded and checked, by \
             the journal"
        ));
    }
    let mut units = units(&manifest).map_err(|p| PublishError::NotBuilt { number, problem: p })?;
    units.push(Unit::R2(&itself));
    for unit in &units {
        if unit_files(unit)
            .iter()
            .all(|f| journal.has(&f.path, &f.sha256))
        {
            continue;
        }
        let done = upload(store, &dir, &manifest, unit, &(options.now)())
            .map_err(|p| stopped(latest, journal.done.len(), p))?;
        for d in done {
            progress(format!("release {number}: {}: {}", d.path, d.checked));
            journal.done.push(d);
        }
        journal
            .write(options.journals)
            .map_err(|e| stopped(latest, journal.done.len(), format!("the journal: {e}")))?;
    }
    let at = (options.now)();
    store
        .query(&move_latest(number, &at))
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

/// Checks the estimate `this` of release `number`'s rows, with those of every other release D1
/// lists that is not pruned, against the cap (CON-3).
fn within_cap(store: &mut dyn Store, number: u32, this: u64) -> Result<(), Refused> {
    let status = published_status(store, number).map_err(Refused::Query)?;
    let (others, releases) = if status.schema {
        let rows = store
            .query(&format!(
                "SELECT total(d1_estimated_bytes) AS bytes, COUNT(*) AS n FROM releases WHERE \
                 pruned_at IS NULL AND number <> {number}"
            ))
            .map_err(Refused::Query)?;
        let row = rows.first();
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a sum of byte counts, whole and positive"
        )]
        let bytes = row
            .and_then(|r| r.get("bytes"))
            .and_then(serde_json::Value::as_f64)
            .map_or(0, |b| b as u64);
        let n = row
            .and_then(|r| r.get("n"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        (bytes, n)
    } else {
        (0, 0)
    };
    if this + others > D1_CAP_BYTES {
        return Err(Refused::Error(PublishError::OverCap {
            number,
            this,
            others,
            releases,
        }));
    }
    Ok(())
}

/// Why publishing was refused before anything was uploaded.
enum Refused {
    Error(PublishError),
    Query(String),
}

/// Where `latest` points, once release `number` is found neither published nor pruned, nor
/// older than the release `latest` points to.
fn refuse_published(store: &mut dyn Store, number: u32) -> Result<Latest, Refused> {
    let status = published_status(store, number).map_err(Refused::Query)?;
    let latest = Latest(status.latest.as_ref().map(|l| l.0));
    let refused = |e| Err(Refused::Error(e));
    if let Some(published_at) = status.published_at {
        return refused(PublishError::AlreadyPublished {
            number,
            published_at,
            latest,
        });
    }
    if let Some(pruned_at) = status.pruned_at {
        return refused(PublishError::Pruned { number, pruned_at });
    }
    match status.latest {
        // Moved by a run stopped before it recorded the publication time: record it now.
        Some((n, moved_at)) if n == number => {
            record_published(store, number, &moved_at).map_err(Refused::Query)?;
            refused(PublishError::AlreadyPublished {
                number,
                published_at: moved_at,
                latest,
            })
        }
        Some((n, _)) if n > number => refused(PublishError::NotNewer { number, latest: n }),
        _ => Ok(latest),
    }
}

/// The journal of release `number` under the manifest with SHA-256 `manifest_sha`: the one an
/// earlier run left, or a new one.
fn open_journal(
    options: &PublishOptions<'_>,
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
