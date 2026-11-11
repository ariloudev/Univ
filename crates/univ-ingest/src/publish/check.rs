//! What local D1 records of a release and of `latest`, and the rows of a release a table holds.

use std::collections::{BTreeMap, BTreeSet};
use univ_model::release::fingerprint::{ROWS, alias, non_null_alias};
use univ_model::release::manifest::TableEstimate;

use univ_model::release::shards::Database;
use univ_model::release::{ColumnKind, SqlValue, Table};

use super::{Latest, QueryRow, Store};

/// What local D1 records of one release, and where `latest` points.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    /// Whether the catalogue holds the index of the object shards (schema version 5).
    pub shard_index: bool,
    /// Whether the catalogue maps each release's paths to its files (schema version 6).
    pub file_map: bool,
    /// The release `latest` points to, and when it was moved there.
    pub latest: Option<(u32, String)>,
    /// Whether D1 lists the release.
    pub listed: bool,
    pub published_at: Option<String>,
    pub pruned_at: Option<String>,
}

impl Status {
    /// Where `latest` points.
    #[must_use]
    pub fn latest(&self) -> Latest {
        Latest(self.latest.as_ref().map(|l| l.0))
    }
}

/// A text as an SQL literal.
#[must_use]
pub fn sql_text(text: &str) -> String {
    SqlValue::from(text).literal()
}

fn text(row: &QueryRow, column: &str) -> Option<String> {
    row.get(column)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

fn integer(row: &QueryRow, column: &str) -> Option<i64> {
    row.get(column).and_then(serde_json::Value::as_i64)
}

/// What the local catalogue database records of release `number`, and where `latest` points.
///
/// # Errors
///
/// Fails when D1 cannot be queried or `latest` names no release number.
pub fn published_status(store: &mut dyn Store, number: u32) -> Result<Status, String> {
    let tables = tables_in(store, Database::Catalogue)?;
    if !(tables.contains("releases") && tables.contains("latest")) {
        return Ok(Status::default());
    }
    let latest = match store
        .query(
            Database::Catalogue,
            "SELECT release, moved_at FROM latest WHERE one = 1",
        )?
        .first()
    {
        Some(row) => {
            let release = integer(row, "release")
                .and_then(|n| u32::try_from(n).ok())
                .ok_or("latest names no release number")?;
            Some((release, text(row, "moved_at").unwrap_or_default()))
        }
        None => None,
    };
    let rows = store.query(
        Database::Catalogue,
        &format!("SELECT published_at, pruned_at FROM releases WHERE number = {number}"),
    )?;
    let row = rows.first();
    Ok(Status {
        shard_index: tables.contains("release_shards"),
        file_map: tables.contains("release_files"),
        latest,
        listed: row.is_some(),
        published_at: row.and_then(|r| text(r, "published_at")),
        pruned_at: row.and_then(|r| text(r, "pruned_at")),
    })
}

/// Checks the rows of release `number` that `table` holds in `database` against `want`, the manifest's
/// count and fingerprint of them, with one query (`univ_model::release::fingerprint`): its own
/// rows for a table of one release, its row in the list of releases, and the id ledger as it
/// stood at the release, which each release carries whole. Gives the row count.
///
/// # Errors
///
/// Fails when D1 cannot be queried, or naming the table and the measure that differs.
pub fn check_rows(
    store: &mut dyn Store,
    database: Database,
    table: &Table,
    number: u32,
    want: &TableEstimate,
) -> Result<u64, String> {
    let rows = store.query(
        database,
        &table.fingerprint_query(&table.scope.filter(number)),
    )?;
    let row = rows
        .first()
        .ok_or_else(|| format!("{database} gave no fingerprint of {}", table.name))?;
    let count = integer(row, ROWS)
        .and_then(|n| u64::try_from(n).ok())
        .ok_or_else(|| format!("D1 gave no count of {}", table.name))?;
    if count != want.rows {
        return Err(format!(
            "{database} holds {count} rows of release {number} in {}, the manifest lists {}",
            table.name, want.rows
        ));
    }
    for column in &want.fingerprint {
        let measure = alias(&column.column);
        let got = row
            .get(&measure)
            .and_then(serde_json::Value::as_f64)
            .ok_or_else(|| format!("D1 gave no {measure} of {}", table.name))?;
        if column.differs(got) {
            return Err(format!(
                "{database} holds release {number}'s rows of {} with {} {got}, the manifest's is \
                 {}: a row was dropped or altered on the way",
                table.name,
                measure_name(table, &column.column),
                column.total
            ));
        }
        if let Some(want) = column.non_null {
            let measure = non_null_alias(&column.column);
            let got = integer(row, &measure)
                .ok_or_else(|| format!("D1 gave no {measure} of {}", table.name))?;
            if u64::try_from(got).ok() != Some(want) {
                return Err(format!(
                    "{database} holds release {number}'s rows of {} with {got} values of {} that \
                     are not NULL, the manifest's is {want}: a row was dropped or altered on the \
                     way",
                    table.name, column.column
                ));
            }
        }
    }
    if let Some(want) = &want.content_sha256 {
        check_content(store, database, table, number, want)?;
    }
    Ok(count)
}

/// Checks the rows of release `number` that `table` holds by their content: their digest in
/// the order of the primary key against `want`, the manifest's.
fn check_content(
    store: &mut dyn Store,
    database: Database,
    table: &Table,
    number: u32,
    want: &str,
) -> Result<(), String> {
    let rows = store.query(database, &table.content_query(&table.scope.filter(number)))?;
    let values: Vec<Vec<SqlValue>> = rows
        .iter()
        .map(|row| {
            table
                .columns
                .iter()
                .map(|c| match row.get(*c) {
                    Some(serde_json::Value::String(t)) => SqlValue::Text(t.clone()),
                    Some(serde_json::Value::Number(n)) => n.as_i64().map_or_else(
                        || SqlValue::Real(n.as_f64().unwrap_or(f64::NAN)),
                        SqlValue::Integer,
                    ),
                    _ => SqlValue::Null,
                })
                .collect()
        })
        .collect();
    let got = crate::release::content_sha256(&values);
    if got != want {
        return Err(format!(
            "{database} holds release {number}'s rows of {} whose content differs from the manifest's \
             (SHA-256 {got} against {want}): a row was dropped or altered on the way",
            table.name
        ));
    }
    Ok(())
}

/// What the fingerprint measures of `column`, as a message names it.
fn measure_name(table: &Table, column: &str) -> String {
    match table.position(column).map(|i| table.kinds[i]) {
        Some(ColumnKind::Text | ColumnKind::Json) => format!("total bytes of {column}"),
        _ => format!("total of {column}"),
    }
}

/// The tables the D1 database `database` holds.
///
/// # Errors
///
/// Fails when D1 cannot be queried.
pub fn tables_in(store: &mut dyn Store, database: Database) -> Result<BTreeSet<String>, String> {
    let rows = store.query(
        database,
        "SELECT name FROM sqlite_master WHERE type = 'table'",
    )?;
    Ok(rows.iter().filter_map(|r| text(r, "name")).collect())
}

/// The tables each D1 database holds, by database.
pub type Tables = BTreeMap<Database, BTreeSet<String>>;

/// The tables each of `databases` holds.
///
/// # Errors
///
/// Fails when D1 cannot be queried.
pub fn tables_of(
    store: &mut dyn Store,
    databases: impl Iterator<Item = Database>,
) -> Result<Tables, String> {
    databases
        .map(|d| tables_in(store, d).map(|t| (d, t)))
        .collect()
}

/// The rows of each of `tables` in `database` that its filter selects, in one query: D1 limits
/// the terms of a compound `SELECT`, so scalar subqueries, not a `UNION`.
///
/// # Errors
///
/// Fails when D1 cannot be queried or gives no count.
pub fn counts_of(
    store: &mut dyn Store,
    database: Database,
    tables: &[(&str, String)],
) -> Result<Vec<u64>, String> {
    if tables.is_empty() {
        return Ok(Vec::new());
    }
    let counts: Vec<String> = tables
        .iter()
        .enumerate()
        .map(|(i, (t, filter))| format!("(SELECT COUNT(*) FROM {t} WHERE {filter}) AS n{i}"))
        .collect();
    let rows = store.query(database, &format!("SELECT {}", counts.join(", ")))?;
    let row = rows
        .first()
        .ok_or_else(|| format!("{database} gave no count of its rows"))?;
    (0..tables.len())
        .map(|i| {
            integer(row, &format!("n{i}"))
                .and_then(|n| u64::try_from(n).ok())
                .ok_or_else(|| format!("{database} gave no count of {}", tables[i].0))
        })
        .collect()
}

/// The SHA-256s in the `sha256` column of the rows `sql` gives of the catalogue.
///
/// # Errors
///
/// Fails when D1 cannot be queried or a row gives no SHA-256.
pub fn sha256s(store: &mut dyn Store, sql: &str) -> Result<BTreeSet<String>, String> {
    store
        .query(Database::Catalogue, sql)?
        .iter()
        .map(|row| {
            text(row, "sha256").ok_or_else(|| "release_files gives a row with no sha256".to_owned())
        })
        .collect()
}

/// The releases the catalogue lists and has not pruned, other than `number`, in order.
///
/// # Errors
///
/// Fails when D1 cannot be queried or a row gives no number.
pub fn kept_releases(store: &mut dyn Store, number: u32) -> Result<Vec<u32>, String> {
    let sql = format!(
        "SELECT number FROM releases WHERE pruned_at IS NULL AND number != {number} ORDER BY \
         number"
    );
    store
        .query(Database::Catalogue, &sql)?
        .iter()
        .map(|row| {
            integer(row, "number")
                .and_then(|n| u32::try_from(n).ok())
                .ok_or_else(|| "releases gives a row with no number".to_owned())
        })
        .collect()
}

/// The schema version the D1 database `database` records, if it records one.
///
/// # Errors
///
/// Fails when D1 cannot be queried.
pub fn schema_version(store: &mut dyn Store, database: Database) -> Result<Option<i64>, String> {
    let rows = store.query(database, "SELECT MAX(version) AS v FROM schema_version")?;
    Ok(rows.first().and_then(|r| integer(r, "v")))
}

/// Runs `sql` in `database`, through `file`, which is removed after whether or not it ran.
///
/// # Errors
///
/// Fails when the file cannot be written or D1 refuses it.
pub fn execute_sql(
    store: &mut dyn Store,
    database: Database,
    file: &std::path::Path,
    sql: &str,
) -> Result<(), String> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(file, sql).map_err(|e| format!("{}: {e}", file.display()))?;
    let executed = store.execute_file(database, file);
    let _ = std::fs::remove_file(file);
    executed
}

/// The manifest of release `number` as R2 holds it, if it does.
///
/// # Errors
///
/// Fails when R2 cannot be read or the manifest does not parse.
pub fn r2_manifest(
    store: &mut dyn Store,
    number: u32,
) -> Result<Option<univ_model::release::ReleaseManifest>, String> {
    let key = univ_model::release::keys::manifest(number);
    let Some(bytes) = store.get(&key)? else {
        return Ok(None);
    };
    crate::release::parse_manifest(
        std::path::Path::new(&key),
        &String::from_utf8_lossy(&bytes),
        number,
    )
    .map(Some)
    .map_err(|e| e.to_string())
}
