//! The numbering and row index of the release `latest` points to, read back from the databases
//! it was published to (build-a-release): what a release needs of the one before to write only
//! its change, made again when the state lost it or when it was published before the state kept
//! it. Every row valid in the release is read once, so its index names exactly what D1 holds.
//!
//! The numbering is read from the same rows: each term's, source's and chain's number from
//! `terms`, `sources` and `chains`, each object shard's level-0 cells from its `object_cells`
//! by its scale domain's term, and the shard of each bucket of objects not placed from the
//! objects each shard holds.

use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::Connection;
use rusqlite::types::Value;
use univ_model::release::rows::{
    ChainRow, ObjectCellRow, ObjectRow, ReleaseFileRow, SourceRow, TermRow,
};
use univ_model::release::shards::{Database, morton};
use univ_model::release::{Cell, Row, Scope, SqlValue, TABLES, Table, valid_in};

use super::d1::{Indexer, ROWS, fresh_index};
use super::kept::Kept;
use super::packing::bucket;

/// Where the rows of a release are read from: the databases it was published to, each an
/// `SQLite` database.
pub trait RowSource {
    /// The databases there are, the catalogue's first.
    fn databases(&self) -> Vec<Database>;

    /// The database `database`.
    ///
    /// # Errors
    ///
    /// Fails when there is none.
    fn db(&self, database: Database) -> Result<&Connection, String>;
}

/// Whether `db` holds a table `name`.
///
/// # Errors
///
/// Fails when the database cannot be read.
pub fn holds(db: &Connection, name: &str) -> Result<bool, String> {
    db.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |r| r.get::<_, i64>(0),
    )
    .map(|n| n > 0)
    .map_err(|e| e.to_string())
}

/// Gives `each` every row of `table` in `db` that `filter` selects, its values in the order of
/// the table's columns.
///
/// # Errors
///
/// Fails when the rows cannot be read, or as `each` fails.
pub fn read_rows(
    db: &Connection,
    table: &Table,
    filter: &str,
    each: &mut dyn FnMut(&[SqlValue]) -> Result<(), String>,
) -> Result<(), String> {
    let sql = format!(
        "SELECT {} FROM {} WHERE {filter}",
        table.columns.join(", "),
        table.name
    );
    let mut q = db.prepare(&sql).map_err(|e| format!("{sql}: {e}"))?;
    let mut rows = q.query([]).map_err(|e| format!("{sql}: {e}"))?;
    let mut values = Vec::with_capacity(table.columns.len());
    while let Some(row) = rows.next().map_err(|e| format!("{sql}: {e}"))? {
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

/// What a reindex wrote.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Reindexed {
    /// The rows indexed, by table.
    pub rows: BTreeMap<String, u64>,
    pub kept: Kept,
}

/// The value of `column` in `row` of `table`.
fn at<'r>(table: &Table, row: &'r [SqlValue], column: &str) -> Result<&'r SqlValue, String> {
    table
        .position(column)
        .and_then(|i| row.get(i))
        .ok_or_else(|| format!("a row of {} has no {column}", table.name))
}

fn integer(table: &Table, row: &[SqlValue], column: &str) -> Result<i64, String> {
    match at(table, row, column)? {
        SqlValue::Integer(n) => Ok(*n),
        other => Err(format!(
            "{}.{column} is {other:?}, not an integer",
            table.name
        )),
    }
}

fn text(table: &Table, row: &[SqlValue], column: &str) -> Result<String, String> {
    match at(table, row, column)? {
        SqlValue::Text(t) => Ok(t.clone()),
        other => Err(format!("{}.{column} is {other:?}, not a text", table.name)),
    }
}

fn number(n: i64) -> Result<u32, String> {
    u32::try_from(n).map_err(|e| format!("{n}: {e}"))
}

/// The numbering and placement of a release, gathered from its rows as they are indexed.
#[derive(Default)]
struct Numbering {
    terms: BTreeMap<String, u32>,
    sources: BTreeMap<String, u32>,
    chains: BTreeMap<String, u32>,
    /// The Morton codes of each shard's level-0 cells, by its scale domain's term id.
    cells: BTreeMap<u32, BTreeMap<u32, Vec<u64>>>,
    /// The buckets of each shard's objects not placed.
    loose: BTreeMap<u32, Vec<u64>>,
}

impl Numbering {
    /// Takes in what `row` of `table` in `database` tells.
    fn see(&mut self, database: Database, table: &Table, row: &[SqlValue]) -> Result<(), String> {
        let name = table.name;
        match database {
            Database::Catalogue if name == TermRow::TABLE.name => {
                let id = number(integer(table, row, "id")?)?;
                self.terms.insert(text(table, row, "term")?, id);
            }
            Database::Catalogue if name == SourceRow::TABLE.name => {
                let n = number(integer(table, row, "n")?)?;
                self.sources.insert(text(table, row, "id")?, n);
            }
            Database::Catalogue if name == ChainRow::TABLE.name => {
                let id = number(integer(table, row, "id")?)?;
                self.chains.insert(text(table, row, "steps")?, id);
            }
            Database::Shard(shard) if name == ObjectCellRow::TABLE.name => {
                let cell = Cell {
                    ix: integer(table, row, "ix")?,
                    iy: integer(table, row, "iy")?,
                    iz: integer(table, row, "iz")?,
                };
                let code = morton(cell).ok_or_else(|| format!("{cell:?} is beyond the reach"))?;
                let domain = number(integer(table, row, "domain")?)?;
                let held = self.cells.entry(shard).or_default();
                held.entry(domain).or_default().push(code);
            }
            Database::Shard(shard)
                if name == ObjectRow::TABLE.name && integer(table, row, "placed")? == 0 =>
            {
                let id = u64::try_from(integer(table, row, "id")?).map_err(|e| e.to_string())?;
                let at = u64::try_from(bucket(id)).map_err(|e| e.to_string())?;
                self.loose.entry(shard).or_default().push(at);
            }
            _ => {}
        }
        Ok(())
    }

    /// The numbering as the state keeps it: each shard's cells by its domain's term, and the
    /// shards holding nothing of the release left out.
    fn kept(self) -> Result<Kept, String> {
        let term_of: BTreeMap<u32, &String> = self.terms.iter().map(|(t, n)| (*n, t)).collect();
        let mut cells: Vec<BTreeMap<String, Vec<u64>>> = Vec::new();
        let shards = self.cells.keys().chain(self.loose.keys());
        if let Some(last) = shards.max() {
            cells.resize(*last as usize + 1, BTreeMap::new());
        }
        for (shard, held) in self.cells {
            for (domain, mut codes) in held {
                let name = term_of
                    .get(&domain)
                    .ok_or_else(|| format!("object_cells names domain {domain}, no term"))?;
                codes.sort_unstable();
                codes.dedup();
                cells[shard as usize].insert((*name).clone(), codes);
            }
        }
        let loose = self
            .loose
            .into_iter()
            .flat_map(|(shard, buckets)| buckets.into_iter().map(move |b| (b, shard)))
            .collect();
        Ok(Kept {
            terms: self.terms,
            sources: self.sources,
            chains: self.chains,
            cells,
            loose,
            ..Kept::default()
        })
    }
}

/// Writes, into the state directory `dir`, the row index and numbering of release `latest`,
/// read from `source`: the row index under [`ROWS`], replacing any there, and the numbering as
/// [`super::NUMBERING`]. The index is written beside, and the one there removed only once it is whole:
/// a reindex that fails leaves the state with its index, or with none, which a build refuses,
/// never with part of one. Databases older than schema 6 are refused: they hold no
/// `release_files`, whose rows a migration writes, so an index of them would have the next
/// release write its every path again and retire none.
///
/// # Errors
///
/// Fails for databases of an older schema, or when a row cannot be read or a file written.
pub fn reindex(source: &dyn RowSource, latest: u32, dir: &Path) -> Result<Reindexed, String> {
    if !holds(source.db(Database::Catalogue)?, ReleaseFileRow::TABLE.name)? {
        return Err(
            "the databases are older than schema 6, rows not yet shared between releases: run \
             `univ-ingest migrate` first"
                .to_owned(),
        );
    }
    let filter = valid_in("", latest);
    let part = dir.join(format!("{ROWS}.part"));
    let io = |path: &Path, e: std::io::Error| format!("{}: {e}", path.display());
    fresh_index(&part).map_err(|e| io(&part, e))?;
    let mut indexed: BTreeMap<String, u64> = BTreeMap::new();
    let mut numbering = Numbering::default();
    for database in source.databases() {
        let db = source.db(database)?;
        for t in TABLES.iter().filter(|t| t.scope == Scope::PerRelease) {
            if !holds(db, t.name)? {
                continue;
            }
            let mut index = Indexer::new(&part, database, *t)?;
            let count = indexed.entry(t.name.to_owned()).or_default();
            read_rows(db, t, &filter, &mut |row| {
                *count += 1;
                numbering.see(database, t, row)?;
                index.record(row)
            })?;
            index.finish()?;
        }
    }
    let kept = numbering.kept()?;
    let rows = dir.join(ROWS);
    if rows.exists() {
        std::fs::remove_dir_all(&rows).map_err(|e| io(&rows, e))?;
    }
    kept.write(dir).map_err(|e| io(dir, e))?;
    std::fs::rename(&part, &rows).map_err(|e| io(&rows, e))?;
    Ok(Reindexed {
        rows: indexed,
        kept,
    })
}
