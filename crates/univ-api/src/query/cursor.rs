//! A volume query's cursor (query-a-bounded-volume): what the next page reads and from where.
//! It records the query it pages, by a digest of its body; the release it reads; the change
//! sequence of each layer whose items it reads; the last result served; and how many were.
//!
//! A later page reads the layers again, and refuses with 409 when any recorded layer changed,
//! is no longer read, or a layer is now read that was not: every change to a layer advances its
//! sequence, so no page can skip or repeat a result because a layer changed between pages. The
//! cursor grows by some thirty bytes for each layer it records.

use std::collections::BTreeMap;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::Query;
use crate::backend::{Backend, rows_as};
use crate::problem::{Problem, ProblemType};

/// The cursor's format.
const VERSION: u8 = 1;

/// Where a page starts: after this result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "part")]
pub(super) enum After {
    Object { id: u64 },
    Structure { id: String },
    Item { layer: String, item: String },
}

impl After {
    /// The part of the results it lies in: objects, then structures, then items.
    pub(super) fn part(&self) -> usize {
        match self {
            Self::Object { .. } => 0,
            Self::Structure { .. } => 1,
            Self::Item { .. } => 2,
        }
    }
}

/// A cursor as the client holds it.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct Cursor {
    v: u8,
    pub query: String,
    pub release: u32,
    pub layers: BTreeMap<String, u64>,
    pub after: After,
    pub served: usize,
}

impl Cursor {
    pub(super) fn new(
        query: &str,
        release: u32,
        layers: BTreeMap<String, u64>,
        after: After,
        served: usize,
    ) -> Self {
        Self {
            v: VERSION,
            query: query.to_owned(),
            release,
            layers,
            after,
            served,
        }
    }

    /// The cursor as text: base64url of its JSON.
    pub(super) fn encode(&self) -> String {
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(self).unwrap_or_default())
    }

    /// The cursor `text` of the query whose digest is `query`.
    ///
    /// # Errors
    ///
    /// A 400 at `/cursor` for text no page gave, or a cursor of another query.
    pub(super) fn decode(text: &str, query: &str) -> Result<Self, Problem> {
        let cursor = URL_SAFE_NO_PAD
            .decode(text)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Self>(&bytes).ok())
            .filter(|cursor| cursor.v == VERSION)
            .ok_or_else(|| refused("is not a cursor a page of this server gave"))?;
        if cursor.query != query {
            return Err(refused(
                "is a cursor of another query: send the query whose page gave it",
            ));
        }
        Ok(cursor)
    }
}

fn refused(reason: &str) -> Problem {
    Problem::new(
        ProblemType::QueryInvalid,
        "The query failed the checks its errors name.",
    )
    .with(
        "errors",
        vec![json!({"pointer": "/cursor", "reason": reason})],
    )
}

/// A layer's change sequence.
#[derive(Deserialize)]
struct Sequence {
    id: String,
    sequence: u64,
}

/// A layer as read for its sequence.
#[derive(Deserialize)]
struct LayerRow {
    id: String,
    sequence: u64,
    retired_at: Option<String>,
    readable: u8,
}

/// The layers `query` reads items of now, with their sequences: those the caller may read,
/// not retired, holding current items, and within its layer filter.
pub(super) async fn sequences<B: Backend>(
    backend: &B,
    query: &Query,
) -> Result<BTreeMap<String, u64>, Problem> {
    let rows = backend
        .layers(
            "SELECT id, sequence FROM layers WHERE retired_at IS NULL AND item_count > 0 \
             AND (visibility = 'public' OR key_id = ?1) \
             AND (?2 IS NULL OR id IN (SELECT value FROM json_each(?2))) ORDER BY id",
            &[json!(query.key), json!(query.filters.layers)],
        )
        .await?;
    Ok(rows_as::<Sequence>(rows)?
        .into_iter()
        .map(|row| (row.id, row.sequence))
        .collect())
}

/// Whether the layers read now are the ones `recorded` and unchanged.
///
/// # Errors
///
/// A 409 naming each layer that changed: its sequence now, or that it is retired or no longer
/// readable.
pub(super) async fn unchanged<B: Backend>(
    backend: &B,
    query: &Query,
    recorded: &BTreeMap<String, u64>,
    now: &BTreeMap<String, u64>,
) -> Result<(), Problem> {
    if recorded == now {
        return Ok(());
    }
    let changed: Vec<&String> = recorded
        .keys()
        .chain(now.keys())
        .filter(|id| recorded.get(*id) != now.get(*id))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let rows = backend
        .layers(
            "SELECT id, sequence, retired_at, \
             (visibility = 'public' OR key_id IS ?1) AS readable FROM layers \
             WHERE id IN (SELECT value FROM json_each(?2)) ORDER BY id",
            &[json!(query.key), json!(changed)],
        )
        .await?;
    let layers: Vec<Value> = rows_as::<LayerRow>(rows)?
        .into_iter()
        .map(|row| match (row.retired_at, row.readable) {
            (Some(at), 1) => json!({"layer": row.id, "sequence": row.sequence, "retired_at": at}),
            (None, 1) => json!({"layer": row.id, "sequence": row.sequence}),
            _ => json!({"layer": row.id, "readable": false}),
        })
        .collect();
    Err(Problem::new(
        ProblemType::QueryLayersChanged,
        "A layer whose items the query reads changed since its first page: query again from the first page.",
    )
    .with("layers", layers))
}
