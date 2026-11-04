//! Creating, listing, reading, configuring and retiring layers (create-and-configure-a-layer,
//! read-a-layer).

use serde_json::json;
use univ_model::api::layers::{
    ChangeKind, CreateLayer, LayerChange, LayerList, LayerPage, LayerRecord,
};
use univ_model::encoding::hex;

use super::{
    COLUMNS, ChangeOf, LayerRow, Statement, advance, broken, readable, run, settle, tagged,
    writable, writer,
};
use crate::access::Caller;
use crate::backend::{Backend, column, first, rows_as};
use crate::http::{Request, Response};
use crate::problem::{Problem, ProblemType};

/// Creates a layer owned by the caller's key: private unless asked public, its change
/// sequence at 0. The insert itself holds the key to its quota of layers.
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
    let mut bytes = [0_u8; 8];
    backend.random(&mut bytes)?;
    let id = format!("lay_{}", hex(&bytes));
    let rows = run(
        backend,
        &[
            Statement::new(
                "INSERT INTO layers (id, key_id, title, description, visibility, created_at) \
                 SELECT ?1, ?2, ?3, ?4, ?5, ?6 FROM keys WHERE id = ?2 AND layers_per_key > \
                 (SELECT COUNT(*) FROM layers WHERE key_id = ?2 AND retired_at IS NULL)",
                vec![
                    json!(id),
                    json!(key),
                    json!(asked.title),
                    json!(asked.description),
                    json!(asked.visibility),
                    json!(backend.now_rfc3339()),
                ],
            ),
            select(&id),
        ],
    )
    .await?;
    let Some(layer) = last(rows)? else {
        return Err(layers_quota(backend, key).await?);
    };
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
    let rows = backend
        .layers(
            &format!(
                "SELECT {COLUMNS} FROM layers l JOIN keys k ON k.id = l.key_id \
                 WHERE l.retired_at IS NULL AND {} ORDER BY l.id",
                super::readable_by("l.", "?1")
            ),
            &[json!(super::key_of(caller))],
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
    let layer = writable(backend, caller, id, None).await?;
    let asked: LayerChange = request.json()?;
    let update = Statement::new(
        "UPDATE layers SET title = COALESCE(?2, title), description = COALESCE(?3, description), \
         visibility = COALESCE(?4, visibility) WHERE id = ?1",
        vec![
            json!(layer.id),
            json!(asked.title),
            json!(asked.description),
            json!(asked.visibility),
        ],
    );
    let changed = changed(backend, &layer, update, ChangeKind::LayerConfigured).await?;
    Response::json(&changed.record())
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
    let layer = writable(backend, caller, id, None).await?;
    let update = Statement::new(
        "UPDATE layers SET retired_at = ?2 WHERE id = ?1",
        vec![json!(layer.id), json!(backend.now_rfc3339())],
    );
    changed(backend, &layer, update, ChangeKind::LayerRetired).await?;
    Ok(Response::empty(204))
}

/// Reads a layer: its details and a page of its current items that are not retracted, by id,
/// their links resolved against the release read.
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
        ("cursor" = Option<String>, Query, description = "The id of the last item of the page before"),
        ("release" = Option<u32>, Query, description = "The release links are resolved against; latest when absent")
    ),
    security((), ("api_key" = [])),
    responses(
        (status = 200, description = "The layer and a page of its items", body = LayerPage),
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
    let mut page = super::items::page(backend, &layer, request.param("cursor")).await?;
    let release = super::links::resolve(request, backend, &mut page.items).await?;
    tagged(request, &layer, release, || Response::json(&page))
}

/// Runs `update` on `layer` and records the change of `kind`, as one batch, and gives the
/// layer as changed. The change is recorded first, so that a retirement records itself while
/// the layer still stands.
async fn changed<B: Backend>(
    backend: &B,
    layer: &LayerRow,
    update: Statement,
    kind: ChangeKind,
) -> Result<LayerRow, Problem> {
    let at = backend.now_rfc3339();
    let change = ChangeOf {
        kind,
        item: None,
        version: None,
        at: &at,
        counted: 0,
        held: 0,
    };
    let [bump, record] = advance(&layer.id, &change);
    match run(backend, &[bump, record, update, select(&layer.id)]).await {
        Ok(rows) => last(rows)?.ok_or_else(|| broken("The layer written", "it did not read back")),
        Err(error) => Err(settle(backend, &layer.id, None, error, |_| None).await),
    }
}

/// The statement that reads layer `id` with its key's quotas.
fn select(id: &str) -> Statement {
    Statement::new(
        format!("SELECT {COLUMNS} FROM layers l JOIN keys k ON k.id = l.key_id WHERE l.id = ?1"),
        vec![json!(id)],
    )
}

/// The layer the last statement of a batch read, if it read one.
fn last(rows: Vec<Vec<crate::Row>>) -> Result<Option<LayerRow>, Problem> {
    Ok(rows_as::<LayerRow>(rows.into_iter().last().unwrap_or_default())?.pop())
}

/// The refusal of a layer beyond `key`'s quota of layers, with its usage.
async fn layers_quota<B: Backend>(backend: &B, key: &str) -> Result<Problem, Problem> {
    let rows = backend
        .layers(
            "SELECT layers_per_key AS quota, (SELECT COUNT(*) FROM layers \
             WHERE key_id = ?1 AND retired_at IS NULL) AS used FROM keys WHERE id = ?1",
            &[json!(key)],
        )
        .await?;
    let row = first(rows).unwrap_or_default();
    Ok(quota_exceeded(
        "layers_per_key",
        column(&row, "quota")?,
        column(&row, "used")?,
    ))
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
