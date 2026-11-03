//! The owner's operations on API keys (issue-and-revoke-keys, enforce-quotas-and-rate-limits):
//! issue, list, change and revoke. Every route here is under [`crate::access::OWNER_PREFIX`],
//! which [`crate::access::authenticate`] opens to the owner secret alone, so a handler is only
//! ever reached by the owner.

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::json;
use univ_model::api::keys::{
    IssueKey, IssuedKey, KeyChange, KeyList, KeyRecord, QuotaChange, Quotas, RateLimitTier,
};

use crate::access::sha256_hex;
use crate::backend::{Backend, rows_as};
use crate::clock::rfc3339;
use crate::http::{Request, Response};
use crate::problem::{Problem, ProblemType};

/// The columns of a key row, as [`record`] reads them.
const COLUMNS: &str = "id, label, issued_at, revoked_at, layers_per_key, items_per_layer, \
                       bytes_per_item_version, tier";

/// Issues a key: its secret is 256 random bits, returned in this response alone; only the
/// secret's SHA-256 is stored.
///
/// # Errors
///
/// A problem answer: the body not a key to issue (400), or the database or the source of
/// randomness failing.
#[utoipa::path(
    post,
    path = "/v1/owner/keys",
    request_body = IssueKey,
    security(("owner" = [])),
    responses((status = 201, description = "The key issued, with its secret", body = IssuedKey))
)]
pub async fn issue<B: Backend>(request: &Request, backend: &B) -> Result<Response, Problem> {
    let asked: IssueKey = body(request)?;
    let quotas = apply(Quotas::default(), asked.quotas);
    let tier = asked.tier.unwrap_or_default();
    let id = format!("key_{}", hex(&random::<8>(backend)?));
    let secret = format!("univ_{}", hex(&random::<32>(backend)?));
    let issued_at = rfc3339(backend.now_millis());
    backend
        .layers(
            "INSERT INTO keys (id, label, secret_sha256, issued_at, layers_per_key, \
             items_per_layer, bytes_per_item_version, tier) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            &[
                json!(id),
                json!(asked.label),
                json!(sha256_hex(&secret)),
                json!(issued_at),
                json!(quotas.layers_per_key),
                json!(quotas.items_per_layer),
                json!(quotas.bytes_per_item_version),
                json!(tier.as_str()),
            ],
        )
        .await?;
    let issued = IssuedKey {
        key: KeyRecord {
            id,
            label: asked.label,
            issued_at,
            revoked_at: None,
            quotas,
            tier,
        },
        secret,
    };
    Ok(Response::json_with_status(201, &issued)?.with_header("cache-control", "no-store"))
}

/// Lists every key issued, revoked ones included, without their secrets.
///
/// # Errors
///
/// A problem answer when the database cannot be read.
#[utoipa::path(
    get,
    path = "/v1/owner/keys",
    security(("owner" = [])),
    responses((status = 200, description = "Every key, oldest first", body = KeyList))
)]
pub async fn list<B: Backend>(backend: &B) -> Result<Response, Problem> {
    let rows = backend
        .layers(
            &format!("SELECT {COLUMNS} FROM keys ORDER BY issued_at, id"),
            &[],
        )
        .await?;
    let keys = rows_as::<KeyRow>(rows)?
        .into_iter()
        .map(KeyRow::record)
        .collect();
    Ok(Response::json(&KeyList { keys })?.with_header("cache-control", "no-store"))
}

/// Changes a key's label, quotas or tier, which apply from its next request.
///
/// # Errors
///
/// A problem answer: no key of that id (404), the body not a change (400), or the database
/// failing.
#[utoipa::path(
    patch,
    path = "/v1/owner/keys/{id}",
    params(("id" = String, Path, description = "The key's id")),
    request_body = KeyChange,
    security(("owner" = [])),
    responses((status = 200, description = "The key as changed", body = KeyRecord))
)]
pub async fn change<B: Backend>(
    request: &Request,
    id: &str,
    backend: &B,
) -> Result<Response, Problem> {
    let asked: KeyChange = body(request)?;
    let key = find(backend, id).await?;
    let quotas = apply(key.quotas, asked.quotas);
    let tier = asked.tier.unwrap_or(key.tier);
    let label = asked.label.unwrap_or(key.label);
    backend
        .layers(
            "UPDATE keys SET label = ?2, layers_per_key = ?3, items_per_layer = ?4, \
             bytes_per_item_version = ?5, tier = ?6 WHERE id = ?1",
            &[
                json!(id),
                json!(label),
                json!(quotas.layers_per_key),
                json!(quotas.items_per_layer),
                json!(quotas.bytes_per_item_version),
                json!(tier.as_str()),
            ],
        )
        .await?;
    let key = find(backend, id).await?;
    Ok(Response::json(&key)?.with_header("cache-control", "no-store"))
}

/// Revokes a key: it is refused from the next request on, and its revocation time recorded.
/// Revoking a key already revoked keeps its first revocation time.
///
/// # Errors
///
/// A problem answer: no key of that id (404), or the database failing.
#[utoipa::path(
    post,
    path = "/v1/owner/keys/{id}/revoke",
    params(("id" = String, Path, description = "The key's id")),
    security(("owner" = [])),
    responses((status = 200, description = "The key as revoked", body = KeyRecord))
)]
pub async fn revoke<B: Backend>(id: &str, backend: &B) -> Result<Response, Problem> {
    find(backend, id).await?;
    backend
        .layers(
            "UPDATE keys SET revoked_at = ?2 WHERE id = ?1 AND revoked_at IS NULL",
            &[json!(id), json!(rfc3339(backend.now_millis()))],
        )
        .await?;
    let key = find(backend, id).await?;
    Ok(Response::json(&key)?.with_header("cache-control", "no-store"))
}

/// A key row as stored.
#[derive(Deserialize)]
struct KeyRow {
    id: String,
    label: String,
    issued_at: String,
    revoked_at: Option<String>,
    layers_per_key: u64,
    items_per_layer: u64,
    bytes_per_item_version: u64,
    tier: RateLimitTier,
}

impl KeyRow {
    fn record(self) -> KeyRecord {
        KeyRecord {
            id: self.id,
            label: self.label,
            issued_at: self.issued_at,
            revoked_at: self.revoked_at,
            quotas: Quotas {
                layers_per_key: self.layers_per_key,
                items_per_layer: self.items_per_layer,
                bytes_per_item_version: self.bytes_per_item_version,
            },
            tier: self.tier,
        }
    }
}

/// Key `id`, or a 404 naming it.
async fn find<B: Backend>(backend: &B, id: &str) -> Result<KeyRecord, Problem> {
    let rows = backend
        .layers(
            &format!("SELECT {COLUMNS} FROM keys WHERE id = ?1"),
            &[json!(id)],
        )
        .await?;
    rows_as::<KeyRow>(rows)?
        .pop()
        .map(KeyRow::record)
        .ok_or_else(|| {
            Problem::new(
                ProblemType::KeyNotFound,
                format!("No key has the id {id:?}."),
            )
            .with("id", id)
        })
}

/// `quotas` with each quota `change` names set.
fn apply(quotas: Quotas, change: QuotaChange) -> Quotas {
    Quotas {
        layers_per_key: change.layers_per_key.unwrap_or(quotas.layers_per_key),
        items_per_layer: change.items_per_layer.unwrap_or(quotas.items_per_layer),
        bytes_per_item_version: change
            .bytes_per_item_version
            .unwrap_or(quotas.bytes_per_item_version),
    }
}

/// The request's JSON body as a `T`, or a 400 naming what is wrong with it.
fn body<T: DeserializeOwned>(request: &Request) -> Result<T, Problem> {
    serde_json::from_slice(&request.body).map_err(|error| {
        Problem::new(
            ProblemType::InvalidBody,
            format!("The body is not what this route takes: {error}."),
        )
    })
}

/// `N` bytes from the backend's source of randomness.
fn random<const N: usize>(backend: &impl Backend) -> Result<[u8; N], Problem> {
    let mut bytes = [0_u8; N];
    backend.random(&mut bytes)?;
    Ok(bytes)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut hex, byte| {
        use std::fmt::Write;
        let _ = write!(hex, "{byte:02x}");
        hex
    })
}
