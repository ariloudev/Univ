//! Links resolved against the release a read is answered from (publish-a-layer-item): a link
//! keeps the id as written, and where a release up to the one read merged or split that
//! object, it is served with the ids that replaced it and the release that did.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use univ_model::api::layers::ItemVersion;

use crate::backend::{Backend, column};
use crate::http::Request;
use crate::objects::resolve::{Resolved, replaced};
use crate::problem::Problem;
use crate::releases;

/// Marks each object link of `versions` that the release read replaced: `replaced_by` lists
/// the objects it leads to (none for a retired id) and `replaced_in` the release that changed
/// it. One query finds the ids the ledger changed; only those are followed.
pub(crate) async fn resolve<B: Backend>(
    request: &Request,
    backend: &B,
    versions: &mut [ItemVersion],
) -> Result<(), Problem> {
    let linked: Vec<u64> = links(versions)
        .filter_map(|link| link.get("object").and_then(Value::as_u64))
        .collect();
    if linked.is_empty() {
        return Ok(());
    }
    let release = releases::resolve(request, backend).await?;
    let rows = backend
        .releases(
            "SELECT DISTINCT id FROM ids_replaced WHERE release <= ?1 \
             AND id IN (SELECT value FROM json_each(?2))",
            &[json!(release), json!(linked)],
        )
        .await?;
    let mut changed = BTreeMap::new();
    for row in rows {
        let id: u64 = column(&row, "id")?;
        let (into, history) = match replaced(backend, release, id).await? {
            Resolved::Merged { into, history } => (vec![into], history),
            Resolved::Split { into, history } => (into, history),
            Resolved::Retired { history } => (Vec::new(), history),
            Resolved::Unknown => continue,
        };
        changed.insert(id, (into, Resolved::replaced_in(&history)));
    }
    for version in versions.iter_mut() {
        let Some(Value::Array(links)) = version
            .body
            .content
            .as_mut()
            .and_then(|content| content.get_mut("links"))
        else {
            continue;
        };
        for link in links.iter_mut().filter_map(Value::as_object_mut) {
            let id = link.get("object").and_then(Value::as_u64);
            if let Some((into, by)) = id.and_then(|id| changed.get(&id)) {
                link.insert("replaced_by".to_owned(), json!(into));
                link.insert("replaced_in".to_owned(), json!(by));
            }
        }
    }
    Ok(())
}

/// Every link of `versions`.
fn links(versions: &[ItemVersion]) -> impl Iterator<Item = &Value> {
    versions
        .iter()
        .filter_map(|version| version.body.content.as_ref()?.get("links")?.as_array())
        .flatten()
}
