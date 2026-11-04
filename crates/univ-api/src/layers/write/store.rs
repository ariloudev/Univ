//! How a new version of an item is stored: one batch that writes the item's row, the
//! version, the item's time records on TT and the layer's change, the owning key's quotas
//! held in the batch itself so that writes racing each other cannot pass them.

use serde_json::{Value, json};
use univ_model::api::layers::{ChangeKind, ItemVersion, VersionBody, VersionKind};
use univ_model::time::{OnTt, TimeKind};

use super::super::config::quota_exceeded;
use super::super::{ChangeOf, LayerRow, Statement, advance, broken, run, settle};
use super::conflict;
use crate::backend::Backend;
use crate::http::Response;
use crate::problem::Problem;

/// A version to store.
pub(super) struct NewVersion<'a> {
    pub item: &'a str,
    pub kind: VersionKind,
    pub body: VersionBody,
    /// The time records on TT of a version with content; none for a retraction.
    pub times: &'a [(TimeKind, OnTt)],
}

/// Stores `new` as the version after the item's current one in `layer` (version 1 for a
/// publish), and answers 201 with it.
pub(super) async fn store<B: Backend>(
    backend: &B,
    layer: &LayerRow,
    new: NewVersion<'_>,
) -> Result<Response, Problem> {
    let text = serde_json::to_string(&new.body).map_err(|e| broken("A version", e))?;
    let bytes = text.len() as u64;
    if bytes > layer.bytes_per_item_version {
        return Err(quota_exceeded(
            "bytes_per_item_version",
            layer.bytes_per_item_version,
            bytes,
        ));
    }
    let version = layer.current_version.unwrap_or(0) + 1;
    let at = backend.now_rfc3339();
    let (layer_id, item) = (layer.id.as_str(), new.item);
    let statements = statements(layer, &new, version, &at, &text);
    if let Err(error) = run(backend, &statements).await {
        return Err(settle(backend, layer_id, Some(item), error, |now| {
            if now.current_version != layer.current_version {
                Some(conflict(
                    item,
                    now.current_version.unwrap_or(0),
                    "Another write changed the item first.",
                ))
            } else if new.kind == VersionKind::Published && now.items_held >= now.items_per_layer {
                Some(quota_exceeded(
                    "items_per_layer",
                    now.items_per_layer,
                    now.items_held,
                ))
            } else {
                None
            }
        })
        .await);
    }
    let answer = ItemVersion {
        item: item.to_owned(),
        version,
        stored_at: at,
        kind: new.kind,
        body: new.body,
    };
    Ok(Response::json_with_status(201, &answer)?.with_header(
        "location",
        &format!("/v1/layers/{layer_id}/items/{item}/versions/{version}"),
    ))
}

/// The statements that store `new` as `version`, its body `text`, ending with the change.
fn statements(
    layer: &LayerRow,
    new: &NewVersion,
    version: u64,
    at: &str,
    text: &str,
) -> Vec<Statement> {
    let current = version - 1;
    let (layer_id, item) = (layer.id.as_str(), new.item);
    let mut statements = vec![if new.kind == VersionKind::Published {
        // The quota of items is held here: past it, no row is inserted, the version's foreign
        // key fails and the batch with it.
        Statement::new(
            "INSERT INTO items (layer_id, id, version) SELECT ?1, ?2, 1 FROM layers \
             WHERE id = ?1 AND items_held < ?3",
            vec![json!(layer_id), json!(item), json!(layer.items_per_layer)],
        )
    } else {
        Statement::new(
            "UPDATE items SET version = ?3, retracted = ?4 \
             WHERE layer_id = ?1 AND id = ?2 AND version = ?5 AND retracted = 0",
            vec![
                json!(layer_id),
                json!(item),
                json!(version),
                json!(i32::from(new.kind == VersionKind::Retraction)),
                json!(current),
            ],
        )
    }];
    // A version written meanwhile holds this number already, and fails the batch.
    statements.push(Statement::new(
        "INSERT INTO item_versions (layer_id, item_id, version, stored_at, kind, content) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        vec![
            json!(layer_id),
            json!(item),
            json!(version),
            json!(at),
            json!(new.kind),
            json!(text),
        ],
    ));
    statements.push(Statement::new(
        "DELETE FROM item_times WHERE layer_id = ?1 AND item_id = ?2",
        vec![json!(layer_id), json!(item)],
    ));
    if !new.times.is_empty() {
        let times: Vec<Value> = new
            .times
            .iter()
            .map(|(kind, on)| json!([kind, on.at, on.earliest, on.latest]))
            .collect();
        statements.push(Statement::new(
            "INSERT INTO item_times (layer_id, item_id, record, kind, at, earliest, latest) \
             SELECT ?1, ?2, key, json_extract(value, '$[0]'), json_extract(value, '$[1]'), \
             json_extract(value, '$[2]'), json_extract(value, '$[3]') FROM json_each(?3)",
            vec![json!(layer_id), json!(item), json!(times)],
        ));
    }
    let (kind, counted, held) = match new.kind {
        VersionKind::Published => (ChangeKind::ItemPublished, 1, 1),
        VersionKind::Update | VersionKind::Correction => (ChangeKind::ItemRevised, 0, 0),
        VersionKind::Retraction => (ChangeKind::ItemRetracted, -1, 0),
    };
    let change = ChangeOf {
        kind,
        item: Some(item),
        version: Some(version),
        at,
        counted,
        held,
    };
    statements.extend(advance(layer_id, &change));
    statements
}
