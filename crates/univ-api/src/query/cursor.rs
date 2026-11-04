//! A volume query's cursor (query-a-bounded-volume): what the next page reads and from where.
//! It records the query it pages, by a digest of its body; the release it reads; the change
//! sequence of each layer whose items it reads; the last result served; and how many were.
//!
//! A cursor is signed, an HMAC-SHA256 under a key drawn from the owner secret, so that a
//! client cannot move it past the cap of results or name layers in it. A later page reads the
//! layers again, and refuses with 409 when any recorded layer changed, is no longer read, or a
//! layer is now read that was not: every change to a layer advances its sequence, so no page
//! can skip or repeat a result because a layer changed between pages. The cursor grows by some
//! thirty bytes for each layer it records.

use std::collections::{BTreeMap, BTreeSet};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use super::Query;
use super::request::invalid;
use crate::backend::{Backend, rows_as};
use crate::layers::readable_by;
use crate::layers::validate::Failures;
use crate::problem::{Problem, ProblemType};

/// The cursor's format.
const VERSION: u8 = 1;

/// The bytes of the signature a cursor carries.
const SIGNATURE: usize = 16;

/// Where a page starts: after this result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "part")]
pub(super) enum After {
    Object { id: u64 },
    Structure { id: String },
    Item { layer: String, item: String },
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

/// The key cursors are signed under, drawn from the owner secret.
pub(super) struct Signing([u8; 32]);

impl Signing {
    /// The key of `backend`'s owner secret, drawn where a page reads or gives a cursor.
    ///
    /// # Errors
    ///
    /// A 500 where the Worker has no owner secret: a key anyone could draw would let a client
    /// forge cursors.
    pub(super) fn of<B: Backend>(backend: &B) -> Result<Self, Problem> {
        let secret = backend.owner_secret().ok_or_else(|| {
            Problem::new(
                ProblemType::InternalError,
                "The Worker has no owner secret to sign a query's cursors with.",
            )
        })?;
        let mut hash = Sha256::new();
        hash.update(b"univ volume query cursor v1\0");
        hash.update(secret);
        Ok(Self(hash.finalize().into()))
    }

    /// HMAC-SHA256 of `message` (RFC 2104), its first [`SIGNATURE`] bytes.
    fn sign(&self, message: &[u8]) -> [u8; SIGNATURE] {
        let pad = |byte: u8| -> [u8; 64] {
            std::array::from_fn(|i| self.0.get(i).copied().unwrap_or(0) ^ byte)
        };
        let inner = Sha256::new()
            .chain_update(pad(0x36))
            .chain_update(message)
            .finalize();
        let outer = Sha256::new()
            .chain_update(pad(0x5c))
            .chain_update(inner)
            .finalize();
        std::array::from_fn(|i| outer[i])
    }
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

    /// The cursor as text: base64url of its JSON, a dot, and its signature.
    pub(super) fn encode(&self, signing: &Signing) -> String {
        let json = serde_json::to_vec(self).unwrap_or_default();
        format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(&json),
            URL_SAFE_NO_PAD.encode(signing.sign(&json))
        )
    }

    /// The cursor `text` of the query whose digest is `query`.
    ///
    /// # Errors
    ///
    /// A 400 at `/cursor` for text no page gave, or a cursor of another query.
    pub(super) fn decode(text: &str, query: &str, signing: &Signing) -> Result<Self, Problem> {
        let cursor = text
            .split_once('.')
            .and_then(|(json, signature)| {
                let json = URL_SAFE_NO_PAD.decode(json).ok()?;
                let signature = URL_SAFE_NO_PAD.decode(signature).ok()?;
                bool::from(signing.sign(&json).ct_eq(signature.as_slice())).then_some(json)
            })
            .and_then(|json| serde_json::from_slice::<Self>(&json).ok())
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
    let mut failures = Failures::default();
    failures.add("/cursor", reason);
    invalid(failures)
        .err()
        .unwrap_or_else(|| Problem::new(ProblemType::QueryInvalid, reason))
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
            &format!(
                "SELECT id, sequence FROM layers WHERE retired_at IS NULL AND item_count > 0 \
                 AND {} AND (?2 IS NULL OR id IN (SELECT value FROM json_each(?2))) ORDER BY id",
                readable_by("", "?1")
            ),
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
/// A 409 naming each layer that changed: its sequence now, its retirement, or, alike for a
/// layer the caller may no longer read and one that does not exist, that it is not readable.
pub(super) async fn unchanged<B: Backend>(
    backend: &B,
    query: &Query,
    recorded: &BTreeMap<String, u64>,
    now: &BTreeMap<String, u64>,
) -> Result<(), Problem> {
    if recorded == now {
        return Ok(());
    }
    let changed: BTreeSet<&String> = recorded
        .keys()
        .chain(now.keys())
        .filter(|id| recorded.get(*id) != now.get(*id))
        .collect();
    let rows = backend
        .layers(
            &format!(
                "SELECT id, sequence, retired_at, {} AS readable FROM layers \
                 WHERE id IN (SELECT value FROM json_each(?2)) ORDER BY id",
                readable_by("", "?1")
            ),
            &[json!(query.key), json!(changed)],
        )
        .await?;
    let read: BTreeMap<String, LayerRow> = rows_as::<LayerRow>(rows)?
        .into_iter()
        .filter(|row| row.readable == 1)
        .map(|row| (row.id.clone(), row))
        .collect();
    let layers: Vec<Value> = changed
        .into_iter()
        .map(|id| match read.get(id) {
            Some(LayerRow {
                retired_at: Some(at),
                sequence,
                ..
            }) => {
                json!({"layer": id, "sequence": sequence, "retired_at": at})
            }
            Some(row) => json!({"layer": id, "sequence": row.sequence}),
            None => json!({"layer": id, "readable": false}),
        })
        .collect();
    Err(Problem::new(
        ProblemType::QueryLayersChanged,
        "A layer whose items the query reads changed since its first page: query again from the first page.",
    )
    .with("layers", layers))
}
