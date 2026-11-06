//! Links resolved against the release a read is answered from (publish-a-layer-item): a link
//! keeps the id as written, and where a release up to the one read merged, split or retired
//! that object, it is served with the ids that replaced it and the release that did.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};
use univ_model::api::layers::ItemVersion;
use univ_model::release::valid_in;

use crate::backend::{Backend, column, json_column};
use crate::http::Request;
use crate::problem::{Problem, ProblemType};
use crate::releases;

/// The most levels of the id ledger followed: a real chain of merges is a few long.
const MAX_LEVELS: usize = 32;

/// Marks each object link of `versions` that the release read replaced: `replaced_by` lists
/// the objects it leads to (none for a retired id) and `replaced_in` the release that first
/// changed it. Gives the release read when there was a link to resolve, for the answer's tag;
/// none otherwise, since the answer then depends on no release.
///
/// The ledger is followed for every linked id at once, a level at a time: one batch of two
/// queries a level, and a single query when no linked id ever changed.
pub(crate) async fn resolve<B: Backend>(
    request: &Request,
    backend: &B,
    versions: &mut [ItemVersion],
) -> Result<Option<u32>, Problem> {
    let linked: BTreeSet<u64> = versions
        .iter()
        .filter_map(|version| version.body.content.as_ref()?.get("links")?.as_array())
        .flatten()
        .filter_map(|link| link.get("object").and_then(Value::as_u64))
        .collect();
    if linked.is_empty() {
        return Ok(None);
    }
    let release = releases::resolve(request, backend).await?;
    let changed = replaced(backend, release, &linked).await?;
    for version in versions.iter_mut() {
        let links = version
            .body
            .content
            .as_mut()
            .and_then(|content| content.get_mut("links"))
            .and_then(Value::as_array_mut);
        for link in links.into_iter().flatten().filter_map(Value::as_object_mut) {
            let id = link.get("object").and_then(Value::as_u64);
            if let Some((into, by)) = id.and_then(|id| changed.get(&id)) {
                link.insert("replaced_by".to_owned(), json!(into));
                link.insert("replaced_in".to_owned(), json!(by));
            }
        }
    }
    Ok(Some(release))
}

/// For each of `ids` that a release up to `release` changed: the objects of `release` it
/// leads to, and the release of its first change. The ledger's changes reachable from `ids`
/// are read first, a level at a time; then each id is followed through them.
async fn replaced<B: Backend>(
    backend: &B,
    release: u32,
    ids: &BTreeSet<u64>,
) -> Result<BTreeMap<u64, (Vec<u64>, u32)>, Problem> {
    let mut changes: BTreeMap<u64, (u32, Vec<u64>)> = BTreeMap::new();
    let mut present: BTreeSet<u64> = BTreeSet::new();
    let mut seen: BTreeSet<u64> = ids.clone();
    let mut frontier: Vec<u64> = ids.iter().copied().collect();
    let mut levels = 0;
    while !frontier.is_empty() {
        levels += 1;
        if levels > MAX_LEVELS {
            return Err(Problem::new(
                ProblemType::InternalError,
                format!("The id ledger leads through more than {MAX_LEVELS} levels."),
            ));
        }
        let held = json!(frontier);
        let mut rows = backend
            .releases_batch(&[
                (
                    &format!(
                        "SELECT id FROM object_shards \
                         WHERE id IN (SELECT value FROM json_each(?2)) AND {}",
                        valid_in("", "?1")
                    ),
                    &[json!(release), held.clone()],
                ),
                (
                    "SELECT id, release, replaced_by FROM ids_replaced WHERE release <= ?1 \
                     AND id IN (SELECT value FROM json_each(?2))",
                    &[json!(release), held],
                ),
            ])
            .await?
            .into_iter();
        for row in rows.next().unwrap_or_default() {
            present.insert(column(&row, "id")?);
        }
        frontier = Vec::new();
        for row in rows.next().unwrap_or_default() {
            let id: u64 = column(&row, "id")?;
            let into: Vec<u64> = json_column(&row, "replaced_by")?;
            frontier.extend(into.iter().copied().filter(|to| seen.insert(*to)));
            changes.insert(id, (column(&row, "release")?, into));
        }
    }
    Ok(ids
        .iter()
        .filter_map(|id| {
            let (by, _) = changes.get(id)?;
            Some((*id, (reach(*id, &changes, &present), *by)))
        })
        .collect())
}

/// The objects `release` holds that `id` leads to through `changes`, breadth first.
fn reach(id: u64, changes: &BTreeMap<u64, (u32, Vec<u64>)>, present: &BTreeSet<u64>) -> Vec<u64> {
    let mut found = BTreeSet::new();
    let mut visited = BTreeSet::from([id]);
    let mut pending = vec![id];
    while let Some(next) = pending.pop() {
        if next != id && present.contains(&next) {
            found.insert(next);
            continue;
        }
        for to in changes
            .get(&next)
            .map(|(_, into)| into.as_slice())
            .unwrap_or_default()
        {
            if visited.insert(*to) {
                pending.push(*to);
            }
        }
    }
    found.into_iter().collect()
}
