//! An in-memory [`Store`]: each D1 database as an in-memory `SQLite` database, which is what D1
//! runs, the catalogue's and each object shard's, and R2 as a map of keys to bytes. It records every operation, can fail the upload after a given
//! number, and can look at D1 and R2 after each operation.

#![allow(dead_code, reason = "each test file uses a part of the store")]

use std::cell::Cell;
use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::Connection;
use rusqlite::types::ValueRef;
use serde_json::{Map, Value};
use univ_ingest::publish::{
    PruneError, Pruned, PublishError, Published, QueryRow, Store, StoreOptions, prune, publish,
};
use univ_model::release::shards::Database;

/// An operation the store was asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    Execute(String),
    Query(String),
    Put(String),
    Get(String),
    Delete(String),
}

impl Op {
    /// Whether it uploads: executes a D1 file or puts an R2 object.
    pub fn uploads(&self) -> bool {
        matches!(self, Self::Execute(_) | Self::Put(_))
    }
}

/// Looks at D1 and R2 after an operation: the catalogue database, the database the operation
/// ran in, and R2.
pub type Observer = Box<dyn FnMut(&Op, &Connection, &Connection, &BTreeMap<String, Vec<u8>>)>;

pub struct MemoryStore {
    /// The catalogue database.
    pub db: Connection,
    /// Each object shard's database, made when first used.
    pub shards: BTreeMap<u32, Connection>,
    /// How many object shards the store binds.
    pub bound: u32,
    pub r2: BTreeMap<String, Vec<u8>>,
    pub ops: Vec<Op>,
    /// Fails every upload once this many have been made.
    pub fail_after: Option<usize>,
    /// Fails the first upload this holds of, and every one after it.
    pub fail_at: Option<fn(&Op) -> bool>,
    /// Gives back R2 objects with their last byte changed.
    pub corrupt_reads: bool,
    pub observer: Option<Observer>,
    pub uploads: usize,
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self {
            db: Connection::open_in_memory().unwrap(),
            shards: BTreeMap::new(),
            bound: 8,
            r2: BTreeMap::new(),
            ops: Vec::new(),
            fail_after: None,
            fail_at: None,
            corrupt_reads: false,
            observer: None,
            uploads: 0,
        }
    }
}

impl MemoryStore {
    fn record(&mut self, op: Op) -> Result<(), String> {
        if op.uploads() {
            if self.fail_after.is_some_and(|k| self.uploads >= k) {
                return Err(format!("injected failure at {op:?}"));
            }
            if self.fail_at.is_some_and(|at| at(&op)) {
                self.fail_after = Some(self.uploads);
                return Err(format!("injected failure at {op:?}"));
            }
            self.uploads += 1;
        }
        self.ops.push(op);
        Ok(())
    }

    /// Shows the observer the last operation, run in `database`, and D1 and R2 after it.
    fn observe(&mut self, database: Database) {
        let db = match database {
            Database::Catalogue => Some(&self.db),
            Database::Shard(i) => self.shards.get(&i),
        };
        if let (Some(observer), Some(op), Some(db)) = (self.observer.as_mut(), self.ops.last(), db)
        {
            observer(op, &self.db, db, &self.r2);
        }
    }

    /// The uploads made, each by its D1 file name or R2 key.
    pub fn uploaded(&self) -> Vec<String> {
        self.ops
            .iter()
            .filter_map(|op| match op {
                Op::Execute(p) | Op::Put(p) if op.uploads() => Some(p.clone()),
                _ => None,
            })
            .collect()
    }

    /// Clears the operations and the injected failure, keeping D1 and R2.
    pub fn reset(&mut self) {
        self.ops.clear();
        self.fail_after = None;
        self.fail_at = None;
        self.corrupt_reads = false;
        self.observer = None;
        self.uploads = 0;
    }

    /// The single integer a query of the catalogue gives.
    pub fn int(&self, sql: &str) -> i64 {
        int(&self.db, sql)
    }

    /// The sum of the single integer a query gives in each database that holds the schema, the
    /// catalogue and every object shard.
    pub fn int_everywhere(&self, sql: &str) -> i64 {
        let schema = "SELECT COUNT(*) FROM sqlite_master WHERE name = 'schema_version'";
        std::iter::once(&self.db)
            .chain(self.shards.values())
            .filter(|db| int(db, schema) > 0)
            .map(|db| int(db, sql))
            .sum()
    }

    /// The database `database`, made when first used.
    pub fn database(&mut self, database: Database) -> &Connection {
        match database {
            Database::Catalogue => &self.db,
            Database::Shard(i) => self
                .shards
                .entry(i)
                .or_insert_with(|| Connection::open_in_memory().unwrap()),
        }
    }
}

/// What `wrangler d1 execute --file` does to a file's SQL before D1 runs it (its
/// `trimSqlQuery`, wrangler 4.156.2): the first `BEGIN TRANSACTION;` and the first `COMMIT;`
/// removed wherever they stand, inside quotes too, and a file still holding `BEGIN TRANSACTION`
/// refused.
pub fn trim_sql_query(sql: &str) -> Result<String, String> {
    if !sql.contains("BEGIN TRANSACTION") {
        return Ok(sql.to_owned());
    }
    let trimmed = sql
        .replacen("BEGIN TRANSACTION;", "", 1)
        .replacen("COMMIT;", "", 1);
    if trimmed.contains("BEGIN TRANSACTION") {
        return Err(
            "Wrangler could not process the provided SQL file, as it contains several \
                    transactions."
                .to_owned(),
        );
    }
    Ok(trimmed)
}

/// The single integer a query of `db` gives.
pub fn int(db: &Connection, sql: &str) -> i64 {
    db.query_row(sql, [], |r| r.get(0)).unwrap()
}

/// The rows of `sql` against `db`, each column by name.
fn rows(db: &Connection, sql: &str) -> rusqlite::Result<Vec<QueryRow>> {
    let mut statement = db.prepare(sql)?;
    if statement.column_count() == 0 {
        statement.execute([])?;
        return Ok(Vec::new());
    }
    let names: Vec<String> = statement
        .column_names()
        .iter()
        .map(|n| (*n).to_owned())
        .collect();
    let mut out = Vec::new();
    let mut query = statement.query([])?;
    while let Some(row) = query.next()? {
        let mut map = Map::new();
        for (i, name) in names.iter().enumerate() {
            let value = match row.get_ref(i)? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(n) => Value::from(n),
                ValueRef::Real(x) => Value::from(x),
                ValueRef::Text(t) | ValueRef::Blob(t) => {
                    Value::from(String::from_utf8_lossy(t).into_owned())
                }
            };
            map.insert(name.clone(), value);
        }
        out.push(map);
    }
    Ok(out)
}

impl Store for MemoryStore {
    fn execute_file(&mut self, database: Database, file: &Path) -> Result<(), String> {
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        self.record(Op::Execute(format!("{}/{name}", database.dir())))?;
        let sql = std::fs::read_to_string(file).map_err(|e| e.to_string())?;
        let done = trim_sql_query(&sql).and_then(|sql| {
            self.database(database)
                .execute_batch(&sql)
                .map_err(|e| e.to_string())
        });
        self.observe(database);
        done
    }

    fn query(&mut self, database: Database, sql: &str) -> Result<Vec<QueryRow>, String> {
        self.record(Op::Query(sql.to_owned()))?;
        let done = rows(self.database(database), sql).map_err(|e| e.to_string());
        self.observe(database);
        done
    }

    fn bound_shards(&mut self) -> Result<u32, String> {
        Ok(self.bound)
    }

    fn put(&mut self, key: &str, file: &Path) -> Result<(), String> {
        self.record(Op::Put(key.to_owned()))?;
        let bytes = std::fs::read(file).map_err(|e| e.to_string())?;
        self.r2.insert(key.to_owned(), bytes);
        self.observe(Database::Catalogue);
        Ok(())
    }

    fn get(&mut self, key: &str) -> Result<Option<Vec<u8>>, String> {
        self.record(Op::Get(key.to_owned()))?;
        let mut bytes = self.r2.get(key).cloned();
        if let Some(last) = bytes
            .as_mut()
            .and_then(|b| b.last_mut())
            .filter(|_| self.corrupt_reads)
        {
            *last ^= 1;
        }
        Ok(bytes)
    }

    fn delete(&mut self, key: &str) -> Result<(), String> {
        self.record(Op::Delete(key.to_owned()))?;
        self.r2.remove(key);
        self.observe(Database::Catalogue);
        Ok(())
    }
}

/// A clock that gives a later second each time it is read.
pub struct Clock(Cell<u32>);

impl Clock {
    pub fn new() -> Self {
        Self(Cell::new(0))
    }

    pub fn now(&self) -> String {
        let n = self.0.get();
        self.0.set(n + 1);
        format!("2026-11-03T13:{:02}:{:02}Z", n / 60, n % 60)
    }
}

/// Publishes release `number` of the build output `out`, built under the state directory
/// `state`, to `store`, its journal in `journals`, reading the time from `clock`.
pub fn publishing(
    store: &mut MemoryStore,
    number: u32,
    out: &Path,
    state: &Path,
    journals: &Path,
    clock: &Clock,
) -> Result<Published, PublishError> {
    let now = || clock.now();
    let options = StoreOptions {
        out,
        state,
        journals,
        now: &now,
    };
    publish(store, number, &options, &mut |_| {})
}

/// Prunes release `number` from `store`, as [`publishing`] publishes it.
pub fn pruning(
    store: &mut MemoryStore,
    number: u32,
    out: &Path,
    state: &Path,
    journals: &Path,
    clock: &Clock,
) -> Result<Pruned, PruneError> {
    let now = || clock.now();
    let options = StoreOptions {
        out,
        state,
        journals,
        now: &now,
    };
    prune(store, number, &options, &mut |_| {})
}

/// The rows of a [`MemoryStore`]'s databases, as a reindex reads them.
pub struct Rows<'a>(pub &'a MemoryStore);

impl Rows<'_> {
    fn db(&self, database: Database) -> Result<&Connection, String> {
        match database {
            Database::Catalogue => Ok(&self.0.db),
            Database::Shard(i) => self.0.shards.get(&i).ok_or(format!("no shard {i}")),
        }
    }
}

impl univ_ingest::release::reindex::RowSource for Rows<'_> {
    fn databases(&mut self) -> Result<Vec<Database>, String> {
        Ok(std::iter::once(Database::Catalogue)
            .chain(self.0.shards.keys().map(|i| Database::Shard(*i)))
            .collect())
    }

    fn holds(&mut self, database: Database, name: &str) -> Result<bool, String> {
        let sql =
            format!("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = '{name}'");
        Ok(int(self.db(database)?, &sql) > 0)
    }

    fn rows(
        &mut self,
        database: Database,
        table: &univ_model::release::Table,
        filter: &str,
        each: &mut dyn FnMut(&[univ_model::release::SqlValue]) -> Result<(), String>,
    ) -> Result<(), String> {
        use rusqlite::types::Value;
        use univ_model::release::SqlValue;
        let db = self.db(database)?;
        let sql = format!(
            "SELECT {} FROM {} WHERE {filter}",
            table.columns.join(", "),
            table.name
        );
        let mut q = db.prepare(&sql).map_err(|e| e.to_string())?;
        let mut rows = q.query([]).map_err(|e| e.to_string())?;
        let mut values = Vec::with_capacity(table.columns.len());
        while let Some(row) = rows.next().map_err(|e| e.to_string())? {
            values.clear();
            for i in 0..table.columns.len() {
                values.push(match row.get::<_, Value>(i).map_err(|e| e.to_string())? {
                    Value::Null | Value::Blob(_) => SqlValue::Null,
                    Value::Integer(n) => SqlValue::Integer(n),
                    Value::Real(x) => SqlValue::Real(x),
                    Value::Text(t) => SqlValue::Text(t),
                });
            }
            each(&values)?;
        }
        Ok(())
    }
}
