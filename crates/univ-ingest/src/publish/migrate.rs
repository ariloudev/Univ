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
//! - Once every database is at schema 6, the files' old keys are deleted.
//!
//! Every step can run again: a table is rebuilt from itself, a file copied again, a row written
//! or ignored, and a database already at schema 6 left alone; an interrupted migration is
//! finished by the next publish.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use serde_json::Value;
use univ_model::release::manifest::Target;
use univ_model::release::shards::Database;
use univ_model::release::{RETIRED, SCHEMA, SCHEMA_VERSION, Scope, TABLES, Table, keys};

use super::Store;
use super::check::{Tables, kept_releases, published_status, schema_version, sql_text, tables_of};
use super::progress::{Lines, Progress};
use crate::release::parse_manifest;
use crate::tiles::sha256;

/// The schema version brought under shared rows.
pub const FROM: i64 = 5;

/// The statement that creates `table` in [`SCHEMA`], named `name`.
fn create_as(table: &Table, name: &str) -> Result<String, String> {
    let head = format!("CREATE TABLE IF NOT EXISTS {} (", table.name);
    let at = SCHEMA
        .find(&head)
        .ok_or_else(|| format!("the schema creates no table {}", table.name))?;
    // The statement ends at its first `;`, `)` or `) WITHOUT ROWID` before it.
    let end = SCHEMA[at..]
        .find(";\n")
        .map(|e| at + e + 1)
        .ok_or_else(|| format!("the schema's table {} is not closed", table.name))?;
    Ok(SCHEMA[at..end].replacen(&head, &format!("CREATE TABLE {name} ("), 1))
}

/// The statements that rebuild each table of [`Scope::PerRelease`] of `held` under the keys of
/// schema 6, then retire the rows of every release but `latest` at the release after it. A run
/// stopped after it dropped a table, before it renamed the rebuilt one, left the rows only in
/// the rebuilt one: it takes the table's name back first.
fn rebuild(
    held: &std::collections::BTreeSet<String>,
    latest: Option<u32>,
) -> Result<String, String> {
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
        let columns = table.columns.join(", ");
        let _ = write!(
            sql,
            "DROP TABLE IF EXISTS {next};\n{};\nINSERT INTO {next} ({columns}) SELECT {columns} \
             FROM {name};\nDROP TABLE {name};\nALTER TABLE {next} RENAME TO {name};\n",
            create_as(table, &next)?
        );
        if let Some(latest) = latest {
            let _ = writeln!(
                sql,
                "UPDATE {name} SET {RETIRED} = release + 1 WHERE release <> {latest} AND \
                 {RETIRED} IS NULL;"
            );
        }
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
    let prefix = format!("releases/{number}/");
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

/// What moving release `number`'s files left to do in D1, and the keys to delete once done.
#[derive(Default)]
struct Moved {
    /// `release_files` rows and the grids' new header keys, for the catalogue.
    sql: String,
    old_keys: Vec<String>,
    files: usize,
}

/// Puts `bytes` in R2 at `key`, through a file in `work`.
fn put_bytes(store: &mut dyn Store, work: &Path, key: &str, bytes: &[u8]) -> Result<(), String> {
    let file = work.join("object");
    std::fs::write(&file, bytes).map_err(|e| format!("{}: {e}", file.display()))?;
    store.put(key, &file)
}

/// Copies each R2 file of release `number` to its SHA-256's key, rewriting grid headers, and
/// adds to `moved` its map of paths and its old keys. `retired` is the release its rows are
/// retired at, none for the release `latest` points to.
fn move_files(
    store: &mut dyn Store,
    (number, retired): (u32, Option<u32>),
    work: &Path,
    moved: &mut Moved,
) -> Result<(), String> {
    let key = keys::manifest(number);
    let bytes = store
        .get(&key)?
        .ok_or_else(|| format!("R2 holds no manifest of release {number} at {key}"))?;
    let manifest = parse_manifest(Path::new(&key), &String::from_utf8_lossy(&bytes), number)
        .map_err(|e| e.to_string())?;
    let r2: Vec<_> = manifest
        .files
        .iter()
        .filter(|f| f.target == Target::R2)
        .collect();
    let shas: BTreeMap<&str, &str> = r2
        .iter()
        .map(|f| (f.path.as_str(), f.sha256.as_str()))
        .collect();
    let retired = retired.map_or_else(|| "NULL".to_owned(), |r| r.to_string());
    for f in &r2 {
        let Some(old) = f.key.as_deref().filter(|k| !k.starts_with(keys::FILES)) else {
            continue;
        };
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
        put_bytes(store, work, &keys::file(&sha), &bytes)?;
        let _ = writeln!(
            moved.sql,
            "INSERT OR IGNORE INTO release_files (release, path, sha256, {RETIRED}) VALUES \
             ({number}, {}, {}, {retired});",
            sql_text(&f.path),
            sql_text(&sha)
        );
        if header {
            let _ = writeln!(
                moved.sql,
                "UPDATE grids SET header_key = {} WHERE release = {number} AND header_key = {};",
                sql_text(&keys::file(&sha)),
                sql_text(old)
            );
        }
        moved.old_keys.push(old.to_owned());
        moved.files += 1;
    }
    Ok(())
}

/// Brings each database of `store` that holds releases of schema [`FROM`] under shared rows,
/// with R2's files, as the module says; `work` holds the files it executes. Whether it migrated:
/// nothing is done where the catalogue is not of that version. A run stopped while it deleted
/// the old keys leaves some, which a prune of their release deletes, as its manifest names them.
///
/// # Errors
///
/// Fails part way when D1 or R2 refuses a step; the next run finishes it.
pub fn migrate(
    store: &mut dyn Store,
    work: &Path,
    progress: &mut dyn Progress,
) -> Result<bool, String> {
    // The catalogue is migrated last, so a store whose catalogue is past schema 5 is done.
    let catalogue = tables_of(store, std::iter::once(Database::Catalogue))?;
    let at_from = |store: &mut dyn Store, tables: &Tables, database| -> Result<bool, String> {
        let held = tables
            .get(&database)
            .is_some_and(|t| t.contains("schema_version"));
        Ok(held && schema_version(store, database)? == Some(FROM))
    };
    if !at_from(store, &catalogue, Database::Catalogue)? {
        return Ok(false);
    }
    let bound = store.bound_shards()?;
    let tables: Tables = tables_of(
        store,
        std::iter::once(Database::Catalogue).chain((0..bound).map(Database::Shard)),
    )?;
    let mut older = Vec::new();
    for database in tables.keys().copied() {
        if at_from(store, &tables, database)? {
            older.push(database);
        }
    }
    // The shards first, the catalogue last.
    older.sort_by_key(|d| *d == Database::Catalogue);
    std::fs::create_dir_all(work).map_err(|e| format!("{}: {e}", work.display()))?;
    let latest = published_status(store, 0)?.latest.map(|(n, _)| n);
    // The files are moved whichever database is left: a run stopped before it deleted the old
    // keys moves what is left of them, and its catalogue holds their rows already.
    let mut moved = Moved::default();
    for number in kept_releases(store, 0)? {
        let retired = (Some(number) != latest).then_some(number + 1);
        move_files(store, (number, retired), work, &mut moved)?;
    }
    progress.note(format!(
        "schema {FROM} to {SCHEMA_VERSION}: {} R2 files copied to the keys of their SHA-256",
        moved.files
    ));
    for database in &older {
        let mut sql = rebuild(&tables[database], latest)?;
        sql.push_str(SCHEMA);
        if *database == Database::Catalogue {
            sql.push_str(&moved.sql);
        }
        let _ = write!(
            sql,
            "\nINSERT OR IGNORE INTO schema_version (version) VALUES ({SCHEMA_VERSION});\n"
        );
        let file = work.join(format!("migrate-{}.sql", database.dir()));
        std::fs::write(&file, sql).map_err(|e| format!("{}: {e}", file.display()))?;
        store.execute_file(*database, &file)?;
        let _ = std::fs::remove_file(&file);
        progress.note(format!(
            "schema {FROM} to {SCHEMA_VERSION}: {database} rebuilt with its rows shared between releases"
        ));
    }
    store.delete_keys(&moved.old_keys)?;
    let _ = std::fs::remove_file(work.join("object"));
    progress.note(format!(
        "schema {FROM} to {SCHEMA_VERSION}: {} R2 keys under the releases' prefixes deleted",
        moved.old_keys.len()
    ));
    Ok(true)
}

/// [`migrate`], telling `progress` a line for each step.
///
/// # Errors
///
/// As [`migrate`].
pub fn migrate_lines(
    store: &mut dyn Store,
    work: &Path,
    progress: &mut dyn FnMut(String),
) -> Result<bool, String> {
    migrate(store, work, &mut Lines(progress))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_table_left_only_under_its_rebuilt_name_takes_its_name_back_and_is_rebuilt_whole() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        let objects = TABLES.iter().find(|t| t.name == "objects").unwrap();
        db.execute_batch(&create_as(objects, "objects__v6").unwrap())
            .unwrap();
        let columns = objects.columns.join(", ");
        let values = vec!["1"; objects.columns.len()].join(", ");
        db.execute_batch(&format!(
            "INSERT INTO objects__v6 ({columns}) VALUES ({values})"
        ))
        .unwrap();
        // A run stopped between dropping `objects` and renaming `objects__v6`.
        let held = std::collections::BTreeSet::from(["objects__v6".to_owned()]);
        let sql = rebuild(&held, Some(2)).unwrap();
        assert!(
            sql.starts_with("ALTER TABLE objects__v6 RENAME TO objects;\n"),
            "{sql}"
        );
        db.execute_batch(&sql).unwrap();
        let count = |sql: &str| db.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap();
        assert_eq!(count("SELECT COUNT(*) FROM objects WHERE retired = 2"), 1);
        assert_eq!(
            count("SELECT COUNT(*) FROM sqlite_master WHERE name = 'objects__v6'"),
            0
        );
    }

    #[test]
    fn a_grid_header_is_rewritten_to_name_its_samples_by_their_content() {
        let header = |samples: &str, sigma: &str| {
            serde_json::to_vec_pretty(&serde_json::json!({
                "format": "univ-volume-grid",
                "fields": [{
                    "samples": samples,
                    "uncertainty": { "form": "standard_deviation", "samples": sigma },
                }],
            }))
            .unwrap()
        };
        let old = header(
            "releases/4/grids/g/field-0.f32",
            "releases/4/grids/g/field-0-sigma.f32",
        );
        let shas = BTreeMap::from([
            ("grids/g/field-0.f32", "aa"),
            ("grids/g/field-0-sigma.f32", "bb"),
        ]);
        assert_eq!(
            rename_samples(&old, 4, &shas).unwrap(),
            header(&keys::file("aa"), &keys::file("bb"))
        );
        // A key the manifest does not list is an error, not a guess.
        let stray = header(
            "releases/4/grids/g/field-9.f32",
            "releases/4/grids/g/field-0-sigma.f32",
        );
        assert!(rename_samples(&stray, 4, &shas).is_err());
    }
}
