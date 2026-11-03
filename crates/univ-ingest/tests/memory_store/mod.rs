//! An in-memory [`Store`]: D1 as an in-memory `SQLite` database, which is what D1 runs, and R2
//! as a map of keys to bytes. It records every operation, can fail the upload after a given
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

/// Looks at D1 and R2 after an operation.
pub type Observer = Box<dyn FnMut(&Op, &Connection, &BTreeMap<String, Vec<u8>>)>;

pub struct MemoryStore {
    pub db: Connection,
    pub r2: BTreeMap<String, Vec<u8>>,
    pub ops: Vec<Op>,
    /// Fails every upload once this many have been made.
    pub fail_after: Option<usize>,
    /// Gives back R2 objects with their last byte changed.
    pub corrupt_reads: bool,
    pub observer: Option<Observer>,
    pub uploads: usize,
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self {
            db: Connection::open_in_memory().unwrap(),
            r2: BTreeMap::new(),
            ops: Vec::new(),
            fail_after: None,
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
            self.uploads += 1;
        }
        self.ops.push(op);
        Ok(())
    }

    fn observe(&mut self) {
        if let (Some(observer), Some(op)) = (self.observer.as_mut(), self.ops.last()) {
            observer(op, &self.db, &self.r2);
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
        self.corrupt_reads = false;
        self.observer = None;
        self.uploads = 0;
    }

    /// The single integer a query gives.
    pub fn int(&self, sql: &str) -> i64 {
        int(&self.db, sql)
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
    fn execute_file(&mut self, file: &Path) -> Result<(), String> {
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        self.record(Op::Execute(name))?;
        let sql = std::fs::read_to_string(file).map_err(|e| e.to_string())?;
        let done = trim_sql_query(&sql)
            .and_then(|sql| self.db.execute_batch(&sql).map_err(|e| e.to_string()));
        self.observe();
        done
    }

    fn query(&mut self, sql: &str) -> Result<Vec<QueryRow>, String> {
        self.record(Op::Query(sql.to_owned()))?;
        let done = rows(&self.db, sql).map_err(|e| e.to_string());
        self.observe();
        done
    }

    fn put(&mut self, key: &str, file: &Path) -> Result<(), String> {
        self.record(Op::Put(key.to_owned()))?;
        let bytes = std::fs::read(file).map_err(|e| e.to_string())?;
        self.r2.insert(key.to_owned(), bytes);
        self.observe();
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
        self.observe();
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
