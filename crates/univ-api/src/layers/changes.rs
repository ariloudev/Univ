//! A layer's changes after a cursor, in the order of its change sequence
//! (follow-a-layers-changes).

use serde_json::json;
use univ_model::api::layers::{Change, ChangePage};

use super::{tagged, visible};
use crate::access::Caller;
use crate::backend::{Backend, rows_as};
use crate::http::{Request, Response};
use crate::problem::Problem;

/// The most changes one page holds.
pub const CHANGES_PER_PAGE: u32 = 1_000;

/// The query parameter of the cursor: the sequence number of the last change already read,
/// 0 for every change since the layer was created.
const SINCE: &str = "since";

/// Gives a layer's changes after a cursor, at most [`CHANGES_PER_PAGE`] of them.
///
/// # Errors
///
/// A problem answer: a cursor that is not a number or lies beyond the layer's sequence (400,
/// with the sequence), a layer the caller may not see (404), or the database failing. A
/// retired layer's changes are still served, its retirement the last of them.
#[utoipa::path(
    get,
    path = "/v1/layers/{id}/changes",
    params(
        ("id" = String, Path, description = "The layer's id"),
        ("since" = Option<u64>, Query, description = "The sequence number of the last change already read; 0, the default, for every change")
    ),
    security((), ("api_key" = [])),
    responses(
        (status = 200, description = "The changes after the cursor", body = ChangePage),
        (status = 304, description = "Nothing changed since the entity tag given")
    )
)]
pub async fn list<B: Backend>(
    request: &Request,
    caller: &Caller,
    id: &str,
    backend: &B,
) -> Result<Response, Problem> {
    let layer = visible(backend, caller, id).await?;
    let given = request.param(SINCE);
    let since = match given.as_deref() {
        None => Some(0),
        Some(text) => text.parse::<u64>().ok(),
    };
    let Some(since) = since.filter(|since| *since <= layer.sequence) else {
        let value = given.unwrap_or_default();
        return Err(Problem::invalid_parameter(
            SINCE,
            &value,
            format!(
                "The cursor must be a change's sequence number, from 0 to the layer's \
                 sequence, {}.",
                layer.sequence
            ),
        )
        .with("sequence", layer.sequence));
    };
    let rows = backend
        .layers(
            "SELECT sequence, at, kind, item_id AS item, version FROM changes \
             WHERE layer_id = ?1 AND sequence > ?2 ORDER BY sequence LIMIT ?3",
            &[json!(layer.id), json!(since), json!(CHANGES_PER_PAGE)],
        )
        .await?;
    let changes: Vec<Change> = rows_as(rows)?;
    let page = ChangePage {
        layer: layer.id.clone(),
        cursor: changes.last().map_or(since, |change| change.sequence),
        sequence: layer.sequence,
        changes,
    };
    tagged(request, &layer, &layer.etag(), || Response::json(&page))
}
