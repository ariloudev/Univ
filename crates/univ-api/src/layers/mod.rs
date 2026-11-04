//! Layers and their items (create-and-configure-a-layer, read-a-layer, publish-a-layer-item,
//! revise-or-retract-an-item, follow-a-layers-changes), in the univ-layers database.
//!
//! Who may do what is settled here once for every route. A layer is read by anyone when it is
//! public and only with its owning key when it is private; to anyone else a private layer
//! answers exactly as a layer that does not exist. A write needs the owning key: another key
//! gets 403 on a public layer and the same 404 on a private one. A retired layer answers 410,
//! with its retirement time, to a caller allowed to read it.
//!
//! Every write that changes a layer runs as one batch: the change, the layer's sequence
//! advanced by one and the change's row in the sequence take effect together or not at all.

pub mod changes;
pub mod config;
pub mod items;
pub mod links;
pub mod validate;
pub mod write;

use serde::Deserialize;
use serde_json::json;
use univ_model::api::layers::{LayerRecord, Visibility};

use crate::access::Caller;
use crate::backend::{Backend, rows_as};
use crate::http::{Request, Response};
use crate::problem::{Problem, ProblemType};

/// The columns of a layer row, as [`LayerRow`] reads them.
pub(crate) const COLUMNS: &str = "id, key_id, title, description, visibility, created_at, \
     retired_at, sequence, \
     (SELECT COUNT(*) FROM items WHERE layer_id = layers.id AND retracted = 0) AS item_count";

/// A layer as stored.
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct LayerRow {
    pub id: String,
    pub key_id: String,
    pub title: String,
    pub description: String,
    pub visibility: Visibility,
    pub created_at: String,
    pub retired_at: Option<String>,
    pub sequence: u64,
    pub item_count: u64,
}

impl LayerRow {
    pub(crate) fn record(&self) -> LayerRecord {
        LayerRecord {
            id: self.id.clone(),
            owner: self.key_id.clone(),
            title: self.title.clone(),
            description: self.description.clone(),
            visibility: self.visibility,
            created_at: self.created_at.clone(),
            sequence: self.sequence,
            item_count: self.item_count,
        }
    }

    /// The entity tag of every read of the layer and its items: the layer's change sequence,
    /// which every change advances.
    pub(crate) fn etag(&self) -> String {
        format!("\"{}.{}\"", self.id, self.sequence)
    }
}

/// The key a write comes from, or a 401: writes need an API key (write-with-an-api-key).
pub(crate) fn writer(caller: &Caller) -> Result<&str, Problem> {
    match caller {
        Caller::Key { id, .. } => Ok(id),
        Caller::Anonymous | Caller::Owner => Err(Problem::new(
            ProblemType::Unauthorized,
            "A write needs an API key, sent as `Authorization: Bearer <secret>`.",
        )
        .with_header("www-authenticate", "Bearer")),
    }
}

/// Layer `id` as `caller` may read it: a 404 for a layer that does not exist or that is
/// private to another key, the same answer either way, and a 410 for a retired one.
pub(crate) async fn readable<B: Backend>(
    backend: &B,
    caller: &Caller,
    id: &str,
) -> Result<LayerRow, Problem> {
    retired(visible(backend, caller, id).await?)
}

/// Layer `id`, retired or not, as `caller` may see it: a 404 as for [`readable`]. Only the
/// change feed reads a retired layer, so that a follower reads its retirement.
pub(crate) async fn visible<B: Backend>(
    backend: &B,
    caller: &Caller,
    id: &str,
) -> Result<LayerRow, Problem> {
    let layer = find(backend, id).await?;
    let owns = matches!(caller, Caller::Key { id: key, .. } if *key == layer.key_id);
    if layer.visibility == Visibility::Private && !owns {
        return Err(not_found(id));
    }
    Ok(layer)
}

/// Layer `id` as `caller` may change it: a 401 without a key, a 403 for another key's public
/// layer, a 404 as for [`readable`], and a 410 for a retired one.
pub(crate) async fn writable<B: Backend>(
    backend: &B,
    caller: &Caller,
    id: &str,
) -> Result<LayerRow, Problem> {
    let key = writer(caller)?;
    let layer = find(backend, id).await?;
    if layer.key_id != key {
        return Err(match layer.visibility {
            Visibility::Private => not_found(id),
            Visibility::Public => Problem::new(
                ProblemType::NotLayerOwner,
                format!("Layer {id:?} belongs to another key, which alone may change it."),
            )
            .with("id", id),
        });
    }
    retired(layer)
}

async fn find<B: Backend>(backend: &B, id: &str) -> Result<LayerRow, Problem> {
    let rows = backend
        .layers(
            &format!("SELECT {COLUMNS} FROM layers WHERE id = ?1"),
            &[json!(id)],
        )
        .await?;
    rows_as::<LayerRow>(rows)?
        .pop()
        .ok_or_else(|| not_found(id))
}

fn retired(layer: LayerRow) -> Result<LayerRow, Problem> {
    match &layer.retired_at {
        None => Ok(layer),
        Some(at) => Err(Problem::new(
            ProblemType::LayerRetired,
            format!("Layer {:?} was retired at {at}.", layer.id),
        )
        .with("id", layer.id.as_str())
        .with("retired_at", at.as_str())),
    }
}

/// The answer to a layer that does not exist, and to a private one read without its key.
pub(crate) fn not_found(id: &str) -> Problem {
    Problem::new(
        ProblemType::LayerNotFound,
        format!("No layer has the id {id:?}."),
    )
    .with("id", id)
}

/// A read's answer marked with `etag`, or a 304 with no body when the request's
/// `If-None-Match` names it already. Layers change, so every answer is revalidated; a private
/// layer's is kept by no shared cache.
pub(crate) fn tagged(
    request: &Request,
    layer: &LayerRow,
    etag: &str,
    answer: impl FnOnce() -> Result<Response, Problem>,
) -> Result<Response, Problem> {
    let cache = match layer.visibility {
        Visibility::Public => "no-cache",
        Visibility::Private => "private, no-cache",
    };
    let response = if matches(request.header("if-none-match"), etag) {
        Response::empty(304)
    } else {
        answer()?
    };
    Ok(response
        .with_header("etag", etag)
        .with_header("cache-control", cache)
        .with_header("vary", "Authorization"))
}

/// Whether an `If-None-Match` header names `etag`, weakly compared (RFC 9110 §13.1.2).
fn matches(header: Option<&str>, etag: &str) -> bool {
    header.is_some_and(|header| {
        header
            .split(',')
            .map(str::trim)
            .any(|tag| tag == "*" || tag.strip_prefix("W/").unwrap_or(tag) == etag)
    })
}

/// The time now, as RFC 3339 UTC to the millisecond.
pub(crate) fn now(backend: &impl Backend) -> String {
    crate::keys::rfc3339(backend.now_millis())
}

#[cfg(test)]
mod tests {
    use super::matches;

    #[test]
    fn if_none_match_is_compared_weakly_and_as_a_list() {
        assert!(matches(Some("\"l.3\""), "\"l.3\""));
        assert!(matches(Some("W/\"l.3\""), "\"l.3\""));
        assert!(matches(Some("\"l.2\", \"l.3\""), "\"l.3\""));
        assert!(matches(Some("*"), "\"l.3\""));
        assert!(!matches(Some("\"l.2\""), "\"l.3\""));
        assert!(!matches(None, "\"l.3\""));
    }
}
