//! A local D1 and R2 that hold releases of schema 5, whose rows each belong to one release and
//! whose files R2 keeps under each release's prefix, brought under shared rows (schema 6) in
//! place before a publish uploads anything (publish-a-release-locally). Nothing is built or
//! uploaded again, and every read pinned to such a release answers as before.
//!
//! - Each table of [`Scope::PerRelease`] in each database is rebuilt under the keys of
//!   schema 6, with `retired`: the rows of the release `latest` points to are valid from it on,
//!   and those of every other release only in it, retired at the release after it.
//! - Each file of a listed release that is not pruned is copied in R2 to its SHA-256's key,
//!   and the release's map of its paths, `release_files`, written. A grid's header names its
//!   samples by their keys, so it is rewritten to name them as a build of schema 6 does, and
//!   its row in `grids` names the header's new key.
//!   Each row naming a file by its old key (every column ending `_key`: `grids.header_key`,
//!   `scale_domains.manifest_key`) names its new key.
//! - Once every database is at schema 6, the files' old keys are deleted: they are listed in
//!   the journals' directory first, so a run stopped while it deleted them finishes the next.
//!
//! A release a publish of schema 5 left part way, listed but never published, is refused: it
//! has no manifest in R2 to move its files by, and is pruned first.
//!
//! Every step can run again: a table is rebuilt from itself, a file copied again, a row written
//! or ignored, and a database already at schema 6 left alone; an interrupted migration is
//! finished by the next run, of `univ-ingest migrate` or a publish.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use serde_json::Value;
use univ_model::release::manifest::Target;
use univ_model::release::shards::Database;
use univ_model::release::{
    RETIRED, SCHEMA, SCHEMA_VERSION, SHARED_ROWS_SINCE, Scope, TABLES, Table, keys,
};

use super::Store;
use super::check::{
    Tables, execute_sql, kept_releases, published_status, r2_manifest, schema_version, sql_text,
    tables_of,
};
use super::columns::{Beyond, differences};
use super::progress::{Lines, Progress};
use super::upload::put_checked;
use crate::tiles::sha256;

/// The schema version brought under shared rows.
pub const FROM: i64 = 5;

/// The directory within the journals' that holds the files a migration executes.
const WORK: &str = "migrate";

/// The statement that creates `table` in [`SCHEMA`], named `name`: from its `CREATE TABLE` to
/// the first `;` ending a line.
pub(crate) fn create_as(table: &Table, name: &str) -> Result<String, String> {
    let head = format!("CREATE TABLE IF NOT EXISTS {} (", table.name);
    let at = SCHEMA
        .find(&head)
        .ok_or_else(|| format!("the schema creates no table {}", table.name))?;
    let end = SCHEMA[at..]
        .find(";\n")
        .map(|e| at + e + 1)
        .ok_or_else(|| format!("the schema's table {} is not closed", table.name))?;
    Ok(SCHEMA[at..end].replacen(&head, &format!("CREATE TABLE {name} ("), 1))
}

/// The statements that rebuild each table of [`Scope::PerRelease`] of `held` under the keys of
/// schema 6, the rows of each release before `latest` retired at the release after it as they
/// are copied: those of a release after it, which a publish stopped part way loaded, stay valid
/// from their release. A run stopped after it dropped a table, before it renamed the rebuilt one, left the
/// rows only in the rebuilt one: it takes the table's name back first. A table keeps the columns
/// `beyond` gives it, which a later migration reads ([`crate::release::schema8`]); a column of
/// [`SCHEMA`] the table lacks is NULL.
fn rebuild(
    held: &std::collections::BTreeSet<String>,
    beyond: &Beyond,
    lacks: &BTreeMap<String, Vec<String>>,
    latest: Option<u32>,
) -> Result<String, String> {
    let retired = latest.map_or_else(
        || "NULL".to_owned(),
        |latest| format!("CASE WHEN release < {latest} THEN release + 1 END"),
    );
    let mut sql = String::new();
    for table in TABLES.iter().filter(|t| t.scope == Scope::PerRelease) {
        let name = table.name;
        let next = format!("{name}__v6");
        if !held.contains(name) {
            if !held.contains(&next) {
                continue;
            }
            let _ = writeln!(sql, "ALTER TABLE {next} RENAME TO {name};");
        }
        let missing = lacks.get(name).map(Vec::as_slice).unwrap_or_default();
        let kept = beyond.get(name).map(Vec::as_slice).unwrap_or_default();
        let mut columns: Vec<&str> = table
            .columns
            .iter()
            .copied()
            .filter(|c| !missing.iter().any(|m| m == c))
            .collect();
        columns.extend(kept.iter().map(|(c, _)| c.as_str()));
        let columns = columns.join(", ");
        let _ = writeln!(
            sql,
            "DROP TABLE IF EXISTS {next};\n{};",
            create_as(table, &next)?
        );
        for (column, kind) in kept {
            let _ = writeln!(sql, "ALTER TABLE {next} ADD COLUMN {column} {kind};");
        }
        let _ = write!(
            sql,
            "INSERT INTO {next} ({columns}, {RETIRED}) SELECT {columns}, {retired} FROM {name};\n\
             DROP TABLE {name};\nALTER TABLE {next} RENAME TO {name};\n"
        );
    }
    Ok(sql)
}

/// A grid header's `fields` with each samples key under release `number`'s prefix renamed to
/// the key of its file's SHA-256 by `shas`, its path's: the header a build of schema 6 writes.
fn rename_samples(
    header: &[u8],
    number: u32,
    shas: &BTreeMap<&str, &str>,
) -> Result<Vec<u8>, String> {
    let mut json: Value = serde_json::from_slice(header).map_err(|e| e.to_string())?;
    let prefix = keys::prefix(number);
    let rename = |key: &mut Value| -> Result<(), String> {
        let Some(old) = key.as_str() else {
            return Ok(());
        };
        let Some(path) = old.strip_prefix(&prefix) else {
            return Ok(());
        };
        let sha = shas
            .get(path)
            .ok_or_else(|| format!("the header names {old}, which the manifest does not list"))?;
        *key = Value::String(keys::file(sha));
        Ok(())
    };
    for field in json
        .get_mut("fields")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        if let Some(samples) = field.get_mut("samples") {
            rename(samples)?;
        }
        if let Some(samples) = field
            .get_mut("uncertainty")
            .and_then(|u| u.get_mut("samples"))
        {
            rename(samples)?;
        }
    }
    serde_json::to_vec_pretty(&json).map_err(|e| e.to_string())
}

/// The file listing the old keys a migration deletes once the catalogue is at schema 6.
const OLD_KEYS: &str = "old-keys.yaml";

/// The table of the catalogue mapping each old key to its new one while the rows naming them
/// are rewritten.
const MOVED_KEYS: &str = "migrate_moved_keys";

/// What moving the releases' files left to do in D1, and the keys to delete once done.
#[derive(Default)]
struct Moved {
    /// `release_files` rows and the old keys' new ones, for the catalogue.
    sql: String,
    old_keys: Vec<String>,
}

impl Moved {
    /// The statements that have every column of the catalogue naming a file by key (its name
    /// ends `_key`) name the file's new key, through [`MOVED_KEYS`].
    fn rename_keys(&self) -> String {
        let mut sql = format!(
            "DROP TABLE IF EXISTS {MOVED_KEYS};\nCREATE TABLE {MOVED_KEYS} (old TEXT PRIMARY \
             KEY, new TEXT NOT NULL);\n{}",
            self.sql
        );
        for table in &TABLES {
            for column in table.columns.iter().filter(|c| c.ends_with("_key")) {
                let name = table.name;
                let _ = writeln!(
                    sql,
                    "UPDATE {name} SET {column} = (SELECT new FROM {MOVED_KEYS} WHERE old = \
                     {column}) WHERE {column} IN (SELECT old FROM {MOVED_KEYS});"
                );
            }
        }
        let _ = writeln!(sql, "DROP TABLE {MOVED_KEYS};");
        sql
    }
}

/// The run stops here when the owner asked it to: every step before is done, and the next
/// run takes the rest.
fn stop_if_asked(progress: &dyn Progress) -> Result<(), String> {
    if progress.stopping() {
        Err("stopped by the owner; migrating again finishes it".to_owned())
    } else {
        Ok(())
    }
}

/// Copies each R2 file of release `number` to its SHA-256's key, checking what R2 gives back,
/// rewriting grid headers, and adds to `moved` its map of paths and its old keys: its rows
/// retired at the release after it unless it is `latest`. `work` holds each file on its way.
fn move_files(
    store: &mut dyn Store,
    (number, latest): (u32, Option<u32>),
    work: &Path,
    moved: &mut Moved,
    progress: &dyn Progress,
) -> Result<(), String> {
    let manifest = r2_manifest(store, number)?.ok_or_else(|| {
        format!(
            "R2 holds no manifest of release {number} at {}",
            keys::manifest(number)
        )
    })?;
    let r2: Vec<_> = manifest
        .files
        .iter()
        .filter(|f| f.target == Target::R2)
        .collect();
    let shas: BTreeMap<&str, &str> = r2
        .iter()
        .map(|f| (f.path.as_str(), f.sha256.as_str()))
        .collect();
    let retired = if Some(number) == latest {
        "NULL".to_owned()
    } else {
        (number + 1).to_string()
    };
    let file = work.join("object");
    for f in &r2 {
        let Some(old) = f.key.as_deref().filter(|k| !k.starts_with(keys::FILES)) else {
            continue;
        };
        stop_if_asked(progress)?;
        // The old keys are deleted only once the catalogue is migrated, which ends the run.
        let held = store
            .get(old)?
            .ok_or_else(|| format!("R2 holds no {old}, which release {number}'s manifest lists"))?;
        let header = f.path.starts_with("grids/") && f.path.ends_with("/header.json");
        let (sha, bytes) = if header {
            let renamed = rename_samples(&held, number, &shas)?;
            (sha256(&renamed), renamed)
        } else {
            (f.sha256.clone(), held)
        };
        std::fs::write(&file, &bytes).map_err(|e| format!("{}: {e}", file.display()))?;
        put_checked(store, &keys::file(&sha), &file, bytes.len() as u64, &sha)?;
        let _ = writeln!(
            moved.sql,
            "INSERT OR IGNORE INTO release_files (release, path, sha256, {RETIRED}) VALUES \
             ({number}, {}, {}, {retired});",
            sql_text(&f.path),
            sql_text(&sha)
        );
        let _ = writeln!(
            moved.sql,
            "INSERT OR REPLACE INTO {MOVED_KEYS} (old, new) VALUES ({}, {});",
            sql_text(old),
            sql_text(&keys::file(&sha))
        );
        moved.old_keys.push(old.to_owned());
    }
    let _ = std::fs::remove_file(&file);
    Ok(())
}

/// Whether `database`, whose tables `tables` lists, is of schema [`FROM`].
fn at_from(store: &mut dyn Store, tables: &Tables, database: Database) -> Result<bool, String> {
    let held = tables
        .get(&database)
        .is_some_and(|t| t.contains("schema_version"));
    Ok(held && schema_version(store, database)? == Some(FROM))
}

/// Whether a table of [`Scope::PerRelease`] that `database` holds lacks [`RETIRED`]: a database
/// given the tables before rows were shared, whatever its `schema_version` says. A database bound
/// but never loaded has no version at all, and a publish creating the schema over its tables
/// records schema 6 without changing them.
fn unshared(store: &mut dyn Store, tables: &Tables, database: Database) -> Result<bool, String> {
    let Some(table) = TABLES.iter().find(|t| {
        t.scope == Scope::PerRelease && tables.get(&database).is_some_and(|h| h.contains(t.name))
    }) else {
        return Ok(false);
    };
    let sql = format!(
        "SELECT COUNT(*) AS n FROM pragma_table_info('{}') WHERE name = '{RETIRED}'",
        table.name
    );
    let rows = store.query(database, &sql)?;
    Ok(rows
        .first()
        .and_then(|r| r.get("n"))
        .and_then(serde_json::Value::as_i64)
        == Some(0))
}

/// Brings each database of `store` that holds releases of schema [`FROM`] under shared rows,
/// with R2's files, as the module says, executing its files from `journals`, and any shard
/// whose tables lack [`RETIRED`] (see [`unshared`]), whenever it is found. Whether it migrated:
/// nothing is done where the catalogue is not of that version and no shard is unshared. Once `progress` says
/// the owner asked it to stop, it stops after the file or database in hand. A run stopped while
/// it deleted the old keys leaves some, which a prune of their release deletes, as its manifest
/// names them.
///
/// # Errors
///
/// Fails part way when D1 or R2 refuses a step; the next run finishes it.
pub fn migrate_with(
    store: &mut dyn Store,
    journals: &Path,
    progress: &mut dyn Progress,
) -> Result<bool, String> {
    let work = journals.join(WORK);
    let listed = work.join(OLD_KEYS);
    // The catalogue is migrated last, so a store whose catalogue is past schema 5 is done but
    // for the old keys a run stopped before it deleted them all.
    let mut tables = tables_of(store, std::iter::once(Database::Catalogue))?;
    let catalogue = at_from(store, &tables, Database::Catalogue)?;
    if catalogue {
        let partial = store.query(
            Database::Catalogue,
            "SELECT number FROM releases WHERE pruned_at IS NULL AND published_at IS NULL",
        )?;
        if let Some(row) = partial.first() {
            return Err(format!(
                "release {} was published only in part under schema {FROM}: prune it, then \
                 migrate",
                row.get("number")
                    .map_or_else(String::new, ToString::to_string)
            ));
        }
    }
    let bound = store.bound_shards()?;
    tables.extend(tables_of(store, (0..bound).map(Database::Shard))?);
    // The shards first, the catalogue last.
    let mut older = Vec::new();
    for (database, _) in tables.iter().filter(|(d, _)| **d != Database::Catalogue) {
        if at_from(store, &tables, *database)? || unshared(store, &tables, *database)? {
            older.push(*database);
        }
    }
    if !catalogue {
        let rebuilt = !older.is_empty();
        let latest = published_status(store, 0)?.latest.map(|(n, _)| n);
        for database in &older {
            rebuild_one(
                store,
                &work,
                (*database, &tables[database]),
                latest,
                "",
                progress,
            )?;
        }
        return Ok(delete_old_keys(store, &listed, progress)? || rebuilt);
    }
    older.push(Database::Catalogue);
    std::fs::create_dir_all(&work).map_err(|e| format!("{}: {e}", work.display()))?;
    let latest = published_status(store, 0)?.latest.map(|(n, _)| n);
    // The files are moved whichever database is left: a run stopped before it deleted the old
    // keys moves what is left of them, and its catalogue holds their rows already.
    let mut moved = Moved::default();
    for number in kept_releases(store, 0)? {
        move_files(store, (number, latest), &work, &mut moved, progress)?;
    }
    progress.note(format!(
        "schema {FROM} to {SCHEMA_VERSION}: {} R2 files copied to the keys of their SHA-256",
        moved.old_keys.len()
    ));
    for database in &older {
        let mut extra = String::new();
        if *database == Database::Catalogue {
            extra = moved.rename_keys();
            let text = serde_norway::to_string(&moved.old_keys).map_err(|e| e.to_string())?;
            std::fs::write(&listed, text).map_err(|e| format!("{}: {e}", listed.display()))?;
        }
        rebuild_one(
            store,
            &work,
            (*database, &tables[database]),
            latest,
            &extra,
            progress,
        )?;
    }
    delete_old_keys(store, &listed, progress)
}

/// Rebuilds `database`, which holds the tables `held`, under shared rows (see [`rebuild`]),
/// with `extra` statements after the schema, and records schema 6 in it.
fn rebuild_one(
    store: &mut dyn Store,
    work: &Path,
    (database, held): (Database, &std::collections::BTreeSet<String>),
    latest: Option<u32>,
    extra: &str,
    progress: &mut dyn Progress,
) -> Result<(), String> {
    stop_if_asked(progress)?;
    let (beyond, lacks) = differences(store, database, held)?;
    let mut sql = rebuild(held, &beyond, &lacks, latest)?;
    sql.push_str(SCHEMA);
    sql.push_str(extra);
    let _ = write!(
        sql,
        "\nINSERT OR IGNORE INTO schema_version (version) VALUES ({SHARED_ROWS_SINCE});\n"
    );
    std::fs::create_dir_all(work).map_err(|e| format!("{}: {e}", work.display()))?;
    let file = work.join(format!("migrate-{}.sql", database.dir()));
    execute_sql(store, database, &file, &sql)?;
    progress.note(format!(
        "schema {FROM} to {SCHEMA_VERSION}: {database} rebuilt with its rows shared between releases"
    ));
    Ok(())
}

/// Deletes the old keys `listed` names, if it is there, then the list: whether there was one.
fn delete_old_keys(
    store: &mut dyn Store,
    listed: &Path,
    progress: &mut dyn Progress,
) -> Result<bool, String> {
    let text = match std::fs::read_to_string(listed) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(format!("{}: {e}", listed.display())),
    };
    let old: Vec<String> =
        serde_norway::from_str(&text).map_err(|e| format!("{}: {e}", listed.display()))?;
    store.delete_keys(&old)?;
    std::fs::remove_file(listed).map_err(|e| format!("{}: {e}", listed.display()))?;
    progress.note(format!(
        "schema {FROM} to {SCHEMA_VERSION}: {} R2 keys under the releases' prefixes deleted",
        old.len()
    ));
    Ok(true)
}

/// [`migrate_with`], telling `progress` a line for each step.
///
/// # Errors
///
/// As [`migrate_with`].
pub fn migrate(
    store: &mut dyn Store,
    journals: &Path,
    progress: &mut dyn FnMut(String),
) -> Result<bool, String> {
    migrate_with(store, journals, &mut Lines(progress))
}

#[cfg(test)]
#[path = "migrate_tests.rs"]
mod tests;
