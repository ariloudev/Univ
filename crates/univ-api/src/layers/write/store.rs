//! How a write is stored: its statements, run as one batch with the layer's sequence and
//! change, the item's current version, and the owning key's quotas.

use serde_json::{Value, json};
use univ_model::api::layers::{ItemVersion, VersionBody, VersionKind};

use super::super::LayerRow;
use super::super::config::quota_exceeded;
use super::super::validate::Checked;
use super::conflict;
use crate::backend::{Backend, column, first};
use crate::http::Response;
use crate::problem::{Problem, ProblemType};

/// One statement of a write, its SQL and its parameters.
pub(super) struct Write {
    sql: &'static str,
    params: Vec<Value>,
}

impl Write {
    pub(super) fn new(sql: &'static str, params: Vec<Value>) -> Self {
        Self { sql, params }
    }
}

pub(super) fn version_row(
    layer: &str,
    item: &str,
    version: u64,
    at: &str,
    kind: VersionKind,
    body: &str,
) -> Write {
    Write::new(
        "INSERT INTO item_versions (layer_id, item_id, version, stored_at, kind, content) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        vec![
            json!(layer),
            json!(item),
            json!(version),
            json!(at),
            json!(kind),
            json!(body),
        ],
    )
}

/// The statements that replace an item's time records on TT by those of `checked`, or remove
/// them for a retraction.
pub(super) fn times(layer: &str, item: &str, checked: Option<&Checked>) -> Vec<Write> {
    let mut writes = vec![Write::new(
        "DELETE FROM item_times WHERE layer_id = ?1 AND item_id = ?2",
        vec![json!(layer), json!(item)],
    )];
    for (record, (kind, on)) in checked.map_or(&[][..], |c| &c.times[..]).iter().enumerate() {
        writes.push(Write::new(
            "INSERT INTO item_times (layer_id, item_id, record, kind, at, earliest, latest) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            vec![
                json!(layer),
                json!(item),
                json!(record),
                json!(kind),
                json!(on.at),
                json!(on.earliest),
                json!(on.latest),
            ],
        ));
    }
    writes
}

/// Runs `writes`, then advances the layer's sequence and records the change, as one batch.
pub(super) async fn commit<B: Backend>(
    backend: &B,
    layer: &LayerRow,
    item: &str,
    version: u64,
    change: &str,
    at: &str,
    mut writes: Vec<Write>,
) -> Result<(), Problem> {
    writes.push(Write::new(
        "UPDATE layers SET sequence = sequence + 1 WHERE id = ?1 AND retired_at IS NULL",
        vec![json!(layer.id)],
    ));
    writes.push(Write::new(
        "INSERT INTO changes (layer_id, sequence, at, kind, item_id, version) \
         SELECT id, sequence, ?2, ?3, ?4, ?5 FROM layers WHERE id = ?1",
        vec![
            json!(layer.id),
            json!(at),
            json!(change),
            json!(item),
            json!(version),
        ],
    ));
    let statements: Vec<(&str, &[Value])> = writes
        .iter()
        .map(|w| (w.sql, w.params.as_slice()))
        .collect();
    match backend.layers_batch(&statements).await {
        Ok(_) => Ok(()),
        // A write that raced another and lost: what stands now is the conflict.
        Err(error) => match current(backend, &layer.id, item).await? {
            Some((current, _)) if current + 1 != version || version == 1 => Err(conflict(
                item,
                current,
                "Another write changed the item first.",
            )),
            _ => Err(error.into()),
        },
    }
}

/// The answer to a stored version: 201, the version, its place and the layer's new tag.
pub(super) fn stored(
    layer: &LayerRow,
    item: &str,
    version: u64,
    stored_at: String,
    kind: VersionKind,
    body: &str,
) -> Result<Response, Problem> {
    let body: VersionBody = serde_json::from_str(body).map_err(|e| {
        Problem::new(
            ProblemType::InternalError,
            format!("A version did not read back: {e}."),
        )
    })?;
    let answer = ItemVersion {
        item: item.to_owned(),
        version,
        stored_at,
        kind,
        body,
    };
    let path = format!("/v1/layers/{}/items/{item}", layer.id);
    Ok(Response::json_with_status(201, &answer)?
        .with_header("location", &format!("{path}/versions/{version}"))
        .with_header("etag", &format!("\"{}.{}\"", layer.id, layer.sequence + 1)))
}

/// The item's current version and whether it is retracted, if the layer holds it.
pub(super) async fn current<B: Backend>(
    backend: &B,
    layer: &str,
    item: &str,
) -> Result<Option<(u64, bool)>, Problem> {
    let rows = backend
        .layers(
            "SELECT version, retracted FROM items WHERE layer_id = ?1 AND id = ?2",
            &[json!(layer), json!(item)],
        )
        .await?;
    first(rows)
        .map(|row| {
            Ok((
                column(&row, "version")?,
                column::<u8>(&row, "retracted")? == 1,
            ))
        })
        .transpose()
}

/// The owning key's quotas on items, and how many items the layer holds.
pub(super) struct Quotas {
    pub(super) items_per_layer: u64,
    pub(super) bytes_per_item_version: u64,
    pub(super) items: u64,
}

impl Quotas {
    /// Whether a version of `body` fits the key's quota of bytes.
    pub(super) fn fits(&self, body: &str) -> Result<(), Problem> {
        let bytes = body.len() as u64;
        if bytes > self.bytes_per_item_version {
            return Err(quota_exceeded(
                "bytes_per_item_version",
                self.bytes_per_item_version,
                bytes,
            ));
        }
        Ok(())
    }
}

pub(super) async fn quotas<B: Backend>(backend: &B, layer: &LayerRow) -> Result<Quotas, Problem> {
    let rows = backend
        .layers(
            "SELECT items_per_layer, bytes_per_item_version, \
             (SELECT COUNT(*) FROM items WHERE layer_id = ?1) AS items FROM keys WHERE id = ?2",
            &[json!(layer.id), json!(layer.key_id)],
        )
        .await?;
    let row = first(rows).unwrap_or_default();
    Ok(Quotas {
        items_per_layer: column(&row, "items_per_layer")?,
        bytes_per_item_version: column(&row, "bytes_per_item_version")?,
        items: column(&row, "items")?,
    })
}

/// A version's body as stored.
pub(super) fn body_of(body: &VersionBody) -> Result<String, Problem> {
    serde_json::to_string(body).map_err(|e| {
        Problem::new(
            ProblemType::InternalError,
            format!("A version did not write: {e}."),
        )
    })
}
