//! Creating, listing, reading, configuring and retiring layers (create-and-configure-a-layer,
//! read-a-layer).

use serde_json::{Value, json};
use univ_model::api::layers::{CreateLayer, LayerChange, LayerList, LayerRecord};
use univ_model::encoding::hex;

use super::{COLUMNS, LayerRow, now, readable, tagged, writable, writer};
use crate::access::Caller;
use crate::backend::{Backend, column, first, rows_as};
use crate::http::{Request, Response};
use crate::problem::{Problem, ProblemType};

/// The most items one page of a layer holds.
pub const ITEMS_PER_PAGE: u32 = 500;

/// Creates a layer owned by the caller's key: private unless asked public, its change
/// sequence at 0.
///
/// # Errors
///
/// A problem answer: no key (401), the body not a layer to create (400), the key's quota of
/// layers reached (403), or the database failing.
#[utoipa::path(
    post,
    path = "/v1/layers",
    request_body = CreateLayer,
    security(("api_key" = [])),
    responses((status = 201, description = "The layer created", body = LayerRecord))
)]
pub async fn create<B: Backend>(
    request: &Request,
    caller: &Caller,
    backend: &B,
) -> Result<Response, Problem> {
    let key = writer(caller)?;
    let asked: CreateLayer = request.json()?;
    let usage = backend
        .layers(
            "SELECT layers_per_key AS quota, (SELECT COUNT(*) FROM layers \
             WHERE key_id = ?1 AND retired_at IS NULL) AS used FROM keys WHERE id = ?1",
            &[json!(key)],
        )
        .await?;
    let row = first(usage).unwrap_or_default();
    let (quota, used): (u64, u64) = (column(&row, "quota")?, column(&row, "used")?);
    if used >= quota {
        return Err(quota_exceeded("layers_per_key", quota, used));
    }
    let mut bytes = [0_u8; 8];
    backend.random(&mut bytes)?;
    let id = format!("lay_{}", hex(&bytes));
    let created_at = now(backend);
    let insert = [
        json!(id),
        json!(key),
        json!(asked.title),
        json!(asked.description),
        json!(asked.visibility),
        json!(created_at),
    ];
    let read = [json!(id)];
    let rows = backend
        .layers_batch(&[
            (
                "INSERT INTO layers (id, key_id, title, description, visibility, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                &insert,
            ),
            (
                &format!("SELECT {COLUMNS} FROM layers WHERE id = ?1"),
                &read,
            ),
        ])
        .await?;
    let layer = one(rows)?;
    Ok(Response::json_with_status(201, &layer.record())?
        .with_header("location", &format!("/v1/layers/{id}")))
}

/// Lists every layer the caller may read and that is not retired: each public layer, and the
/// caller's key's private ones.
///
/// # Errors
///
/// A problem answer when the database cannot be read.
#[utoipa::path(
    get,
    path = "/v1/layers",
    security((), ("api_key" = [])),
    responses((status = 200, description = "Every layer the caller may read, by id", body = LayerList))
)]
pub async fn list<B: Backend>(caller: &Caller, backend: &B) -> Result<Response, Problem> {
    let key = match caller {
        Caller::Key { id, .. } => Value::String(id.clone()),
        Caller::Anonymous | Caller::Owner => Value::Null,
    };
    let rows = backend
        .layers(
            &format!(
                "SELECT {COLUMNS} FROM layers WHERE retired_at IS NULL \
                 AND (visibility = 'public' OR key_id = ?1) ORDER BY id"
            ),
            &[key],
        )
        .await?;
    let layers = rows_as::<LayerRow>(rows)?
        .iter()
        .map(LayerRow::record)
        .collect();
    Ok(Response::json(&LayerList { layers })?
        .with_header("cache-control", "private, no-cache")
        .with_header("vary", "Authorization"))
}

/// Changes a layer's title, description or visibility, advancing its change sequence by one.
///
/// # Errors
///
/// A problem answer: no key (401), another key's public layer (403), a layer the caller may
/// not see (404), a retired one (410), the body not a change (400), or the database failing.
#[utoipa::path(
    patch,
    path = "/v1/layers/{id}",
    params(("id" = String, Path, description = "The layer's id")),
    request_body = LayerChange,
    security(("api_key" = [])),
    responses((status = 200, description = "The layer as changed", body = LayerRecord))
)]
pub async fn configure<B: Backend>(
    request: &Request,
    caller: &Caller,
    id: &str,
    backend: &B,
) -> Result<Response, Problem> {
    let layer = writable(backend, caller, id).await?;
    let asked: LayerChange = request.json()?;
    let update = [
        json!(layer.id),
        json!(asked.title),
        json!(asked.description),
        json!(asked.visibility),
    ];
    let rows = changed(
        backend,
        &layer.id,
        (
            "UPDATE layers SET title = COALESCE(?2, title), \
             description = COALESCE(?3, description), \
             visibility = COALESCE(?4, visibility), sequence = sequence + 1 \
             WHERE id = ?1 AND retired_at IS NULL",
            &update,
        ),
        "layer_configured",
    )
    .await?;
    Ok(Response::json(&rows.record())?.with_header("etag", &rows.etag()))
}

/// Retires a layer: it and its items are served no more, and it leaves its key's quota of
/// layers. The retirement is the layer's last change.
///
/// # Errors
///
/// A problem answer as for [`configure`], or the database failing.
#[utoipa::path(
    delete,
    path = "/v1/layers/{id}",
    params(("id" = String, Path, description = "The layer's id")),
    security(("api_key" = [])),
    responses((status = 204, description = "The layer retired"))
)]
pub async fn retire<B: Backend>(
    caller: &Caller,
    id: &str,
    backend: &B,
) -> Result<Response, Problem> {
    let layer = writable(backend, caller, id).await?;
    let update = [json!(layer.id), json!(now(backend))];
    changed(
        backend,
        &layer.id,
        (
            "UPDATE layers SET retired_at = ?2, sequence = sequence + 1 \
             WHERE id = ?1 AND retired_at IS NULL",
            &update,
        ),
        "layer_retired",
    )
    .await?;
    Ok(Response::empty(204))
}

/// Reads a layer: its details and a page of its current items that are not retracted, by id.
///
/// # Errors
///
/// A problem answer: a layer the caller may not see (404), a retired one (410), or the
/// database failing.
#[utoipa::path(
    get,
    path = "/v1/layers/{id}",
    params(
        ("id" = String, Path, description = "The layer's id"),
        ("cursor" = Option<String>, Query, description = "The id of the last item of the page before")
    ),
    security((), ("api_key" = [])),
    responses(
        (status = 200, description = "The layer and a page of its items", body = univ_model::api::layers::LayerPage),
        (status = 304, description = "Nothing changed since the entity tag given")
    )
)]
pub async fn read<B: Backend>(
    request: &Request,
    caller: &Caller,
    id: &str,
    backend: &B,
) -> Result<Response, Problem> {
    let layer = readable(backend, caller, id).await?;
    let etag = layer.etag();
    let page = super::items::page(backend, &layer, request.param("cursor")).await?;
    tagged(request, &layer, &etag, || Response::json(&page))
}

/// Runs `update` on layer `id`, which advances its sequence by one, records the change of
/// `kind` at that sequence in the same batch, and gives the layer as changed.
async fn changed<B: Backend>(
    backend: &B,
    id: &str,
    update: (&str, &[Value]),
    kind: &str,
) -> Result<LayerRow, Problem> {
    let change = [json!(id), json!(now(backend)), json!(kind)];
    let read = [json!(id)];
    let rows = backend
        .layers_batch(&[
            update,
            (
                "INSERT INTO changes (layer_id, sequence, at, kind) \
                 SELECT id, sequence, ?2, ?3 FROM layers WHERE id = ?1",
                &change,
            ),
            (
                &format!("SELECT {COLUMNS} FROM layers WHERE id = ?1"),
                &read,
            ),
        ])
        .await?;
    one(rows)
}

/// The layer the last statement of a batch read.
fn one(rows: Vec<Vec<crate::Row>>) -> Result<LayerRow, Problem> {
    rows_as::<LayerRow>(rows.into_iter().last().unwrap_or_default())?
        .pop()
        .ok_or_else(|| {
            Problem::new(
                ProblemType::InternalError,
                "The layer written did not read back.",
            )
        })
}

/// A refusal of a write that would take a key beyond `quota`.
pub(crate) fn quota_exceeded(quota: &str, limit: u64, usage: u64) -> Problem {
    Problem::new(
        ProblemType::QuotaExceeded,
        format!(
            "The write would take the key beyond its quota {quota} of {limit}; it uses {usage}."
        ),
    )
    .with("quota", quota)
    .with("limit", limit)
    .with("usage", usage)
}
