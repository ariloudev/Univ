//! What an object id, a designation or a common name leads to in the release a read is pinned
//! to (look-up-an-object).
//!
//! An id leads to its object when the release holds it. Otherwise the id ledger says what
//! became of it, reading only what releases up to the pinned one recorded: an id merged or
//! split is followed through every later change the ledger holds up to that release, to the
//! objects the release holds. A read of an older release still finds the object as it was
//! there, because objects are rows of their release.

use serde_json::json;
use univ_model::release::rows::name_key;

use crate::backend::{Backend, column, first, json_column};
use crate::problem::Problem;

/// The most changes an id is followed through; the ledger never chains further in practice.
const MAX_HOPS: usize = 32;

/// What an id that a release does not hold leads to in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolved {
    /// The id was merged, in release `replaced_in`, into the one object `into`.
    Merged { into: u64, replaced_in: u32 },
    /// The id was split, in release `replaced_in`, into the objects `into`.
    Split { into: Vec<u64>, replaced_in: u32 },
    /// The id was retired in release `replaced_in`, or what it led to was.
    Retired { replaced_in: u32 },
    /// No release up to this one issued the id.
    Unknown,
}

/// Whether `release` holds object `id`.
pub async fn present<B: Backend>(backend: &B, release: u32, id: u64) -> Result<bool, Problem> {
    let rows = backend
        .releases(
            "SELECT 1 AS present FROM objects WHERE release = ?1 AND id = ?2",
            &[json!(release), json!(id)],
        )
        .await?;
    Ok(!rows.is_empty())
}

/// The ledger's change of `id` recorded by a release up to `release`: the release, the change
/// and the ids that replaced it.
async fn change<B: Backend>(
    backend: &B,
    release: u32,
    id: u64,
) -> Result<Option<(u32, String, Vec<u64>)>, Problem> {
    let rows = backend
        .releases(
            "SELECT release, change, replaced_by FROM ids_replaced \
             WHERE id = ?1 AND release <= ?2",
            &[json!(id), json!(release)],
        )
        .await?;
    first(rows)
        .map(|row| {
            Ok((
                column(&row, "release")?,
                column(&row, "change")?,
                json_column(&row, "replaced_by")?,
            ))
        })
        .transpose()
}

/// What `id`, absent from `release`, leads to there.
pub async fn replaced<B: Backend>(backend: &B, release: u32, id: u64) -> Result<Resolved, Problem> {
    let Some((replaced_in, _, by)) = change(backend, release, id).await? else {
        return Ok(Resolved::Unknown);
    };
    let mut found: Vec<u64> = Vec::new();
    let mut pending = by;
    let mut hops = 0;
    while let Some(next) = pending.pop() {
        hops += 1;
        if hops > MAX_HOPS {
            break;
        }
        if present(backend, release, next).await? {
            found.push(next);
        } else if let Some((_, _, further)) = change(backend, release, next).await? {
            pending.extend(further);
        }
    }
    found.sort_unstable();
    found.dedup();
    Ok(match found.as_slice() {
        [] => Resolved::Retired { replaced_in },
        [into] => Resolved::Merged {
            into: *into,
            replaced_in,
        },
        _ => Resolved::Split {
            into: found,
            replaced_in,
        },
    })
}

/// The objects of `release` carrying the name `name` (case and white space ignored), each
/// with the spellings it carries, by id.
pub async fn by_name<B: Backend>(
    backend: &B,
    release: u32,
    name: &str,
) -> Result<Vec<(u64, Vec<String>)>, Problem> {
    let rows = backend
        .releases(
            "SELECT DISTINCT object_id, name FROM names WHERE release = ?1 AND key = ?2 \
             ORDER BY object_id, name",
            &[json!(release), json!(name_key(name))],
        )
        .await?;
    let mut objects: Vec<(u64, Vec<String>)> = Vec::new();
    for row in &rows {
        let id: u64 = column(row, "object_id")?;
        let spelling: String = column(row, "name")?;
        match objects.last_mut() {
            Some((last, names)) if *last == id => names.push(spelling),
            _ => objects.push((id, vec![spelling])),
        }
    }
    Ok(objects)
}

/// The objects of `release` carrying the designation `identifier` of source `source`, by id.
pub async fn by_designation<B: Backend>(
    backend: &B,
    release: u32,
    source: &str,
    identifier: &str,
) -> Result<Vec<u64>, Problem> {
    let rows = backend
        .releases(
            "SELECT DISTINCT d.object_id FROM designations d JOIN sources s \
             ON s.release = d.release AND s.n = d.source \
             WHERE d.release = ?1 AND s.id = ?2 AND d.identifier = ?3 ORDER BY d.object_id",
            &[json!(release), json!(source), json!(identifier)],
        )
        .await?;
    rows.iter().map(|row| column(row, "object_id")).collect()
}
