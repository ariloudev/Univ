//! What an object id, a designation or a common name leads to in the release a read is pinned
//! to (look-up-an-object).
//!
//! An id leads to its object when the release holds it. Otherwise the id ledger says what
//! became of it, reading only what releases up to the pinned one recorded: an id merged or
//! split is followed through every later change the ledger holds up to that release, to the
//! objects the release holds. A read of an older release still finds the object as it was
//! there, because objects are rows of their release.

use std::collections::{HashSet, VecDeque};

use serde_json::json;
use univ_model::api::lookup::LedgerChange;
use univ_model::release::rows::name_key;
use univ_model::valid_in;

use super::vocabulary::vocabulary;
use crate::backend::{Backend, column, first};
use crate::problem::{Problem, ProblemType};
use crate::shards::{shard_map, shard_of};

/// The most ids the ledger is followed through from one id. Real chains are one or two
/// changes long; a ledger that leads further is faulty, and is reported as such.
const MAX_VISITED: usize = 1_024;

/// What an id that a release does not hold leads to in it, with each ledger change followed
/// from it in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolved {
    /// The id leads to the one object `into`.
    Merged {
        into: u64,
        history: Vec<LedgerChange>,
    },
    /// The id leads to the objects `into`.
    Split {
        into: Vec<u64>,
        history: Vec<LedgerChange>,
    },
    /// The id leads to no object: it, or each object it led to, was retired.
    Retired { history: Vec<LedgerChange> },
    /// No release up to this one changed the id.
    Unknown,
}

impl Resolved {
    /// The release that replaced the id looked up: the first change followed.
    #[must_use]
    pub fn replaced_in(history: &[LedgerChange]) -> u32 {
        history.first().map_or(0, |change| change.release)
    }
}

/// Whether `release` holds object `id`: whether the catalogue's index names its shard.
pub async fn present<B: Backend>(backend: &B, release: u32, id: u64) -> Result<bool, Problem> {
    Ok(shard_of(backend, release, id).await?.is_some())
}

/// The ledger's change of `id` recorded by a release up to `release`.
async fn change<B: Backend>(
    backend: &B,
    release: u32,
    id: u64,
) -> Result<Option<LedgerChange>, Problem> {
    let params = [json!(id), json!(release)];
    let mut rows = backend
        .releases_batch(&[
            (
                "SELECT release, change FROM ids_replaced WHERE id = ?1 AND release <= ?2",
                &params,
            ),
            (
                "SELECT replaced_by FROM id_replacements WHERE id = ?1 AND release <= ?2 \
                 ORDER BY n",
                &params,
            ),
        ])
        .await?
        .into_iter();
    let (changes, by) = (
        rows.next().unwrap_or_default(),
        rows.next().unwrap_or_default(),
    );
    first(changes)
        .map(|row| {
            Ok(LedgerChange {
                id,
                release: column(&row, "release")?,
                change: column(&row, "change")?,
                replaced_by: by
                    .iter()
                    .map(|row| column(row, "replaced_by"))
                    .collect::<Result<_, _>>()?,
            })
        })
        .transpose()
}

/// What `id`, absent from `release`, leads to there: every change the ledger records up to
/// that release is followed, breadth first, to the objects the release holds.
///
/// # Errors
///
/// A problem answer when the release database cannot be read, or an internal error when the
/// ledger leads through more ids than any real chain does.
pub async fn replaced<B: Backend>(backend: &B, release: u32, id: u64) -> Result<Resolved, Problem> {
    let mut history = Vec::new();
    let mut found: Vec<u64> = Vec::new();
    let mut visited = HashSet::from([id]);
    let mut pending = VecDeque::from([id]);
    while let Some(next) = pending.pop_front() {
        if next != id && present(backend, release, next).await? {
            found.push(next);
            continue;
        }
        let Some(change) = change(backend, release, next).await? else {
            continue;
        };
        for &by in &change.replaced_by {
            if visited.insert(by) {
                pending.push_back(by);
            }
        }
        history.push(change);
        if visited.len() > MAX_VISITED {
            return Err(Problem::new(
                ProblemType::InternalError,
                format!("The id ledger leads from {id} through more than {MAX_VISITED} ids."),
            ));
        }
    }
    found.sort_unstable();
    Ok(match (found.as_slice(), history.is_empty()) {
        (_, true) => Resolved::Unknown,
        ([], false) => Resolved::Retired { history },
        ([into], false) => Resolved::Merged {
            into: *into,
            history,
        },
        _ => Resolved::Split {
            into: found,
            history,
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
            concat!(
                "SELECT DISTINCT object_id, name FROM names WHERE key = ?2 AND ",
                valid_in!("", "?1"),
                " ORDER BY object_id, name"
            ),
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

/// The objects of `release` carrying the designation `identifier` of source `source`, by id,
/// each with the shard it is in.
pub async fn by_designation<B: Backend>(
    backend: &B,
    release: u32,
    source: &str,
    identifier: &str,
) -> Result<Vec<(u64, u32)>, Problem> {
    by_designations(
        backend,
        release,
        &[(source.to_owned(), identifier.to_owned())],
    )
    .await
}

/// The objects of `release` carrying any of `designations`, each a source and an identifier, by
/// id, each once with the shard it is in: every object shard of the release is read at once, in
/// one statement each, since a designation says nothing of where its object is.
pub async fn by_designations<B: Backend>(
    backend: &B,
    release: u32,
    designations: &[(String, String)],
) -> Result<Vec<(u64, u32)>, Problem> {
    let vocabulary = vocabulary(backend, release).await?;
    let mut params = vec![json!(release)];
    let mut any = Vec::new();
    for (source, identifier) in designations {
        if let Some(n) = vocabulary.source_n(source) {
            any.push(format!(
                "(source = ?{} AND identifier = ?{})",
                params.len() + 1,
                params.len() + 2
            ));
            params.extend([json!(n), json!(identifier)]);
        }
    }
    if any.is_empty() {
        return Ok(Vec::new());
    }
    let map = shard_map(backend, release).await?;
    let sql = format!(
        "SELECT DISTINCT object_id FROM designations WHERE ({}) AND {}",
        any.join(" OR "),
        valid_in!("", "?1")
    );
    let statements: Vec<(u32, &str, &[serde_json::Value])> = map
        .shards
        .iter()
        .map(|&shard| (shard, sql.as_str(), &params[..]))
        .collect();
    let mut found = Vec::new();
    for (&shard, rows) in map.shards.iter().zip(backend.shards(&statements).await?) {
        for row in &rows {
            found.push((column(row, "object_id")?, shard));
        }
    }
    found.sort_unstable();
    Ok(found)
}
