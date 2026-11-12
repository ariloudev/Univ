//! A store of schema 7 brought to schema 8 in place (keep-json-out-of-the-tables-that-grow-with-
//! a-releases-objects): every JSON column of a table that grows with a release's objects becomes
//! rows and columns of schema 8, each row valid in the releases its parent row is, and the tables
//! are rebuilt without them. Nothing is built or published again, and every object record a
//! release served reads the same after. Memberships, which no release of schema 7 recorded, come
//! with the next release built.
//!
//! - The JSON is read in Rust with univ-model's own types, never with SQL's JSON functions, and
//!   written through the builders a build writes the same rows with ([`super::extras`]).
//! - Each database is migrated in one transaction: the object shards first, the catalogue last.
//!   A run stopped part way leaves each database at 7 or at 8, never between, and the next run
//!   migrates those left at 7; a database at 8 is left alone.
//! - A term no release named, such as a flag's name, is numbered after every term ever numbered,
//!   as a build numbers a new one, and written to the catalogue before the database that uses it
//!   is committed. The state's numbering and row index are then made again from the store
//!   (`univ-ingest reindex`, which `univ-ingest migrate` runs).
//!
//! [`migrate_local_d1`] migrates the local D1 it is given, in place, and compacts each file: no
//! Worker may have it open, so it is given a clone of the state, swapped in once migrated.

mod codes;
mod convert;
mod sql;

use std::path::Path;

use rusqlite::Connection;
use univ_model::release::rows::TermRow;
use univ_model::release::{RETIRED, SCHEMA, SCHEMA_7_JSON_HELD, SCHEMA_VERSION, TABLES};

use super::local_d1::{database_files, latest_of};
use super::reindex::holds;
pub use codes::StoreCodes;
use sql::{Inserts, Temp, columns_of, rebuild, rebuilt, sql_err};

/// The first column of schema 7's JSON that `db` still holds, as `table.column`; none when it
/// holds none.
///
/// # Errors
///
/// Fails when its tables cannot be read.
pub fn json_held(db: &Connection) -> Result<Option<String>, String> {
    let sql = SCHEMA_7_JSON_HELD;
    let mut q = db.prepare(sql).map_err(sql_err(sql))?;
    let mut rows = q.query([]).map_err(sql_err(sql))?;
    rows.next()
        .map_err(sql_err(sql))?
        .map(|r| r.get(0))
        .transpose()
        .map_err(sql_err(sql))
}

/// Whether `db` holds a JSON column that schema 8 replaces.
///
/// # Errors
///
/// Fails when its tables cannot be read.
pub fn holds_json(db: &Connection) -> Result<bool, String> {
    Ok(json_held(db)?.is_some())
}

/// How many rows of each table a database's migration read for their JSON.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Converted {
    pub entries: u64,
    pub measurements: u64,
    pub objects: u64,
    pub replaced: u64,
}

/// A store's migration: its codes, and the release its new terms are valid from.
struct Migration {
    codes: StoreCodes,
    from: u32,
}

impl Migration {
    fn new(catalogue: &Connection) -> Result<Self, String> {
        let sql = "SELECT COALESCE(MIN(release), 1) FROM terms";
        Ok(Self {
            codes: StoreCodes::read(catalogue)?,
            from: catalogue
                .query_row(sql, [], |r| r.get(0))
                .map_err(sql_err(sql))?,
        })
    }

    /// Writes the terms numbered since the last call into `catalogue`: in a transaction of its
    /// own unless `within` one already.
    fn write_terms(&mut self, catalogue: &Connection, within: bool) -> Result<(), String> {
        let rows: Vec<TermRow> = self
            .codes
            .take_added()
            .into_iter()
            .map(|(id, term)| TermRow {
                release: self.from,
                id,
                term,
            })
            .collect();
        if within || rows.is_empty() {
            return Inserts::new(catalogue).rows(&rows, None);
        }
        let tx = catalogue
            .unchecked_transaction()
            .map_err(|e| format!("a transaction cannot begin: {e}"))?;
        Inserts::new(catalogue).rows(&rows, None)?;
        tx.commit()
            .map_err(|e| format!("the terms cannot be committed: {e}"))
    }

    /// Brings `db` to schema 8 in one transaction, its new terms written to `catalogue`, or to
    /// `db` itself when it is the catalogue.
    fn database(
        &mut self,
        db: &Connection,
        catalogue: Option<&Connection>,
    ) -> Result<Converted, String> {
        let tx = db
            .unchecked_transaction()
            .map_err(|e| format!("a transaction cannot begin: {e}"))?;
        schema_8_tables(db)?;
        let temp = Temp::new(db)?;
        let mut done = Converted::default();
        let mut pending = convert::Pending::new();
        let codes = &mut self.codes;
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
            let old = columns_of(db, table.name)?;
            if old
                .iter()
                .any(|c| c != RETIRED && !table.columns.contains(&c.as_str()))
            {
                rebuild(db, table, &old)?;
            }
        }
        Inserts::new(db).each(pending.iter().map(|(row, retired)| (row, *retired)))?;
        // The indexes of the new tables, made once their rows are in.
        db.execute_batch(SCHEMA).map_err(sql_err("the schema"))?;
        let version =
            format!("INSERT OR IGNORE INTO schema_version (version) VALUES ({SCHEMA_VERSION})");
        db.execute_batch(&version).map_err(sql_err(&version))?;
        match catalogue {
            Some(catalogue) => self.write_terms(catalogue, false)?,
            None => self.write_terms(db, true)?,
        }
        tx.commit()
            .map_err(|e| format!("the migration cannot be committed: {e}"))?;
        Ok(done)
    }
}

/// Gives `db` the tables of schema 8 it lacks, without the indexes of those tables, which are
/// made once their rows are written.
fn schema_8_tables(db: &Connection) -> Result<(), String> {
    let mut made = Vec::new();
    for table in &TABLES {
        if !holds(db, table.name)? {
            made.push(table.name);
        }
    }
    db.execute_batch(SCHEMA).map_err(sql_err("the schema"))?;
    let sql = "SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = ?1 \
               AND sql IS NOT NULL";
    for table in made {
        let mut q = db.prepare(sql).map_err(sql_err(sql))?;
        let indexes: Vec<String> = q
            .query_map([table], |r| r.get(0))
            .map_err(sql_err(sql))?
            .collect::<Result<_, _>>()
            .map_err(sql_err(sql))?;
        for index in indexes {
            let drop = format!("DROP INDEX {index}");
            db.execute_batch(&drop).map_err(sql_err(&drop))?;
        }
    }
    Ok(())
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
    let mut m = Migration::new(catalogue)?;
    for i in older {
        let done = m.database(shards[i], Some(catalogue))?;
        progress(format!("schema 7 to 8: shard {i} of the given: {done:?}"));
    }
    if catalogue_older {
        let done = m.database(catalogue, None)?;
        progress(format!("schema 7 to 8: the catalogue: {done:?}"));
    }
    Ok(true)
}

/// `path`, opened to be migrated, with a page cache fit for rewriting a shard, its temporary
/// tables' too.
fn open(path: &Path) -> Result<Connection, String> {
    let shown = path.display();
    let db = Connection::open(path).map_err(|e| format!("{shown}: {e}"))?;
    db.execute_batch("PRAGMA cache_size = -262144; PRAGMA temp.cache_size = -262144;")
        .map_err(|e| format!("{shown}: {e}"))?;
    Ok(db)
}

/// Gives back the space of the tables a migration dropped: the write-ahead log checkpointed
/// into `db`'s file, and the file vacuumed.
fn compact(db: &Connection, path: &Path) -> Result<(), String> {
    db.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM;")
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// [`migrate_databases`] on the local D1 kept in Wrangler's state directory `persist_to`, one
/// database at a time and each compacted once migrated: the catalogue the database whose
/// `latest` points to a release, every other that holds objects an object shard. Nothing to do
/// where it holds no local D1 or none of schema 7's JSON. No Worker may have it open.
///
/// # Errors
///
/// As [`migrate_databases`], and when a database holds schema 7's JSON but no database or more
/// than one is the catalogue.
pub fn migrate_local_d1(
    persist_to: &Path,
    progress: &mut dyn FnMut(String),
) -> Result<bool, String> {
    let mut catalogue = None;
    let mut shards = Vec::new();
    let mut json = false;
    for path in database_files(persist_to)? {
        let db = open(&path)?;
        if !holds(&db, "objects")? {
            continue;
        }
        json |= holds_json(&db)?;
        match (latest_of(&db)?, &catalogue) {
            (Some(_), Some(_)) => {
                return Err(format!(
                    "two databases in {} have latest point to a release",
                    persist_to.display()
                ));
            }
            (Some(_), None) => catalogue = Some(path),
            (None, _) => shards.push(path),
        }
    }
    if !json {
        return Ok(false);
    }
    let path =
        catalogue.ok_or_else(|| format!("no database in {} has latest", persist_to.display()))?;
    let catalogue = open(&path)?;
    let mut m = Migration::new(&catalogue)?;
    for shard in &shards {
        let db = open(shard)?;
        if !holds_json(&db)? {
            continue;
        }
        let done = m.database(&db, Some(&catalogue))?;
        compact(&db, shard)?;
        progress(format!("schema 7 to 8: {}: {done:?}", shard.display()));
    }
    if holds_json(&catalogue)? {
        let done = m.database(&catalogue, None)?;
        compact(&catalogue, &path)?;
        progress(format!("schema 7 to 8: the catalogue: {done:?}"));
    }
    Ok(true)
}
