//! The backend of the Worker: the release database as its D1 binding `DB`, the univ-layers
//! database as `LAYERS`, the releases bucket as its R2 binding `RELEASES`, the rate limiters
//! `ANON`, `BASIC`, `STANDARD` and `EXTENDED`, and the secret `OWNER_SECRET`.

use serde_json::Value;
use std::sync::LazyLock;

use univ_api::{Backend, BackendError, Kept, Limiter, Row};
use univ_model::api::keys::RateLimitTier;
use worker::wasm_bindgen::JsValue;
use worker::{D1Database, Env};

/// The binding of the release database, as `wrangler.toml` names it.
const RELEASES: &str = "DB";

/// The binding of the univ-layers database, as `wrangler.toml` names it.
const LAYERS: &str = "LAYERS";

/// The binding of the releases bucket, as `wrangler.toml` names it.
pub(crate) const BUCKET: &str = "RELEASES";

/// The secret that opens the owner's operations.
const OWNER_SECRET: &str = "OWNER_SECRET";

/// The Worker's storage for one request, each binding resolved when a request first uses it.
pub struct D1 {
    env: Env,
}

impl D1 {
    /// The storage the Worker's bindings name.
    #[must_use]
    pub fn new(env: &Env) -> Self {
        Self { env: env.clone() }
    }

    fn database(&self, binding: &str) -> Result<D1Database, BackendError> {
        self.env.d1(binding).map_err(failed)
    }
}

fn failed(error: impl std::fmt::Display) -> BackendError {
    BackendError(error.to_string())
}

/// Every row of one statement on `database`.
async fn all(database: &D1Database, sql: &str, params: &[Value]) -> Result<Vec<Row>, BackendError> {
    let error = failed;
    let params: Vec<JsValue> = params.iter().map(js_value).collect();
    let statement = database.prepare(sql).bind(&params).map_err(error)?;
    statement
        .all()
        .await
        .map_err(error)?
        .results()
        .map_err(error)
}

impl Backend for D1 {
    async fn releases(&self, sql: &str, params: &[Value]) -> Result<Vec<Row>, BackendError> {
        all(&self.database(RELEASES)?, sql, params).await
    }

    async fn layers(&self, sql: &str, params: &[Value]) -> Result<Vec<Row>, BackendError> {
        all(&self.database(LAYERS)?, sql, params).await
    }

    fn random(&self, bytes: &mut [u8]) -> Result<(), BackendError> {
        getrandom::fill(bytes).map_err(|error| BackendError(error.to_string()))
    }

    fn now_millis(&self) -> u64 {
        worker::Date::now().as_millis()
    }

    async fn limit(&self, limiter: Limiter, client: &str) -> Result<bool, BackendError> {
        let error = failed;
        let binding = match limiter {
            Limiter::Anonymous => "ANON",
            Limiter::Key(RateLimitTier::Basic) => "BASIC",
            Limiter::Key(RateLimitTier::Standard) => "STANDARD",
            Limiter::Key(RateLimitTier::Extended) => "EXTENDED",
        };
        let outcome = self
            .env
            .rate_limiter(binding)
            .map_err(error)?
            .limit(client.to_owned())
            .await
            .map_err(error)?;
        Ok(outcome.success)
    }

    fn owner_secret(&self) -> Option<String> {
        self.env
            .secret(OWNER_SECRET)
            .ok()
            .map(|secret| secret.to_string())
            .filter(|secret| !secret.is_empty())
    }

    async fn releases_batch(
        &self,
        statements: &[(&str, &[Value])],
    ) -> Result<Vec<Vec<Row>>, BackendError> {
        let error = failed;
        let releases = self.database(RELEASES)?;
        let prepared = statements
            .iter()
            .map(|(sql, params)| {
                let params: Vec<JsValue> = params.iter().map(js_value).collect();
                releases.prepare(*sql).bind(&params)
            })
            .collect::<worker::Result<Vec<_>>>()
            .map_err(error)?;
        releases
            .batch(prepared)
            .await
            .map_err(error)?
            .iter()
            .map(|result| result.results().map_err(error))
            .collect()
    }

    async fn blob(&self, key: &str) -> Result<Option<Vec<u8>>, BackendError> {
        let error = failed;
        let bucket = self.env.bucket(BUCKET).map_err(error)?;
        let Some(object) = bucket.get(key).execute().await.map_err(error)? else {
            return Ok(None);
        };
        match object.body() {
            Some(body) => body.bytes().await.map(Some).map_err(error),
            None => Ok(Some(Vec::new())),
        }
    }

    fn kept(&self) -> &Kept {
        &KEPT
    }
}

/// What this isolate keeps across its requests: releases never change once published.
static KEPT: LazyLock<Kept> = LazyLock::new(Kept::default);

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
