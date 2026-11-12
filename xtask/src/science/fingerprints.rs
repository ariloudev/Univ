//! The manifest's per-table fingerprints recomputed from the loaded rows, as publishing
//! recomputes them from D1: univ-model's fingerprint query, and for the lookup tables the
//! digest of their rows in univ-model's canonical form.
//!
//! The loaders take each row a release shares with the releases before it as the release's own,
//! its `release` its number, so the scratch database no longer holds the rows the manifest
//! fingerprints. Each loader therefore measures the rows as it reads them, before that, and
//! [`Recorder::record`] keeps the measures in the scratch database for the check.

use std::collections::HashMap;

use rusqlite::types::ValueRef;
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use univ_model::release::fingerprint::{ROWS, alias, canonical_row, non_null_alias};
use univ_model::release::manifest::TableEstimate;
use univ_model::release::{ReleaseManifest, SqlValue, TABLES, Table};

/// The scratch tables the loaders record the fingerprints in: each column's total and count of
/// values not NULL (the row count as the column `''`), and each lookup table's digest.
const RECORDED: &str = "CREATE TABLE IF NOT EXISTS science_fingerprints (tbl TEXT NOT NULL, \
     col TEXT NOT NULL, total REAL NOT NULL, non_null INTEGER NOT NULL, PRIMARY KEY (tbl, col)); \
     CREATE TABLE IF NOT EXISTS science_contents (tbl TEXT PRIMARY KEY, sha256 TEXT NOT NULL);";

/// The fingerprint of a release's rows of every table, added up over the databases that hold
/// them.
pub struct Recorder {
    release: u32,
    /// Each table's measures by column, the row count by `''`: the total and the values not NULL.
    totals: HashMap<&'static str, HashMap<String, (f64, i64)>>,
    /// Each lookup table's rows, to be put in key order and digested once all are read.
    contents: HashMap<&'static str, Vec<Vec<SqlValue>>>,
}

impl Recorder {
    #[must_use]
    pub fn new(release: u32) -> Self {
        Self {
            release,
            totals: HashMap::new(),
            contents: HashMap::new(),
        }
    }

    /// Adds the measures of the release's rows of each table `db` holds, as it holds them.
    ///
    /// # Errors
    ///
    /// Fails when a query cannot run.
    pub fn add(&mut self, db: &Connection) -> Result<(), String> {
        for table in &TABLES {
            let held: i64 = db
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_schema WHERE type = 'table' AND name = ?1",
                    [table.name],
                    |r| r.get(0),
                )
                .map_err(|e| format!("{}: {e}", table.name))?;
            if held == 0 {
                continue;
            }
            let filter = table.scope.filter(self.release);
            let totals = self.totals.entry(table.name).or_default();
            db.query_row(&table.fingerprint_query(&filter), [], |row| {
                let rows: i64 = row.get(ROWS)?;
                totals.entry(String::new()).or_default().1 += rows;
                for column in table.columns {
                    let total: f64 = row.get(alias(column).as_str())?;
                    let non_null: i64 = row.get(non_null_alias(column).as_str())?;
                    let at = totals.entry((*column).to_owned()).or_default();
                    at.0 += total;
                    at.1 += non_null;
                }
                Ok(())
            })
            .map_err(|e| format!("{}: {e}", table.name))?;
            if table.checked_by_content() {
                let rows = self.contents.entry(table.name).or_default();
                read_rows(db, table, &filter, rows)?;
            }
        }
        Ok(())
    }

    /// Keeps the measures in the scratch database `db`, in place of any recorded before.
    ///
    /// # Errors
    ///
    /// Fails when they cannot be written.
    pub fn record(mut self, db: &Connection) -> Result<(), String> {
        let fail = |e: rusqlite::Error| format!("cannot record the fingerprints: {e}");
        db.execute_batch(RECORDED).map_err(fail)?;
        db.execute_batch("DELETE FROM science_fingerprints; DELETE FROM science_contents;")
            .map_err(fail)?;
        let mut put = db
            .prepare("INSERT INTO science_fingerprints VALUES (?1, ?2, ?3, ?4)")
            .map_err(fail)?;
        for (table, totals) in &self.totals {
            for (column, (total, non_null)) in totals {
                put.execute(params![table, column, total, non_null])
                    .map_err(fail)?;
            }
        }
        let mut put = db
            .prepare("INSERT INTO science_contents VALUES (?1, ?2)")
            .map_err(fail)?;
        for table in TABLES.iter().filter(|t| t.checked_by_content()) {
            let mut rows = self.contents.remove(table.name).unwrap_or_default();
            table.sort_by_key(&mut rows);
            let mut hasher = Sha256::new();
            let mut bytes = Vec::new();
            for row in &rows {
                bytes.clear();
                canonical_row(row, &mut bytes);
                hasher.update(&bytes);
            }
            put.execute(params![table.name, hex(&hasher.finalize())])
                .map_err(fail)?;
        }
        Ok(())
    }
}

/// Every measure of every table that differs from the manifest's, described; empty when all
/// match. Gives also the number of tables compared.
///
/// # Errors
///
/// Fails when a query cannot run, or the scratch database holds no fingerprints recorded as it
/// was loaded.
pub fn differences(
    db: &Connection,
    manifest: &ReleaseManifest,
) -> Result<(usize, Vec<String>), String> {
    let mut out = Vec::new();
    for want in manifest.estimates().flat_map(|e| &e.tables) {
        if !TABLES.iter().any(|t| t.name == want.table) {
            out.push(format!(
                "the manifest fingerprints {}, no table of the schema",
                want.table
            ));
        }
    }
    let held: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM sqlite_schema WHERE name = 'science_fingerprints'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if held == 0 {
        return Err(
            "the scratch database holds no fingerprints of the rows as loaded: it was loaded \
             before they were recorded, so load it again"
                .to_owned(),
        );
    }
    let mut compared = 0;
    for table in &TABLES {
        // The catalogue's and every object shard's rows, all loaded into one database.
        let Some(want) = manifest.table_estimate(table.name) else {
            out.push(format!(
                "the manifest gives no fingerprint of {}",
                table.name
            ));
            continue;
        };
        compared += 1;
        compare(db, table, &want, &mut out)?;
    }
    Ok((compared, out))
}

fn compare(
    db: &Connection,
    table: &Table,
    want: &TableEstimate,
    out: &mut Vec<String>,
) -> Result<(), String> {
    let name = table.name;
    let recorded = |column: &str| -> Result<(f64, i64), String> {
        db.query_row(
            "SELECT total, non_null FROM science_fingerprints WHERE tbl = ?1 AND col = ?2",
            [name, column],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map(Option::unwrap_or_default)
        .map_err(|e| format!("{name}: {e}"))
    };
    let (_, rows) = recorded("")?;
    if u64::try_from(rows).ok() != Some(want.rows) {
        out.push(format!(
            "{name}: {rows} rows loaded, the manifest lists {}",
            want.rows
        ));
    }
    for column in &want.fingerprint {
        let (got, non_null) = recorded(&column.column)?;
        if column.differs(got) {
            out.push(format!(
                "{name}.{}: total {got}, the manifest's {}",
                column.column, column.total
            ));
        }
        if let Some(n) = column.non_null
            && u64::try_from(non_null).ok() != Some(n)
        {
            out.push(format!(
                "{name}.{}: {non_null} values not NULL, the manifest's {n}",
                column.column
            ));
        }
    }
    if let Some(digest) = &want.content_sha256 {
        let got: String = db
            .query_row(
                "SELECT sha256 FROM science_contents WHERE tbl = ?1",
                [name],
                |r| r.get(0),
            )
            .map_err(|e| format!("{name}: {e}"))?;
        if &got != digest {
            out.push(format!(
                "{name}: content SHA-256 {got}, the manifest's {digest}"
            ));
        }
    }
    Ok(())
}

/// Adds the rows `filter` selects of `table` in `db` to `out`, each as its values.
fn read_rows(
    db: &Connection,
    table: &Table,
    filter: &str,
    out: &mut Vec<Vec<SqlValue>>,
) -> Result<(), String> {
    let fail = |e: rusqlite::Error| format!("{}: {e}", table.name);
    let mut q = db.prepare(&table.content_query(filter)).map_err(fail)?;
    let mut rows = q.query([]).map_err(fail)?;
    while let Some(row) = rows.next().map_err(fail)? {
        let mut values = Vec::with_capacity(table.columns.len());
        for i in 0..table.columns.len() {
            values.push(sql_value(row.get_ref(i).map_err(fail)?)?);
        }
        out.push(values);
    }
    Ok(())
}

fn sql_value(value: ValueRef<'_>) -> Result<SqlValue, String> {
    Ok(match value {
        ValueRef::Null => SqlValue::Null,
        ValueRef::Integer(n) => SqlValue::Integer(n),
        ValueRef::Real(x) => SqlValue::Real(x),
        ValueRef::Text(t) => SqlValue::Text(
            String::from_utf8(t.to_vec()).map_err(|_| "a text that is not UTF-8".to_owned())?,
        ),
        ValueRef::Blob(_) => return Err("a blob, which no table of the schema holds".to_owned()),
    })
}

pub use univ_model::encoding::hex;
