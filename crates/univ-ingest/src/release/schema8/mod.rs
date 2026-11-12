//! A store of schema 7 brought to schema 8 in place (keep-json-out-of-the-tables-that-grow-with-
//! a-releases-objects): every JSON column of a table that grows with a release's objects becomes
//! rows and columns of schema 8, each row valid in the releases its parent row is, and the tables
//! are rebuilt without them. Nothing is built or published again: every record a release served
//! reads the same after.
//!
//! - The JSON is read in Rust with univ-model's own types, never with SQL's JSON functions, and
//!   written through the builders a build writes the same rows with ([`super::extras`]).
//! - Each database is migrated in one transaction: the object shards first, the catalogue last.
//!   A run stopped part way leaves each database at 7 or at 8, never between, and the next run
//!   migrates those left at 7; a database at 8 is left alone.
//! - A term no release named, such as a flag's name, is numbered after every term ever numbered,
//!   as a build numbers a new one, and written to the catalogue before the database that uses it
//!   is committed; `univ-ingest reindex` then makes the state's numbering and row index again.
//!
//! [`migrate_local_d1`] works on the `SQLite` files of the local D1 that `wrangler dev` keeps,
//! which no Worker may have open while it runs: a clone of the state, swapped in once migrated.

mod codes;
mod convert;
mod sql;

use std::path::Path;

use rusqlite::Connection;
use univ_model::release::{LOCAL_D1_FILES, SCHEMA};

pub use codes::StoreCodes;
use sql::{Temp, columns_of, rebuild, rebuilt, sql_err};

/// The version this migration brings a database to.
pub const TO: u32 = 8;

/// The JSON columns of schema 7 that schema 8 replaces, by table.
pub const JSON_COLUMNS: [(&str, &str); 6] = [
    ("objects", "links"),
    ("objects", "relations"),
    ("entries", "flags"),
    ("entries", "extra"),
    ("measurements", "extra"),
    ("ids_replaced", "replaced_by"),
];

/// Whether `db` holds a JSON column that schema 8 replaces.
///
/// # Errors
///
/// Fails when its tables cannot be read.
pub fn holds_json(db: &Connection) -> Result<bool, String> {
    for (table, column) in JSON_COLUMNS {
        if columns_of(db, table)?.contains(column) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// How many rows of each table a database's migration read for their JSON.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Converted {
    pub entries: u64,
    pub measurements: u64,
    pub objects: u64,
    pub replaced: u64,
}

/// Writes the terms `added` into the catalogue `catalogue`, valid from `from` on.
fn write_terms(catalogue: &Connection, from: u32, added: &[(u32, String)]) -> Result<(), String> {
    let sql = "INSERT OR IGNORE INTO terms (release, id, term, retired) VALUES (?1, ?2, ?3, NULL)";
    let mut q = catalogue.prepare_cached(sql).map_err(sql_err(sql))?;
    for (id, term) in added {
        q.execute(rusqlite::params![from, id, term])
            .map_err(sql_err(sql))?;
    }
    Ok(())
}

/// Brings `db` to schema 8 in one transaction, its new terms written to `catalogue`, or to
/// `db` itself when it is the catalogue, valid from release `from` on.
fn migrate_database(
    db: &Connection,
    codes: &mut StoreCodes,
    catalogue: Option<&Connection>,
    from: u32,
) -> Result<Converted, String> {
    let tx = db
        .unchecked_transaction()
        .map_err(|e| format!("a transaction cannot begin: {e}"))?;
    // Schema 8's new tables; the tables it rebuilds keep their columns until rebuilt.
    db.execute_batch(SCHEMA).map_err(sql_err("the schema"))?;
    let temp = Temp::new(db)?;
    let mut done = Converted::default();
    let mut pending = convert::Pending::new();
    if columns_of(db, "entries")?.contains("flags") {
        done.entries = convert::entries(db, codes, &temp, &mut pending)?;
    }
    if columns_of(db, "measurements")?.contains("extra") {
        done.measurements = convert::measurements(db, codes, &temp)?;
    }
    if columns_of(db, "objects")?.contains("links") {
        done.objects = convert::objects(db, codes)?;
    }
    if columns_of(db, "ids_replaced")?.contains("replaced_by") {
        done.replaced = convert::replaced(db)?;
    }
    codes.check()?;
    for table in rebuilt() {
        let held = columns_of(db, table.name)?;
        if !held.is_empty()
            && held
                .iter()
                .any(|c| !table.columns.contains(&c.as_str()) && c != univ_model::release::RETIRED)
        {
            rebuild(db, table)?;
        }
    }
    for (row, retired) in &pending {
        sql::insert(db, std::slice::from_ref(row), *retired)?;
    }
    // The indexes of the rebuilt tables, dropped with them.
    db.execute_batch(SCHEMA).map_err(sql_err("the schema"))?;
    let version = format!("INSERT OR IGNORE INTO schema_version (version) VALUES ({TO})");
    db.execute_batch(&version).map_err(sql_err(&version))?;
    write_terms(catalogue.unwrap_or(db), from, &codes.take_added())?;
    tx.commit()
        .map_err(|e| format!("the migration cannot be committed: {e}"))?;
    Ok(done)
}

/// Brings each of `shards`, then `catalogue`, to schema 8 where it holds a JSON column schema 8
/// replaces, telling `progress` a line for each. Whether any was migrated.
///
/// # Errors
///
/// Fails when a database cannot be read or written, a JSON value does not read as its type, or
/// a row names a source the catalogue does not number; the database in hand is left at 7.
pub fn migrate_databases(
    catalogue: &Connection,
    shards: &[&Connection],
    progress: &mut dyn FnMut(String),
) -> Result<bool, String> {
    let mut older = Vec::new();
    for (i, shard) in shards.iter().enumerate() {
        if holds_json(shard)? {
            older.push(i);
        }
    }
    let catalogue_older = holds_json(catalogue)?;
    if older.is_empty() && !catalogue_older {
        return Ok(false);
    }
    let mut codes = StoreCodes::read(catalogue)?;
    let from: u32 = catalogue
        .query_row("SELECT COALESCE(MIN(release), 1) FROM terms", [], |r| {
            r.get(0)
        })
        .map_err(sql_err("the first release of the terms"))?;
    for i in older {
        let done = migrate_database(shards[i], &mut codes, Some(catalogue), from)?;
        progress(format!(
            "schema 7 to {TO}: shard {i} of the given: {done:?}"
        ));
    }
    if catalogue_older {
        let done = migrate_database(catalogue, &mut codes, None, from)?;
        progress(format!("schema 7 to {TO}: the catalogue: {done:?}"));
    }
    Ok(true)
}

/// [`migrate_databases`] on the local D1 kept in Wrangler's state directory `persist_to`: the
/// catalogue the database whose `latest` points to a release, every other that holds entries an
/// object shard. No Worker may have the state open meanwhile.
///
/// # Errors
///
/// As [`migrate_databases`], and when no database or more than one is the catalogue.
pub fn migrate_local_d1(
    persist_to: &Path,
    progress: &mut dyn FnMut(String),
) -> Result<bool, String> {
    let dir = persist_to.join(LOCAL_D1_FILES);
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "sqlite"))
        .collect();
    files.sort();
    let mut catalogue = None;
    let mut shards = Vec::new();
    for path in files {
        let db = Connection::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        db.execute_batch("PRAGMA cache_size = -262144")
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if columns_of(&db, "entries")?.is_empty() {
            continue;
        }
        let points: Option<i64> = if columns_of(&db, "latest")?.is_empty() {
            None
        } else {
            db.query_row("SELECT release FROM latest", [], |r| r.get(0))
                .ok()
        };
        match (points, &catalogue) {
            (Some(_), Some(_)) => {
                return Err(format!(
                    "two databases in {} have latest point to a release",
                    dir.display()
                ));
            }
            (Some(_), None) => catalogue = Some(db),
            (None, _) => shards.push(db),
        }
    }
    let catalogue =
        catalogue.ok_or_else(|| format!("no database in {} has latest", dir.display()))?;
    let shards: Vec<&Connection> = shards.iter().collect();
    migrate_databases(&catalogue, &shards, progress)
}
