//! What a handler reads from: the release and univ-layers databases, as SQL on SQLite, which D1 is in the
//! Worker and an in-memory database is in the native tests. The same statements run on both.

use std::future::Future;

use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use univ_model::api::keys::{ANONYMOUS_REQUESTS, RateLimitTier};

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

    /// Runs several statements that read the release database in one round trip, as D1's
    /// batch does, and gives each one's rows in order.
    fn releases_batch(
        &self,
        statements: &[(&str, &[Value])],
    ) -> impl Future<Output = Result<Vec<Vec<Row>>, BackendError>>;

    /// The object of `key` in the releases bucket, read whole: for small files such as a
    /// tile manifest. None when the bucket holds no such key.
    fn blob(&self, key: &str) -> impl Future<Output = Result<Option<Vec<u8>>, BackendError>>;

    /// What this backend keeps between requests: for the Worker, one keeping per isolate.
    fn kept(&self) -> &crate::Kept;

    /// Runs one statement on the univ-layers database (keys, layers, items), reading or
    /// writing, with `?1`, `?2` … bound to `params`, and gives every row.
    fn layers(
        &self,
        sql: &str,
        params: &[Value],
    ) -> impl Future<Output = Result<Vec<Row>, BackendError>>;

    /// Runs several statements on the univ-layers database as one transaction, as D1's batch
    /// does: all of them take effect, or, when one fails, none.
    fn layers_batch(
        &self,
        statements: &[(&str, &[Value])],
    ) -> impl Future<Output = Result<Vec<Vec<Row>>, BackendError>>;

    /// Fills `bytes` from a cryptographically secure source of randomness.
    ///
    /// # Errors
    ///
    /// When the platform has none to give.
    fn random(&self, bytes: &mut [u8]) -> Result<(), BackendError>;

    /// The time now, in milliseconds since the Unix epoch.
    fn now_millis(&self) -> u64;

    /// Counts one request of `client` against `limiter`; true while it is within the limit.
    fn limit(
        &self,
        limiter: Limiter,
        client: &str,
    ) -> impl Future<Output = Result<bool, BackendError>>;

    /// The time now, as RFC 3339 UTC to the millisecond, as every stored time is written.
    fn now_rfc3339(&self) -> String {
        let at = std::time::UNIX_EPOCH + std::time::Duration::from_millis(self.now_millis());
        humantime::format_rfc3339_millis(at).to_string()
    }

    /// The owner secret the Worker was given, if any; without one, every owner operation is
    /// refused.
    fn owner_secret(&self) -> Option<String>;
}

/// The rate limits a request is counted against: anonymous clients by address, and keys by
/// their tier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Limiter {
    Anonymous,
    Key(RateLimitTier),
}

impl Limiter {
    /// The requests it allows in [`univ_model::api::keys::RATE_WINDOW_SECONDS`].
    #[must_use]
    pub fn requests(self) -> u32 {
        match self {
            Self::Anonymous => ANONYMOUS_REQUESTS,
            Self::Key(tier) => tier.requests(),
        }
    }
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

/// The value of column `name` of `row`, as `T`; a missing column reads as NULL. A column of
/// another type is the server's failure, not the client's.
///
/// # Errors
///
/// An internal-error problem naming the column.
pub fn column<T: DeserializeOwned>(row: &Row, name: &str) -> Result<T, Problem> {
    T::deserialize(row.get(name).unwrap_or(&Value::Null)).map_err(|error| unreadable(name, &error))
}

/// The JSON document that the text of column `name` holds, as `T`; a NULL column reads as
/// JSON null, so an `Option` is none.
///
/// # Errors
///
/// An internal-error problem naming the column.
pub fn json_column<T: DeserializeOwned>(row: &Row, name: &str) -> Result<T, Problem> {
    match row.get(name) {
        Some(Value::String(text)) => serde_json::from_str(text),
        other => T::deserialize(other.unwrap_or(&Value::Null)),
    }
    .map_err(|error| unreadable(name, &error))
}

/// Each row as `T`, its columns read as the fields of the same names.
///
/// # Errors
///
/// An internal-error problem when a row does not read.
pub fn rows_as<T: DeserializeOwned>(rows: Vec<Row>) -> Result<Vec<T>, Problem> {
    rows.into_iter()
        .map(|row| {
            serde_json::from_value(Value::Object(row)).map_err(|error| {
                Problem::new(
                    ProblemType::InternalError,
                    format!("A row of the database did not read: {error}."),
                )
            })
        })
        .collect()
}

fn unreadable(name: &str, error: &serde_json::Error) -> Problem {
    Problem::new(
        ProblemType::InternalError,
        format!("The database's column {name} did not read: {error}."),
    )
}

impl From<BackendError> for Problem {
    fn from(error: BackendError) -> Self {
        Problem::new(
            ProblemType::InternalError,
            format!("The database could not be read: {}.", error.0),
        )
    }
}
