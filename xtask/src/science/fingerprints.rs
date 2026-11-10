//! The manifest's per-table fingerprints recomputed from the loaded rows, as publishing
//! recomputes them from D1: univ-model's fingerprint query, and for the lookup tables the
//! digest of their rows in univ-model's canonical form.

use rusqlite::Connection;
use rusqlite::types::ValueRef;
use sha2::{Digest, Sha256};
use univ_model::release::fingerprint::{ROWS, alias, canonical_row, non_null_alias};
use univ_model::release::manifest::TableEstimate;
use univ_model::release::{ReleaseManifest, SqlValue, TABLES, Table};

/// Every measure of every table that differs from the manifest's, described; empty when all
/// match. Gives also the number of tables compared.
///
/// # Errors
///
/// Fails when a query cannot run.
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
        compare(db, table, manifest.release, &want, &mut out)?;
    }
    Ok((compared, out))
}

fn compare(
    db: &Connection,
    table: &Table,
    number: u32,
    want: &TableEstimate,
    out: &mut Vec<String>,
) -> Result<(), String> {
    let filter = table.scope.filter(number);
    let sql = table.fingerprint_query(&filter);
    let name = table.name;
    db.query_row(&sql, [], |row| {
        let rows: i64 = row.get(ROWS)?;
        if u64::try_from(rows).ok() != Some(want.rows) {
            out.push(format!(
                "{name}: {rows} rows loaded, the manifest lists {}",
                want.rows
            ));
        }
        for column in &want.fingerprint {
            let got: f64 = row.get(alias(&column.column).as_str())?;
            if column.differs(got) {
                out.push(format!(
                    "{name}.{}: total {got}, the manifest's {}",
                    column.column, column.total
                ));
            }
            if let Some(n) = column.non_null {
                let got: i64 = row.get(non_null_alias(&column.column).as_str())?;
                if u64::try_from(got).ok() != Some(n) {
                    out.push(format!(
                        "{name}.{}: {got} values not NULL, the manifest's {n}",
                        column.column
                    ));
                }
            }
        }
        Ok(())
    })
    .map_err(|e| format!("{name}: {e}"))?;
    if let Some(digest) = &want.content_sha256 {
        let got = content_sha256(db, table, &filter)?;
        if &got != digest {
            out.push(format!(
                "{name}: content SHA-256 {got}, the manifest's {digest}"
            ));
        }
    }
    Ok(())
}

/// The SHA-256, in hex, of the rows `filter` selects in the order of the primary key, each in
/// univ-model's canonical form.
fn content_sha256(db: &Connection, table: &Table, filter: &str) -> Result<String, String> {
    let fail = |e: rusqlite::Error| format!("{}: {e}", table.name);
    let mut q = db.prepare(&table.content_query(filter)).map_err(fail)?;
    let mut rows = q.query([]).map_err(fail)?;
    let mut hasher = Sha256::new();
    let mut bytes = Vec::new();
    let mut values = Vec::with_capacity(table.columns.len());
    while let Some(row) = rows.next().map_err(fail)? {
        values.clear();
        for i in 0..table.columns.len() {
            values.push(sql_value(row.get_ref(i).map_err(fail)?)?);
        }
        bytes.clear();
        canonical_row(&values, &mut bytes);
        hasher.update(&bytes);
    }
    Ok(hex(&hasher.finalize()))
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
