//! What local D1 records of a release and of `latest`, and the rows of a release a table holds.

use univ_model::release::fingerprint::{ROWS, alias};
use univ_model::release::manifest::TableEstimate;
use univ_model::release::{ColumnKind, SqlValue, Table};

use super::{Latest, QueryRow, Store};

/// What local D1 records of one release, and where `latest` points.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    /// Whether D1 holds the schema's list of releases at all.
    pub schema: bool,
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

/// What local D1 records of release `number`, and where `latest` points.
///
/// # Errors
///
/// Fails when D1 cannot be queried or `latest` names no release number.
pub fn published_status(store: &mut dyn Store, number: u32) -> Result<Status, String> {
    let tables = store.query(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name IN ('releases', 'latest')",
    )?;
    if tables.len() < 2 {
        return Ok(Status::default());
    }
    let latest = match store
        .query("SELECT release, moved_at FROM latest WHERE one = 1")?
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
    let rows = store.query(&format!(
        "SELECT published_at, pruned_at FROM releases WHERE number = {number}"
    ))?;
    let row = rows.first();
    Ok(Status {
        schema: true,
        latest,
        listed: row.is_some(),
        published_at: row.and_then(|r| text(r, "published_at")),
        pruned_at: row.and_then(|r| text(r, "pruned_at")),
    })
}

/// Checks the rows of release `number` that `table` holds against `want`, the manifest's
/// count and fingerprint of them, with one query (`univ_model::release::fingerprint`): its own
/// rows for a table of one release, its row in the list of releases, and the id ledger as it
/// stood at the release, which each release carries whole. Gives the row count.
///
/// # Errors
///
/// Fails when D1 cannot be queried, or naming the table and the measure that differs.
pub fn check_rows(
    store: &mut dyn Store,
    table: &Table,
    number: u32,
    want: &TableEstimate,
) -> Result<u64, String> {
    let rows = store.query(&table.fingerprint_query(&table.scope.filter(number)))?;
    let row = rows
        .first()
        .ok_or_else(|| format!("D1 gave no fingerprint of {}", table.name))?;
    let count = integer(row, ROWS)
        .and_then(|n| u64::try_from(n).ok())
        .ok_or_else(|| format!("D1 gave no count of {}", table.name))?;
    if count != want.rows {
        return Err(format!(
            "D1 holds {count} rows of release {number} in {}, the manifest lists {}",
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
                "D1 holds release {number}'s rows of {} with {} {got}, the manifest's is {}: a \
                 row was dropped or altered on the way",
                table.name,
                measure_name(table, &column.column),
                column.total
            ));
        }
    }
    Ok(count)
}

/// What the fingerprint measures of `column`, as a message names it.
fn measure_name(table: &Table, column: &str) -> String {
    match table.position(column).map(|i| table.kinds[i]) {
        Some(ColumnKind::Text) => format!("total bytes of {column}"),
        _ => format!("total of {column}"),
    }
}

/// The schema version D1 records, if it records one.
///
/// # Errors
///
/// Fails when D1 cannot be queried.
pub fn schema_version(store: &mut dyn Store) -> Result<Option<i64>, String> {
    let rows = store.query("SELECT MAX(version) AS v FROM schema_version")?;
    Ok(rows.first().and_then(|r| integer(r, "v")))
}
