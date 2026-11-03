//! The owner's operations on API keys (issue-and-revoke-keys, enforce-quotas-and-rate-limits):
//! issue, list, change and revoke. Every route here is under [`crate::access::OWNER_PREFIX`],
//! which [`crate::access::admit`] opens to the owner secret alone, so a handler is only ever
//! reached by the owner. No answer here is stored by a cache.

use std::time::{Duration, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::json;
use univ_model::api::keys::{
    IssueKey, IssuedKey, KeyChange, KeyList, KeyRecord, MAX_QUOTA, QuotaChange, Quotas,
    RateLimitTier,
};
use univ_model::api::layers::Usage;
use univ_model::encoding::hex;

use crate::access::sha256_hex;
use crate::backend::{Backend, rows_as};
use crate::http::{Request, Response};
use crate::problem::{Problem, ProblemType};

/// The columns of a key row, as [`KeyRow`] reads them, with its usage.
const COLUMNS: &str = "id, label, issued_at, revoked_at, layers_per_key, items_per_layer, \
                       bytes_per_item_version, tier, \
                       (SELECT COUNT(*) FROM layers WHERE key_id = keys.id \
                        AND retired_at IS NULL) AS layers, \
                       (SELECT COALESCE(MAX(n), 0) FROM (SELECT COUNT(*) AS n FROM items \
                        JOIN layers ON layers.id = items.layer_id \
                        WHERE layers.key_id = keys.id AND layers.retired_at IS NULL \
                        GROUP BY items.layer_id)) AS largest_layer_items";

/// Issues a key: its secret is 256 random bits, returned in this response alone; only the
/// secret's SHA-256 is stored.
///
/// # Errors
///
/// A problem answer: the body not a key to issue, or a quota above 2^53 − 1 (400), or the
/// database or the source of randomness failing.
#[utoipa::path(
    post,
    path = "/v1/owner/keys",
    request_body = IssueKey,
    security(("owner" = [])),
    responses((status = 201, description = "The key issued, with its secret", body = IssuedKey))
)]
pub async fn issue<B: Backend>(request: &Request, backend: &B) -> Result<Response, Problem> {
    let asked: IssueKey = request.json()?;
    let quotas = bounded(asked.quotas)?;
    let quotas = Quotas {
        layers_per_key: quotas
            .layers_per_key
            .unwrap_or(Quotas::default().layers_per_key),
        items_per_layer: quotas
            .items_per_layer
            .unwrap_or(Quotas::default().items_per_layer),
        bytes_per_item_version: quotas
            .bytes_per_item_version
            .unwrap_or(Quotas::default().bytes_per_item_version),
    };
    let id = format!("key_{}", hex(&random::<8>(backend)?));
    let secret = format!("univ_{}", hex(&random::<32>(backend)?));
    let issued_at = now(backend);
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
                json!(asked.tier),
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
            tier: asked.tier,
            usage: Usage::default(),
        },
        secret,
    };
    unstored(201, &issued)
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
    unstored(200, &KeyList { keys })
}

/// Changes a key's label, quotas or tier, which apply from its next request.
///
/// # Errors
///
/// A problem answer: no key of that id (404), the body not a change or a quota above
/// 2^53 − 1 (400), or the database failing.
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
    let asked: KeyChange = request.json()?;
    let quotas = bounded(asked.quotas)?;
    let update = [
        json!(id),
        json!(asked.label),
        json!(quotas.layers_per_key),
        json!(quotas.items_per_layer),
        json!(quotas.bytes_per_item_version),
        json!(asked.tier),
    ];
    let read = [json!(id)];
    let rows = backend
        .layers_batch(&[
            (
                "UPDATE keys SET label = COALESCE(?2, label), \
                 layers_per_key = COALESCE(?3, layers_per_key), \
                 items_per_layer = COALESCE(?4, items_per_layer), \
                 bytes_per_item_version = COALESCE(?5, bytes_per_item_version), \
                 tier = COALESCE(?6, tier) WHERE id = ?1",
                &update,
            ),
            (&select_one(), &read),
        ])
        .await?;
    unstored(200, &the_key(rows.into_iter().last(), id)?)
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
    let update = [json!(id), json!(now(backend))];
    let read = [json!(id)];
    let rows = backend
        .layers_batch(&[
            (
                "UPDATE keys SET revoked_at = COALESCE(revoked_at, ?2) WHERE id = ?1",
                &update,
            ),
            (&select_one(), &read),
        ])
        .await?;
    unstored(200, &the_key(rows.into_iter().last(), id)?)
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
    layers: u64,
    largest_layer_items: u64,
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
            usage: Usage {
                layers: self.layers,
                largest_layer_items: self.largest_layer_items,
            },
        }
    }
}

/// The statement that reads key `?1` with its usage.
fn select_one() -> String {
    format!("SELECT {COLUMNS} FROM keys WHERE id = ?1")
}

/// The one key `rows` hold, or a 404 naming `id`.
fn the_key(rows: Option<Vec<crate::Row>>, id: &str) -> Result<KeyRecord, Problem> {
    rows_as::<KeyRow>(rows.unwrap_or_default())?
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

/// The quotas asked, or a 400 naming the first above [`MAX_QUOTA`], which neither D1 nor a
/// JSON client would carry exactly.
fn bounded(quotas: QuotaChange) -> Result<QuotaChange, Problem> {
    for (name, quota) in [
        ("layers_per_key", quotas.layers_per_key),
        ("items_per_layer", quotas.items_per_layer),
        ("bytes_per_item_version", quotas.bytes_per_item_version),
    ] {
        if let Some(quota) = quota.filter(|quota| *quota > MAX_QUOTA) {
            return Err(Problem::new(
                ProblemType::InvalidBody,
                format!("The quota {name} is {quota}; a quota is at most {MAX_QUOTA}."),
            )
            .with("quota", name)
            .with("limit", MAX_QUOTA));
        }
    }
    Ok(quotas)
}

/// `value` as JSON with `status`, kept by no cache: owner answers are the owner's alone, and
/// an issued key's carries its secret.
fn unstored(status: u16, value: &impl Serialize) -> Result<Response, Problem> {
    Ok(Response::json_with_status(status, value)?.with_header("cache-control", "no-store"))
}

/// The time now, as RFC 3339 UTC to the millisecond.
fn now(backend: &impl Backend) -> String {
    rfc3339(backend.now_millis())
}

/// `millis` since the Unix epoch as RFC 3339 UTC to the millisecond.
pub(crate) fn rfc3339(millis: u64) -> String {
    humantime::format_rfc3339_millis(UNIX_EPOCH + Duration::from_millis(millis)).to_string()
}

/// `N` bytes from the backend's source of randomness.
fn random<const N: usize>(backend: &impl Backend) -> Result<[u8; N], Problem> {
    let mut bytes = [0_u8; N];
    backend.random(&mut bytes)?;
    Ok(bytes)
}
