//! Reading items: a layer's current items page by page, one item with its versions, and any
//! version by number (read-a-layer).

use serde::Deserialize;
use serde_json::json;
use univ_model::api::layers::{
    ItemRecord, ItemVersion, LayerPage, VersionBody, VersionKind, VersionSummary,
};

use super::config::ITEMS_PER_PAGE;
use super::{LayerRow, readable, tagged};
use crate::access::Caller;
use crate::backend::{Backend, rows_as};
use crate::http::{Request, Response};
use crate::problem::{Problem, ProblemType};

/// A version row as stored.
#[derive(Deserialize)]
struct VersionRow {
    item_id: String,
    version: u64,
    stored_at: String,
    kind: VersionKind,
    content: String,
}

impl VersionRow {
    fn version(self) -> Result<ItemVersion, Problem> {
        let body: VersionBody = serde_json::from_str(&self.content).map_err(|error| {
            Problem::new(
                ProblemType::InternalError,
                format!(
                    "Version {} of item {:?} did not read: {error}.",
                    self.version, self.item_id
                ),
            )
        })?;
        Ok(ItemVersion {
            item: self.item_id,
            version: self.version,
            stored_at: self.stored_at,
            kind: self.kind,
            body,
        })
    }
}

/// One page of `layer`'s current items that are not retracted, after the item `cursor`.
pub(crate) async fn page<B: Backend>(
    backend: &B,
    layer: &LayerRow,
    cursor: Option<String>,
) -> Result<LayerPage, Problem> {
    let rows = backend
        .layers(
            "SELECT v.item_id, v.version, v.stored_at, v.kind, v.content FROM items i \
             JOIN item_versions v ON v.layer_id = i.layer_id AND v.item_id = i.id \
             AND v.version = i.version \
             WHERE i.layer_id = ?1 AND i.retracted = 0 AND i.id > ?2 ORDER BY i.id LIMIT ?3",
            &[
                json!(layer.id),
                json!(cursor.unwrap_or_default()),
                json!(ITEMS_PER_PAGE + 1),
            ],
        )
        .await?;
    let mut items = rows_as::<VersionRow>(rows)?
        .into_iter()
        .map(VersionRow::version)
        .collect::<Result<Vec<_>, _>>()?;
    let next = if items.len() > ITEMS_PER_PAGE as usize {
        items.truncate(ITEMS_PER_PAGE as usize);
        items.last().map(|item| item.item.clone())
    } else {
        None
    };
    Ok(LayerPage {
        layer: layer.record(),
        items,
        next,
    })
}

/// Reads an item: its current version, a retraction included, and the list of all its
/// versions.
///
/// # Errors
///
/// A problem answer: a layer the caller may not see or an item it does not hold (404), a
/// retired layer (410), or the database failing.
#[utoipa::path(
    get,
    path = "/v1/layers/{id}/items/{item}",
    params(
        ("id" = String, Path, description = "The layer's id"),
        ("item" = String, Path, description = "The item's id")
    ),
    security((), ("api_key" = [])),
    responses(
        (status = 200, description = "The item's current version and its versions", body = ItemRecord),
        (status = 304, description = "Nothing changed since the entity tag given")
    )
)]
pub async fn read<B: Backend>(
    request: &Request,
    caller: &Caller,
    id: &str,
    item: &str,
    backend: &B,
) -> Result<Response, Problem> {
    let layer = readable(backend, caller, id).await?;
    let mut versions = versions(backend, &layer.id, item, None).await?;
    super::links::resolve(request, backend, &mut versions).await?;
    let Some(current) = versions.pop() else {
        return Err(not_found(&layer.id, item));
    };
    let mut list: Vec<VersionSummary> = versions.iter().map(summary).collect();
    list.push(summary(&current));
    let record = ItemRecord {
        layer: layer.id.clone(),
        retracted: current.kind == VersionKind::Retraction,
        current,
        versions: list,
    };
    tagged(request, &layer, &layer.etag(), || Response::json(&record))
}

/// Reads one version of an item by its number; a version never changes once stored.
///
/// # Errors
///
/// A problem answer: a layer the caller may not see, or an item or version it does not hold
/// (404), a retired layer (410), or the database failing.
#[utoipa::path(
    get,
    path = "/v1/layers/{id}/items/{item}/versions/{version}",
    params(
        ("id" = String, Path, description = "The layer's id"),
        ("item" = String, Path, description = "The item's id"),
        ("version" = u64, Path, description = "The version's number, from 1")
    ),
    security((), ("api_key" = [])),
    responses((status = 200, description = "The version", body = ItemVersion))
)]
pub async fn version<B: Backend>(
    request: &Request,
    caller: &Caller,
    id: &str,
    item: &str,
    number: &str,
    backend: &B,
) -> Result<Response, Problem> {
    let layer = readable(backend, caller, id).await?;
    let wanted = number.parse::<u64>().ok();
    let found = match wanted {
        Some(wanted) => versions(backend, &layer.id, item, Some(wanted))
            .await?
            .pop(),
        None => None,
    };
    let version = found.ok_or_else(|| not_found(&layer.id, item).with("version", number))?;
    let mut version = [version];
    super::links::resolve(request, backend, &mut version).await?;
    let [version] = version;
    Ok(Response::json(&version)?.with_header("cache-control", "private, no-cache"))
}

/// The versions of `item`, oldest first, or only version `number`.
pub(crate) async fn versions<B: Backend>(
    backend: &B,
    layer: &str,
    item: &str,
    number: Option<u64>,
) -> Result<Vec<ItemVersion>, Problem> {
    let rows = backend
        .layers(
            "SELECT item_id, version, stored_at, kind, content FROM item_versions \
             WHERE layer_id = ?1 AND item_id = ?2 AND (?3 IS NULL OR version = ?3) \
             ORDER BY version",
            &[json!(layer), json!(item), json!(number)],
        )
        .await?;
    rows_as::<VersionRow>(rows)?
        .into_iter()
        .map(VersionRow::version)
        .collect()
}

fn summary(version: &ItemVersion) -> VersionSummary {
    VersionSummary {
        version: version.version,
        stored_at: version.stored_at.clone(),
        kind: version.kind,
    }
}

/// The answer to an item the layer does not hold.
pub(crate) fn not_found(layer: &str, item: &str) -> Problem {
    Problem::new(
        ProblemType::ItemNotFound,
        format!("Layer {layer:?} holds no item {item:?}."),
    )
    .with("layer", layer)
    .with("item", item)
}
