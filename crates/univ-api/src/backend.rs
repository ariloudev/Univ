//! What a handler reads from: the release database, as SQL on SQLite, which D1 is in the
//! Worker and an in-memory database is in the native tests. The same statements run on both.

use std::future::Future;

use serde_json::{Map, Value};

use crate::problem::{Problem, ProblemType};

/// One row of a result, by column name, as D1 returns it: integers and reals as JSON numbers,
/// text as strings, and NULL as null.
pub type Row = Map<String, Value>;

/// Why a read of the backend failed, as the backend says it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendError(pub String);

/// The storage a request is answered from.
pub trait Backend {
    /// Runs one statement that reads the release database, with `?1`, `?2` … bound to
    /// `params`, and gives every row.
    fn releases(
        &self,
        sql: &str,
        params: &[Value],
    ) -> impl Future<Output = Result<Vec<Row>, BackendError>>;
}

/// The first row of `rows`, if any.
#[must_use]
pub fn first(mut rows: Vec<Row>) -> Option<Row> {
    if rows.is_empty() {
        None
    } else {
        Some(rows.swap_remove(0))
    }
}

/// The value of column `name` of `row`, as `T`; a missing column or another type is the
/// server's failure, not the client's.
///
/// # Errors
///
/// An internal-error problem naming the column.
pub fn column<T: serde::de::DeserializeOwned>(row: &Row, name: &str) -> Result<T, Problem> {
    let value = row.get(name).cloned().unwrap_or(Value::Null);
    serde_json::from_value(value).map_err(|error| unreadable(name, &error))
}

/// The JSON document that the text of column `name` holds, as `T`.
///
/// # Errors
///
/// An internal-error problem naming the column.
pub fn json_column<T: serde::de::DeserializeOwned>(row: &Row, name: &str) -> Result<T, Problem> {
    let text: String = column(row, name)?;
    serde_json::from_str(&text).map_err(|error| unreadable(name, &error))
}

fn unreadable(name: &str, error: &serde_json::Error) -> Problem {
    Problem::new(
        ProblemType::InternalError,
        format!("The release database's column {name} did not read: {error}."),
    )
}

impl From<BackendError> for Problem {
    fn from(error: BackendError) -> Self {
        Problem::new(
            ProblemType::InternalError,
            format!("The release database could not be read: {}.", error.0),
        )
    }
}
