//! Reading items: a layer's current items page by page, one item with its versions, and any
//! version by number (read-a-layer). Their links are resolved against the release read.

use serde::Deserialize;
use serde_json::json;
use univ_model::api::layers::{
    ItemRecord, ItemVersion, LayerPage, VersionBody, VersionKind, VersionSummary,
};

use super::{LayerRow, Statement, broken, readable, run, tagged};
use crate::access::Caller;
use crate::backend::{Backend, rows_as};
use crate::http::{Request, Response};
use crate::problem::{Problem, ProblemType};

/// The most items one page of a layer holds.
pub const ITEMS_PER_PAGE: usize = 500;

/// The columns of a version row, as [`VersionRow`] reads them.
const VERSION: &str = "item_id, version, stored_at, kind, content";

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
            broken(
                &format!("Version {} of item {:?}", self.version, self.item_id),
                error,
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

fn versions(rows: Vec<crate::Row>) -> Result<Vec<ItemVersion>, Problem> {
    rows_as::<VersionRow>(rows)?
        .into_iter()
        .map(VersionRow::version)
        .collect()
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
    let mut items = versions(rows)?;
    let next = (items.len() > ITEMS_PER_PAGE).then(|| {
        items.truncate(ITEMS_PER_PAGE);
        items.last().map(|item| item.item.clone())
    });
    Ok(LayerPage {
        layer: layer.record(),
        items,
        next: next.flatten(),
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
        ("item" = String, Path, description = "The item's id"),
        ("release" = Option<u32>, Query, description = "The release links are resolved against; latest when absent")
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
    let mut rows = run(
        backend,
        &[
            Statement::new(
                "SELECT version, stored_at, kind FROM item_versions \
                 WHERE layer_id = ?1 AND item_id = ?2 ORDER BY version",
                vec![json!(layer.id), json!(item)],
            ),
            Statement::new(
                format!(
                    "SELECT {VERSION} FROM item_versions v WHERE layer_id = ?1 AND item_id = ?2 \
                     AND version = (SELECT version FROM items WHERE layer_id = ?1 AND id = ?2)"
                ),
                vec![json!(layer.id), json!(item)],
            ),
        ],
    )
    .await?
    .into_iter();
    let list: Vec<VersionSummary> = rows_as(rows.next().unwrap_or_default())?;
    let mut current = versions(rows.next().unwrap_or_default())?;
    let release = super::links::resolve(request, backend, &mut current).await?;
    let Some(current) = current.pop() else {
        return Err(not_found(&layer.id, item));
    };
    let record = ItemRecord {
        layer: layer.id.clone(),
        retracted: current.kind == VersionKind::Retraction,
        current,
        versions: list,
    };
    tagged(request, &layer, release, || Response::json(&record))
}

/// Reads one version of an item by its number; a version never changes once stored, though
/// its links are resolved against the release read.
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
        ("version" = u64, Path, description = "The version's number, from 1"),
        ("release" = Option<u32>, Query, description = "The release links are resolved against; latest when absent")
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
    let missing = || not_found(&layer.id, item).with("version", number);
    let wanted = number.parse::<u64>().map_err(|_| missing())?;
    let rows = backend
        .layers(
            &format!(
                "SELECT {VERSION} FROM item_versions \
                 WHERE layer_id = ?1 AND item_id = ?2 AND version = ?3"
            ),
            &[json!(layer.id), json!(item), json!(wanted)],
        )
        .await?;
    let mut found = versions(rows)?;
    let release = super::links::resolve(request, backend, &mut found).await?;
    let version = found.pop().ok_or_else(missing)?;
    tagged(request, &layer, release, || Response::json(&version))
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
