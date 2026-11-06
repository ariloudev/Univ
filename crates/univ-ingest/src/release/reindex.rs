//! The numbering and row index of the release `latest` points to, read back from the databases
//! it was published to (build-a-release): what a release needs of the one before to write only
//! its change, made again when the state lost it or when it was published before the state kept
//! it. Every row valid in the release is read, so its index names exactly what D1 holds.
//!
//! The numbering is read from the rows too: each term's, source's and chain's number from
//! `terms`, `sources` and `chains`, each object shard's level-0 cells from its `object_cells`
//! by its scale domain's term, and the shard of each bucket of objects not placed from the
//! objects each shard holds.

use std::collections::BTreeMap;
use std::path::Path;

use univ_model::release::shards::{Database, morton};
use univ_model::release::{Cell, SCHEMA_VERSION, Scope, SqlValue, TABLES, Table, valid_in};

use super::d1::{Indexer, ROWS};
use super::kept::Kept;
use super::packing::LOOSE_BUCKETS;

/// Where the rows of a release are read from: the databases it was published to.
pub trait RowSource {
    /// The databases there are, the catalogue's first.
    ///
    /// # Errors
    ///
    /// Fails when they cannot be found.
    fn databases(&mut self) -> Result<Vec<Database>, String>;

    /// Whether `database` holds a table `name`.
    ///
    /// # Errors
    ///
    /// Fails when the database cannot be read.
    fn holds(&mut self, database: Database, name: &str) -> Result<bool, String>;

    /// Gives `each` every row of `table` in `database` that `filter` selects, its values in
    /// the order of the table's columns.
    ///
    /// # Errors
    ///
    /// Fails when the rows cannot be read, or as `each` fails.
    fn rows(
        &mut self,
        database: Database,
        table: &Table,
        filter: &str,
        each: &mut dyn FnMut(&[SqlValue]) -> Result<(), String>,
    ) -> Result<(), String>;
}

/// What a reindex wrote.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Reindexed {
    /// The rows indexed, by table.
    pub rows: BTreeMap<String, u64>,
    pub kept: Kept,
}

/// The table named `name` of the schema.
fn table(name: &str) -> Result<Table, String> {
    TABLES
        .iter()
        .copied()
        .find(|t| t.name == name)
        .ok_or_else(|| format!("{name} is not a table of the schema"))
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

/// The numbers of a catalogue table of release `latest`: each `name` with its `id`.
fn numbers(
    source: &mut dyn RowSource,
    (name, id, named): (&str, &str, &str),
    filter: &str,
) -> Result<BTreeMap<String, u32>, String> {
    let t = table(name)?;
    let mut out = BTreeMap::new();
    source.rows(Database::Catalogue, &t, filter, &mut |row| {
        out.insert(text(&t, row, named)?, number(integer(&t, row, id)?)?);
        Ok(())
    })?;
    Ok(out)
}

/// The numbering and placement of release `latest`, from the rows valid in it.
fn kept(source: &mut dyn RowSource, databases: &[Database], filter: &str) -> Result<Kept, String> {
    let terms = numbers(source, ("terms", "id", "term"), filter)?;
    let term_of: BTreeMap<u32, String> = terms.iter().map(|(t, n)| (*n, t.clone())).collect();
    let mut kept = Kept {
        sources: numbers(source, ("sources", "n", "id"), filter)?,
        chains: numbers(source, ("chains", "id", "steps"), filter)?,
        terms,
        ..Kept::default()
    };
    let (cells, objects) = (table("object_cells")?, table("objects")?);
    for &database in databases {
        let Database::Shard(shard) = database else {
            continue;
        };
        // A shard bound but never loaded holds nothing of the release.
        if !source.holds(database, cells.name)? {
            continue;
        }
        let mut held: BTreeMap<String, Vec<u64>> = BTreeMap::new();
        source.rows(database, &cells, filter, &mut |row| {
            let domain = number(integer(&cells, row, "domain")?)?;
            let name = term_of
                .get(&domain)
                .ok_or_else(|| format!("object_cells names domain {domain}, no term"))?;
            let cell = Cell {
                ix: integer(&cells, row, "ix")?,
                iy: integer(&cells, row, "iy")?,
                iz: integer(&cells, row, "iz")?,
            };
            let code = morton(cell).ok_or_else(|| format!("{cell:?} is beyond the reach"))?;
            held.entry(name.clone()).or_default().push(code);
            Ok(())
        })?;
        for codes in held.values_mut() {
            codes.sort_unstable();
            codes.dedup();
        }
        let mut loose = Vec::new();
        source.rows(database, &objects, filter, &mut |row| {
            if integer(&objects, row, "placed")? == 0 {
                loose.push(integer(&objects, row, "id")?.unsigned_abs() % LOOSE_BUCKETS);
            }
            Ok(())
        })?;
        // A shard bound but holding nothing of the release is none of its shards.
        if held.is_empty() && loose.is_empty() {
            continue;
        }
        kept.loose
            .extend(loose.into_iter().map(|bucket| (bucket, shard)));
        let at = shard as usize;
        if kept.cells.len() <= at {
            kept.cells.resize(at + 1, BTreeMap::new());
        }
        kept.cells[at] = held;
    }
    Ok(kept)
}

/// Writes, into the state directory `dir`, the row index and numbering of release `latest`,
/// read from `source`, whose databases are of schema `version`: the row index under
/// [`ROWS`], replacing any there, and the numbering as `numbering.yaml`. Databases of an older
/// schema are refused: they hold no `release_files`, whose rows a migration writes, so an
/// index of them would have the next release write its every path again and retire none.
///
/// # Errors
///
/// Fails for databases of an older schema, or when a row cannot be read or a file written.
pub fn reindex(
    source: &mut dyn RowSource,
    (latest, version): (u32, i64),
    dir: &Path,
) -> Result<Reindexed, String> {
    if version < i64::from(SCHEMA_VERSION) {
        return Err(format!(
            "the databases are of schema {version}, older than {SCHEMA_VERSION}: run \
             `univ-ingest migrate` first"
        ));
    }
    let filter = valid_in("", latest);
    let databases = source.databases()?;
    let rows_dir = dir.join(ROWS);
    super::d1::fresh_index(&rows_dir).map_err(|e| format!("{}: {e}", rows_dir.display()))?;
    let mut indexed: BTreeMap<String, u64> = BTreeMap::new();
    for &database in &databases {
        for t in TABLES.iter().filter(|t| t.scope == Scope::PerRelease) {
            if !source.holds(database, t.name)? {
                continue;
            }
            let mut index = Indexer::new(&rows_dir, database, *t)?;
            let count = indexed.entry(t.name.to_owned()).or_default();
            source.rows(database, t, &filter, &mut |row| {
                *count += 1;
                index.record(row)
            })?;
            index.finish()?;
        }
    }
    let kept = kept(source, &databases, &filter)?;
    kept.write(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok(Reindexed {
        rows: indexed,
        kept,
    })
}
