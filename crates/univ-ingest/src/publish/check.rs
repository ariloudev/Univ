//! What local D1 records of a release and of `latest`, and the rows of a release a table holds.

use univ_model::release::{SqlValue, Table};

use super::{QueryRow, Store};

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

/// The rows of release `number` the table holds: its own rows for a table of one release, its
/// row in the list of releases, and the id ledger as it stood at the release (rows whose
/// release is at most `number`), which each release carries whole.
///
/// # Errors
///
/// Fails when D1 cannot be queried.
pub fn release_rows(store: &mut dyn Store, table: &Table, number: u32) -> Result<u64, String> {
    let filter = if table.per_release {
        format!("release = {number}")
    } else if table.name == "releases" {
        format!("number = {number}")
    } else {
        format!("release <= {number}")
    };
    let rows = store.query(&format!(
        "SELECT COUNT(*) AS n FROM {} WHERE {filter}",
        table.name
    ))?;
    rows.first()
        .and_then(|r| integer(r, "n"))
        .and_then(|n| u64::try_from(n).ok())
        .ok_or_else(|| format!("D1 gave no count of {}", table.name))
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
