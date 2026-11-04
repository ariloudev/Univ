//! Layers and their items (create-and-configure-a-layer, read-a-layer, publish-a-layer-item,
//! revise-or-retract-an-item, follow-a-layers-changes), in the univ-layers database.
//!
//! Who may do what is settled here once for every route. A layer is read by anyone when it is
//! public and only with its owning key when it is private; to anyone else a private layer
//! answers exactly as a layer that does not exist. A write needs the owning key: another key
//! gets 403 on a public layer and the same 404 on a private one. A retired layer answers 410,
//! with its retirement time, to a caller allowed to read it.
//!
//! Every write that changes a layer runs as one batch, which [`advance`] ends: the change,
//! the layer's sequence advanced by one with its item counts, and the change's row in the
//! sequence take effect together or not at all.

pub mod changes;
pub mod config;
pub mod items;
pub mod links;
pub mod validate;
pub mod write;

use serde::Deserialize;
use serde_json::{Value, json};
use univ_model::api::layers::{ChangeKind, LayerRecord, Visibility};

use crate::access::Caller;
use crate::backend::{Backend, BackendError, Row, rows_as};
use crate::http::{Request, Response};
use crate::problem::{Problem, ProblemType};

/// A layer's columns with its owning key's quotas, as [`LayerRow`] reads them; the query
/// names the layer `l` and its key `k`.
pub(crate) const COLUMNS: &str = "l.id, l.key_id, l.title, l.description, l.visibility, \
     l.created_at, l.retired_at, l.sequence, l.item_count, l.items_held, k.items_per_layer, \
     k.bytes_per_item_version";

/// A layer as stored, with its owning key's quotas and, for a write, the item it concerns.
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
    pub items_held: u64,
    pub items_per_layer: u64,
    pub bytes_per_item_version: u64,
    /// The current version of the item a write concerns, if the layer holds it.
    #[serde(default)]
    pub current_version: Option<u64>,
    #[serde(default)]
    pub current_retracted: Option<u8>,
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
}

/// The key a write comes from, or a 401: writes need an API key (write-with-an-api-key).
/// The key `caller` reads with, whose private layers it may read.
pub(crate) fn key_of(caller: &Caller) -> Option<&str> {
    match caller {
        Caller::Key { id, .. } => Some(id),
        Caller::Anonymous | Caller::Owner => None,
    }
}

/// The SQL test of a layer row a key may read: a public layer, or one of its own. `table` is
/// the row's alias with its dot (or nothing), `key` the parameter that binds the key or null.
pub(crate) fn readable_by(table: &str, key: &str) -> String {
    format!("({table}visibility = 'public' OR {table}key_id IS {key})")
}

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
    let layer = find(backend, id, None).await?;
    let owns = matches!(caller, Caller::Key { id: key, .. } if *key == layer.key_id);
    if layer.visibility == Visibility::Private && !owns {
        return Err(not_found(id));
    }
    Ok(layer)
}

/// Layer `id` as `caller` may change it, with item `item`'s current version where named: a
/// 401 without a key, a 403 for another key's public layer, a 404 as for [`readable`], and a
/// 410 for a retired one.
pub(crate) async fn writable<B: Backend>(
    backend: &B,
    caller: &Caller,
    id: &str,
    item: Option<&str>,
) -> Result<LayerRow, Problem> {
    let key = writer(caller)?;
    let layer = find(backend, id, item).await?;
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

/// Layer `id` with its key's quotas, and item `item`'s current version where named: one read.
async fn find<B: Backend>(backend: &B, id: &str, item: Option<&str>) -> Result<LayerRow, Problem> {
    let rows = backend
        .layers(
            &format!(
                "SELECT {COLUMNS}, i.version AS current_version, \
                 i.retracted AS current_retracted FROM layers l JOIN keys k ON k.id = l.key_id \
                 LEFT JOIN items i ON i.layer_id = l.id AND i.id = ?2 WHERE l.id = ?1"
            ),
            &[json!(id), json!(item)],
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

/// One statement of a batch, its SQL and its parameters.
pub(crate) struct Statement {
    sql: String,
    params: Vec<Value>,
}

impl Statement {
    pub(crate) fn new(sql: impl Into<String>, params: Vec<Value>) -> Self {
        Self {
            sql: sql.into(),
            params,
        }
    }
}

/// Runs `statements` as one batch on the univ-layers database.
pub(crate) async fn run<B: Backend>(
    backend: &B,
    statements: &[Statement],
) -> Result<Vec<Vec<Row>>, BackendError> {
    let borrowed: Vec<(&str, &[Value])> = statements
        .iter()
        .map(|s| (s.sql.as_str(), s.params.as_slice()))
        .collect();
    backend.layers_batch(&borrowed).await
}

/// What a change of a layer is: its kind, the item and version it made, when, and how it
/// moves the layer's counts of items not retracted and of items held.
pub(crate) struct ChangeOf<'a> {
    pub kind: ChangeKind,
    pub item: Option<&'a str>,
    pub version: Option<u64>,
    pub at: &'a str,
    pub counted: i64,
    pub held: i64,
}

/// The statements that end every write's batch: the layer's sequence advanced by one with
/// its counts, and the change recorded at the new sequence number. A layer retired since it
/// was read matches no row, so the change's row repeats an existing sequence number and the
/// whole batch fails.
pub(crate) fn advance(layer: &str, change: &ChangeOf) -> [Statement; 2] {
    [
        Statement::new(
            "UPDATE layers SET sequence = sequence + 1, item_count = item_count + ?2, \
             items_held = items_held + ?3 WHERE id = ?1 AND retired_at IS NULL",
            vec![json!(layer), json!(change.counted), json!(change.held)],
        ),
        Statement::new(
            "INSERT INTO changes (layer_id, sequence, at, kind, item_id, version) \
             SELECT id, sequence, ?2, ?3, ?4, ?5 FROM layers WHERE id = ?1",
            vec![
                json!(layer),
                json!(change.at),
                json!(change.kind),
                json!(change.item),
                json!(change.version),
            ],
        ),
    ]
}

/// The answer to a write's batch that failed: the layer read again, a retirement that came
/// first answering 410 and anything `then` finds standing in its way answering as `then` says;
/// otherwise the failure itself.
pub(crate) async fn settle<B: Backend>(
    backend: &B,
    layer: &str,
    item: Option<&str>,
    error: BackendError,
    then: impl FnOnce(&LayerRow) -> Option<Problem>,
) -> Problem {
    match find(backend, layer, item).await.and_then(retired) {
        Err(problem) => problem,
        Ok(now) => then(&now).unwrap_or_else(|| error.into()),
    }
}

/// The entity tag of a read of a layer or its items: the layer's change sequence, which every
/// change advances, and the release its links were resolved against, if any.
pub(crate) fn etag(layer: &LayerRow, release: Option<u32>) -> String {
    match release {
        Some(release) => format!("\"{}.{}.r{release}\"", layer.id, layer.sequence),
        None => format!("\"{}.{}\"", layer.id, layer.sequence),
    }
}

/// A read's answer marked with its entity tag, or a 304 with no body when the request's
/// `If-None-Match` names it already; pinned to the release its links were resolved against.
/// Layers change, so every answer is revalidated; a private layer's is kept by no shared
/// cache.
pub(crate) fn tagged(
    request: &Request,
    layer: &LayerRow,
    release: Option<u32>,
    answer: impl FnOnce() -> Result<Response, Problem>,
) -> Result<Response, Problem> {
    let etag = etag(layer, release);
    let cache = match layer.visibility {
        Visibility::Public => "no-cache",
        Visibility::Private => "private, no-cache",
    };
    let response = if matches(request.header("if-none-match"), &etag) {
        Response::empty(304)
    } else {
        answer()?
    };
    let response = response
        .with_header("etag", &etag)
        .with_header("cache-control", cache)
        .with_header("vary", "Authorization");
    Ok(match release {
        Some(release) => crate::releases::pinned(response, release),
        None => response,
    })
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

/// An internal error saying `what` did not read or write, with the cause.
pub(crate) fn broken(what: &str, error: impl std::fmt::Display) -> Problem {
    Problem::new(ProblemType::InternalError, format!("{what}: {error}."))
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
