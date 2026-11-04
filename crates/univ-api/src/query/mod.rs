//! `POST /v1/query/volume` (query-a-bounded-volume, filter-by-time): the objects, structures and
//! layer items in a volume of space, in stable pages of [`RESULTS_PER_PAGE`] to at most
//! [`MAX_RESULTS`], with the tile nodes that cover it on the first page.
//!
//! Results come in one total order, objects by id, then structures by id, then items by layer
//! and item, so that paging a query whose layers do not change gives the same pages. Each
//! filter given keeps only the results that carry one of its values: kinds and scale domains
//! narrow objects and structures, sources objects, and layers and time items, so that
//! a filter of one leaves out the parts that carry none.

mod cursor;
mod items;
mod objects;
mod request;
mod structures;

use std::sync::Arc;

use serde_json::json;
use univ_model::api::query::{Filters, MAX_RESULTS, QueryResult, RESULTS_PER_PAGE, VolumePage};
use univ_model::release::read::Vocabulary;
use univ_model::time::TimeKind;
use univ_model::units::Unit;
use univ_model::volume::Region;

use crate::access::Caller;
use crate::backend::{Backend, column};
use crate::bulk::domains_of;
use crate::http::{Request, Response};
use crate::layers::validate::{Failures, pointer};
use crate::objects::vocabulary::vocabulary;
use crate::problem::{Problem, ProblemType};
use crate::releases::{RELEASE_PARAM, pinned, published, resolve};
use cursor::{After, Cursor};

/// A volume query, read and resolved against the release it reads.
pub(crate) struct Query {
    release: u32,
    /// The volume's region in the ICRS, in megaparsecs.
    region: Region,
    filters: Filters,
    /// The terms of the kinds filtered by, and the sources' numbers.
    kind_ids: Option<Vec<i64>>,
    source_ns: Option<Vec<i64>>,
    time: Option<(TimeKind, f64, f64)>,
    /// The caller's key, whose private layers it reads.
    key: Option<String>,
    vocabulary: Arc<Vocabulary>,
}

impl Query {
    fn reads_objects(&self) -> bool {
        self.filters.layers.is_none() && self.time.is_none()
    }

    fn reads_structures(&self) -> bool {
        self.reads_objects() && self.filters.sources.is_none()
    }

    fn reads_items(&self) -> bool {
        self.filters.kinds.is_none()
            && self.filters.sources.is_none()
            && self.filters.scale_domains.is_none()
    }
}

/// Answers a page of a bounded volume query.
///
/// # Errors
///
/// A problem answer: a malformed query (400, each member by JSON pointer), a cursor of another
/// query or a release other than the cursor's (400), a layer read that changed since the first
/// page (409), a release never published (404) or pruned (410), or the database failing.
#[utoipa::path(
    post,
    path = "/v1/query/volume",
    params(("release" = Option<u32>, Query, description = "The release read; latest when absent. A cursor's release is the one it read.")),
    request_body = univ_model::api::query::VolumeQuery,
    security((), ("api_key" = [])),
    responses((status = 200, description = "A page of the results", body = VolumePage))
)]
pub async fn volume<B: Backend>(
    request: &Request,
    caller: &Caller,
    backend: &B,
) -> Result<Response, Problem> {
    let body = request::read(request)?;
    let cursor = match &body.cursor {
        Some(text) => Some(Cursor::decode(text, &body.fingerprint)?),
        None => None,
    };
    let release = match &cursor {
        Some(cursor) => {
            if request
                .param(RELEASE_PARAM)
                .is_some_and(|r| r != cursor.release.to_string())
            {
                return Err(Problem::invalid_parameter(
                    RELEASE_PARAM,
                    &request.param(RELEASE_PARAM).unwrap_or_default(),
                    format!(
                        "The cursor reads release {}; a later page reads it too.",
                        cursor.release
                    ),
                ));
            }
            published(cursor.release, backend).await?
        }
        None => resolve(request, backend).await?,
    };
    let key = match caller {
        Caller::Key { id, .. } => Some(id.clone()),
        Caller::Anonymous | Caller::Owner => None,
    };
    let query = resolved(
        backend,
        release,
        key,
        body.volume.region_in(Unit::Megaparsec),
        body.filters,
        body.time,
    )
    .await?;
    let layers = if query.reads_items() {
        cursor::sequences(backend, &query).await?
    } else {
        std::collections::BTreeMap::new()
    };
    if let Some(cursor) = &cursor {
        cursor::unchanged(backend, &query, &cursor.layers, &layers).await?;
    }
    let served_before = cursor.as_ref().map_or(0, |c| c.served);
    let room = RESULTS_PER_PAGE.min(MAX_RESULTS.saturating_sub(served_before));
    let mut found = collect(backend, &query, cursor.map(|c| c.after), room + 1).await?;
    let more = found.len() > room;
    found.truncate(room);
    let served = served_before + found.len();
    let next = match found.last() {
        Some((after, _)) if more && served < MAX_RESULTS => {
            Some(Cursor::new(&body.fingerprint, release, layers, after.clone(), served).encode())
        }
        _ => None,
    };
    let page = VolumePage {
        release,
        results: found.into_iter().map(|(_, result)| result).collect(),
        served,
        next,
        truncated: more && served >= MAX_RESULTS,
        tiles: None,
    };
    let _ = body.max_level;
    Ok(pinned(Response::json(&page)?, release))
}

/// Up to `want` results after `after`, across the parts the query reads, in order.
async fn collect<B: Backend>(
    backend: &B,
    query: &Query,
    after: Option<After>,
    want: usize,
) -> Result<Vec<(After, QueryResult)>, Problem> {
    let start = after.as_ref().map_or(0, After::part);
    let mut found = Vec::new();
    for part in start..3 {
        let need = want - found.len();
        if need == 0 {
            break;
        }
        let from = after.as_ref().filter(|a| a.part() == part);
        let more = match (part, from) {
            (0, from) if query.reads_objects() => {
                let id = match from {
                    Some(After::Object { id }) => Some(*id),
                    _ => None,
                };
                objects::page(backend, query, id, need).await?
            }
            (1, from) if query.reads_structures() => {
                let id = match from {
                    Some(After::Structure { id }) => Some(id.as_str()),
                    _ => None,
                };
                structures::page(backend, query, id, need).await?
            }
            (2, from) if query.reads_items() => {
                let at = match from {
                    Some(After::Item { layer, item }) => Some((layer.as_str(), item.as_str())),
                    _ => None,
                };
                items::page(backend, query, at, need).await?
            }
            _ => Vec::new(),
        };
        found.extend(more);
    }
    Ok(found)
}

/// The query's filters resolved against `release` and the layers the caller may read.
///
/// # Errors
///
/// A 400 naming each value no kind, source, scale domain or readable layer has.
async fn resolved<B: Backend>(
    backend: &B,
    release: u32,
    key: Option<String>,
    region: Option<Region>,
    filters: Filters,
    time: Option<(TimeKind, f64, f64)>,
) -> Result<Query, Problem> {
    let vocabulary = vocabulary(backend, release).await?;
    let mut failures = Failures::default();
    let mut known =
        |name: &str, values: &Option<Vec<String>>, known: &dyn Fn(&str) -> bool, what: String| {
            for (i, value) in values.iter().flatten().enumerate() {
                if !known(value) {
                    failures.add(
                        pointer(&format!("/filters/{name}"), &i.to_string()),
                        what.clone(),
                    );
                }
            }
        };
    let structure_kinds = structure_kinds(backend, release, &filters).await?;
    known(
        "kinds",
        &filters.kinds,
        &|kind| vocabulary.term_id(kind).is_some() || structure_kinds.iter().any(|k| k == kind),
        format!("is a kind of object or structure of release {release}"),
    );
    known(
        "sources",
        &filters.sources,
        &|source| vocabulary.source_n(source).is_some(),
        format!("is a source of release {release}"),
    );
    let domains = domains_of(backend, release).await?;
    known(
        "scale_domains",
        &filters.scale_domains,
        &|name| domains.iter().any(|d| d.name == name),
        format!("is a scale domain of release {release}"),
    );
    let readable = readable_layers(backend, key.as_deref(), &filters).await?;
    known(
        "layers",
        &filters.layers,
        &|layer| readable.iter().any(|l| l == layer),
        "is a layer, not retired, that the caller may read".to_owned(),
    );
    request::invalid(failures)?;
    let Some(region) = region else {
        return Err(Problem::new(
            ProblemType::QueryInvalid,
            "The volume did not resolve.",
        ));
    };
    Ok(Query {
        release,
        region,
        kind_ids: filters
            .kinds
            .as_ref()
            .map(|k| k.iter().filter_map(|t| vocabulary.term_id(t)).collect()),
        source_ns: filters
            .sources
            .as_ref()
            .map(|s| s.iter().filter_map(|t| vocabulary.source_n(t)).collect()),
        filters,
        time,
        key,
        vocabulary,
    })
}

/// The kinds of structure of `release` among those `filters` names.
async fn structure_kinds<B: Backend>(
    backend: &B,
    release: u32,
    filters: &Filters,
) -> Result<Vec<String>, Problem> {
    if filters.kinds.is_none() {
        return Ok(Vec::new());
    }
    let rows = backend
        .releases(
            "SELECT DISTINCT kind FROM structures WHERE release = ?1 \
             AND kind IN (SELECT value FROM json_each(?2))",
            &[json!(release), json!(filters.kinds)],
        )
        .await?;
    rows.iter()
        .map(|row| column::<String>(row, "kind"))
        .collect()
}

/// The layers among those `filters` names that the caller may read and that are not retired.
async fn readable_layers<B: Backend>(
    backend: &B,
    key: Option<&str>,
    filters: &Filters,
) -> Result<Vec<String>, Problem> {
    if filters.layers.is_none() {
        return Ok(Vec::new());
    }
    let rows = backend
        .layers(
            "SELECT id FROM layers WHERE retired_at IS NULL \
             AND (visibility = 'public' OR key_id = ?1) \
             AND id IN (SELECT value FROM json_each(?2))",
            &[json!(key), json!(filters.layers)],
        )
        .await?;
    rows.iter().map(|row| column::<String>(row, "id")).collect()
}
