//! The backend of the Worker: the release database as its D1 binding `DB`.

use serde_json::Value;
use std::sync::LazyLock;

use univ_api::{Backend, BackendError, Row, Vocabularies};
use worker::wasm_bindgen::JsValue;
use worker::{D1Database, Env};

/// The binding of the release database, as `wrangler.toml` names it.
const RELEASES: &str = "DB";

/// The Worker's storage for one request.
pub struct D1 {
    releases: D1Database,
}

impl D1 {
    /// The storage the Worker's bindings name.
    pub fn new(env: &Env) -> worker::Result<Self> {
        Ok(Self {
            releases: env.d1(RELEASES)?,
        })
    }
}

impl Backend for D1 {
    async fn releases(&self, sql: &str, params: &[Value]) -> Result<Vec<Row>, BackendError> {
        let error = |error: worker::Error| BackendError(error.to_string());
        let params: Vec<JsValue> = params.iter().map(js_value).collect();
        let statement = self.releases.prepare(sql).bind(&params).map_err(error)?;
        statement
            .all()
            .await
            .map_err(error)?
            .results()
            .map_err(error)
    }

    async fn releases_batch(
        &self,
        statements: &[(&str, &[Value])],
    ) -> Result<Vec<Vec<Row>>, BackendError> {
        let error = |error: worker::Error| BackendError(error.to_string());
        let prepared = statements
            .iter()
            .map(|(sql, params)| {
                let params: Vec<JsValue> = params.iter().map(js_value).collect();
                self.releases.prepare(*sql).bind(&params)
            })
            .collect::<worker::Result<Vec<_>>>()
            .map_err(error)?;
        self.releases
            .batch(prepared)
            .await
            .map_err(error)?
            .iter()
            .map(|result| result.results().map_err(error))
            .collect()
    }

    fn vocabularies(&self) -> &Vocabularies {
        &VOCABULARIES
    }
}

/// The release vocabularies of this isolate, kept across its requests: releases never change
/// once published.
static VOCABULARIES: LazyLock<Vocabularies> = LazyLock::new(Vocabularies::default);

/// A bound parameter as D1 takes it.
fn js_value(value: &Value) -> JsValue {
    match value {
        Value::Null => JsValue::NULL,
        Value::Bool(flag) => JsValue::from_bool(*flag),
        Value::Number(number) => number.as_f64().map_or(JsValue::NULL, JsValue::from_f64),
        Value::String(text) => JsValue::from_str(text),
        other => JsValue::from_str(&other.to_string()),
    }
}
