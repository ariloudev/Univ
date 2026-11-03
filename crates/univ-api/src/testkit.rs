//! What the native tests answer requests from: the release database as an in-memory SQLite
//! database with the real schema, and a way to run a handler's future to its end.

use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use rusqlite::types::ValueRef;
use serde_json::Value;

use crate::backend::{Backend, BackendError, Row};
use crate::http::{Request, Response};

/// Runs `future` to its end. The backends of the tests never wait, so a future that is not
/// ready after many polls is a fault of the test.
pub fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    for _ in 0..1_000 {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
    }
    panic!("the future never finished");
}

/// The release database, with univ-model's schema, in memory.
pub struct Sqlite {
    connection: rusqlite::Connection,
}

impl Sqlite {
    /// An empty release database.
    pub fn new() -> Self {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute_batch(univ_model::release::SCHEMA)
            .unwrap();
        Self { connection }
    }

    /// Runs `sql`, statements that write the fixture's rows.
    pub fn exec(&self, sql: &str) -> &Self {
        self.connection.execute_batch(sql).unwrap();
        self
    }

    /// The answer to `request`.
    pub fn answer(&self, request: &Request) -> Response {
        block_on(crate::handle(request, self))
    }
}

impl Backend for Sqlite {
    fn releases(
        &self,
        sql: &str,
        params: &[Value],
    ) -> impl Future<Output = Result<Vec<Row>, BackendError>> {
        std::future::ready(self.query(sql, params))
    }
}

impl Sqlite {
    /// Runs one statement, as D1 would, and gives every row.
    fn query(&self, sql: &str, params: &[Value]) -> Result<Vec<Row>, BackendError> {
        let error = |error: rusqlite::Error| BackendError(error.to_string());
        let mut statement = self.connection.prepare(sql).map_err(error)?;
        let names: Vec<String> = statement
            .column_names()
            .iter()
            .map(|name| (*name).to_owned())
            .collect();
        let params: Vec<rusqlite::types::Value> = params.iter().map(sql_value).collect();
        let mut rows = statement
            .query(rusqlite::params_from_iter(params))
            .map_err(error)?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(error)? {
            let mut fields = Row::new();
            for (i, name) in names.iter().enumerate() {
                fields.insert(name.clone(), json_value(row.get_ref(i).map_err(error)?));
            }
            out.push(fields);
        }
        Ok(out)
    }
}

/// A bound parameter as SQLite takes it, the way D1 binds JSON values.
fn sql_value(value: &Value) -> rusqlite::types::Value {
    match value {
        Value::Null => rusqlite::types::Value::Null,
        Value::Bool(flag) => rusqlite::types::Value::Integer(i64::from(*flag)),
        Value::Number(number) => number.as_i64().map_or_else(
            || rusqlite::types::Value::Real(number.as_f64().unwrap()),
            rusqlite::types::Value::Integer,
        ),
        Value::String(text) => rusqlite::types::Value::Text(text.clone()),
        other => rusqlite::types::Value::Text(other.to_string()),
    }
}

/// A column's value as D1 returns it in JSON.
fn json_value(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(integer) => Value::from(integer),
        ValueRef::Real(real) => Value::from(real),
        ValueRef::Text(text) => Value::from(String::from_utf8_lossy(text).into_owned()),
        ValueRef::Blob(_) => panic!("the release database holds no blobs"),
    }
}

/// Three releases: 1 published and pruned, 2 published and latest, 3 built but never
/// published, each with two sources.
pub const RELEASES: &str = r#"
INSERT INTO releases VALUES (1, '2026-11-01T10:00:00Z', '2026-11-01T11:00:00Z',
  '2026-11-02T09:00:00Z',
  '{"name":"Planck 2018","hubble_constant":67.66,"omega_matter":0.3111}', 300.0, 'm', 'i', 2,
  'rules', 'rule', 'name', '["hipparcos-2007","twomrs"]', 10, 2, 0, 3, 1000);
INSERT INTO releases VALUES (2, '2026-11-02T08:00:00Z', '2026-11-02T08:30:00Z', NULL,
  '{"name":"Planck 2018","hubble_constant":67.66,"omega_matter":0.3111}', 300.0, 'm', 'i', 2,
  'rules', 'rule', 'name', '["hipparcos-2007","twomrs"]', 12, 2, 1, 4, 1200);
INSERT INTO releases VALUES (3, '2026-11-03T08:00:00Z', NULL, NULL,
  '{"name":"Planck 2018","hubble_constant":67.66,"omega_matter":0.3111}', 300.0, 'm', 'i', 2,
  'rules', 'rule', 'name', '["hipparcos-2007"]', 13, 2, 1, 4, 1300);
INSERT INTO latest VALUES (1, 2, '2026-11-02T08:30:00Z');
"#;
