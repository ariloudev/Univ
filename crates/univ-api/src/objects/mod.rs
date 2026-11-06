//! Objects (look-up-an-object): read by id, designation or common name.
//!
//! An object is answered 200 with its record. A lookup that leads to more than one object, a
//! shared name or designation or a split id, answers 300 with the objects; a merged id answers
//! 301 to the object it is part of, in the same release. Anything else answers 404 naming what
//! was looked up.

pub(crate) mod resolve;
pub(crate) mod vocabulary;

use serde_json::json;
use univ_model::api::lookup::{
    Ambiguity, LedgerChange, LookedUp, ObjectChoice, ObjectChoices, ObjectMerged,
};
use univ_model::api::objects::ObjectRecord;
use univ_model::release::read;
use univ_model::valid_in;

use self::resolve::{Resolved, by_designation, by_name, replaced};
use self::vocabulary::vocabulary;
use crate::backend::{Backend, Row, first};
use crate::http::{Request, Response};
use crate::problem::{Problem, ProblemType};
use crate::releases::{RELEASE_PARAM, pinned, resolve};
use crate::shards::shard_of;

/// Reads one object of the release the request reads, by its id.
///
/// # Errors
///
/// A problem answer: the release refused, an id that is not a positive whole number (400), an
/// id that leads to no object (404), or the release database unreadable.
#[utoipa::path(
    get,
    path = "/v1/objects/{id}",
    params(
        ("id" = u64, Path, description = "The object's id"),
        ("release" = Option<u32>, Query, description = "The release to read; latest when absent")
    ),
    responses(
        (status = 200, description = "The object", body = ObjectRecord),
        (status = 300, description = "The id was split; the objects that replaced it", body = ObjectChoices),
        (status = 301, description = "The id was merged; the object it is part of", body = ObjectMerged)
    )
)]
pub async fn get<B: Backend>(
    request: &Request,
    id: &str,
    backend: &B,
) -> Result<Response, Problem> {
    let release = resolve(request, backend).await?;
    let id: u64 = id.parse().ok().filter(|id| *id > 0).ok_or_else(|| {
        Problem::invalid_parameter(
            "id",
            id,
            format!("An object id is a positive whole number, not {id:?}."),
        )
    })?;
    if let Some(record) = record(backend, release, id, None).await? {
        return Ok(record);
    }
    let looked_up = LookedUp::Id { id };
    let unpinned = request.param(RELEASE_PARAM).is_none();
    let answer = match replaced(backend, release, id).await? {
        Resolved::Merged { into, history } => {
            let merged = ObjectMerged {
                release,
                id,
                replaced_in: Resolved::replaced_in(&history),
                merged_into: into,
                href: href(into, release),
                history,
            };
            let response =
                Response::json_with_status(301, &merged)?.with_header("location", &merged.href);
            pinned(response, release)
        }
        Resolved::Split { into, history } => choices(
            release,
            looked_up,
            Ambiguity::Split,
            history,
            into.into_iter().map(|id| (id, Vec::new())),
        )?,
        Resolved::Retired { history } => {
            return Err(not_found(release, &looked_up)
                .with("replaced_in", Resolved::replaced_in(&history))
                .with(
                    "history",
                    serde_json::to_value(&history).unwrap_or_default(),
                ));
        }
        Resolved::Unknown => return Err(not_found(release, &looked_up)),
    };
    // Where an unpinned read was sent depends on the release `latest` names now, which moves:
    // a cache must not keep the redirect.
    Ok(if unpinned {
        answer.with_header("cache-control", "no-store")
    } else {
        answer
    })
}

/// Looks an object up by a common name, or by a source and the designation it gives.
///
/// # Errors
///
/// A problem answer: the release refused, neither a name nor a source and designation (400),
/// nothing found (404), or the release database unreadable.
#[utoipa::path(
    get,
    path = "/v1/objects",
    params(
        ("name" = Option<String>, Query, description = "A common name, such as M31; letter case and white space are ignored"),
        ("source" = Option<String>, Query, description = "A source's id, such as gaia-dr3-bright, with `designation`"),
        ("designation" = Option<String>, Query, description = "The identifier the source gives the object, with `source`"),
        ("release" = Option<u32>, Query, description = "The release to read; latest when absent")
    ),
    responses(
        (status = 200, description = "The one object the name or designation leads to", body = ObjectRecord),
        (status = 300, description = "More than one object carries the name or designation", body = ObjectChoices)
    )
)]
pub async fn find<B: Backend>(request: &Request, backend: &B) -> Result<Response, Problem> {
    let release = resolve(request, backend).await?;
    // The shard of each object a designation was found in, which saves asking the index.
    let mut shards = Vec::new();
    let (looked_up, ambiguity, found) = match (
        request.param("name"),
        request.param("source"),
        request.param("designation"),
    ) {
        (Some(name), None, None) => {
            let found = by_name(backend, release, &name).await?;
            (LookedUp::Name { name }, Ambiguity::SharedName, found)
        }
        (None, Some(source), Some(designation)) => {
            let found = by_designation(backend, release, &source, &designation).await?;
            shards = found.iter().map(|&(_, shard)| shard).collect();
            (
                LookedUp::Designation {
                    source,
                    designation,
                },
                Ambiguity::SharedDesignation,
                found.into_iter().map(|(id, _)| (id, Vec::new())).collect(),
            )
        }
        (name, source, designation) => {
            // Name the parameter that is missing, or the one that does not belong.
            let parameter = match (name.is_some(), source.is_some(), designation.is_some()) {
                (true, _, _) => "name",
                (false, true, false) => "designation",
                _ => "source",
            };
            return Err(Problem::new(
                ProblemType::InvalidParameter,
                "Look an object up by `name` alone, or by `source` and `designation`.",
            )
            .with("parameter", parameter));
        }
    };
    match found.as_slice() {
        [] => Err(not_found(release, &looked_up)),
        [(id, _)] => present_after_all(backend, release, *id, shards.first().copied()).await,
        _ => {
            let answer = choices(release, looked_up, ambiguity, Vec::new(), found)?;
            Ok(if request.param(RELEASE_PARAM).is_none() {
                answer.with_header("cache-control", "no-store")
            } else {
                answer
            })
        }
    }
}

/// The record of object `id` of `release`, or none when the release does not hold it: its
/// shard, `known` or from the catalogue's index, then every row of the object in one round trip
/// to it.
async fn record<B: Backend>(
    backend: &B,
    release: u32,
    id: u64,
    known: Option<u32>,
) -> Result<Option<Response>, Problem> {
    let shard = match known {
        Some(shard) => shard,
        None => match shard_of(backend, release, id).await? {
            Some(shard) => shard,
            None => return Ok(None),
        },
    };
    let params = [json!(release), json!(id)];
    let batch = backend
        .shards(&[
            (
                shard,
                concat!(
                    "SELECT * FROM objects WHERE id = ?2 AND ",
                    valid_in!("", "?1")
                ),
                &params,
            ),
            (
                shard,
                concat!(
                    "SELECT * FROM entries WHERE object_id = ?2 AND ",
                    valid_in!("", "?1")
                ),
                &params,
            ),
            (
                shard,
                concat!(
                    "SELECT * FROM measurements WHERE object_id = ?2 AND ",
                    valid_in!("", "?1")
                ),
                &params,
            ),
            (
                shard,
                concat!(
                    "SELECT source, identifier FROM designations WHERE object_id = ?2 AND ",
                    valid_in!("", "?1")
                ),
                &params,
            ),
        ])
        .await?;
    let [objects, entries, measurements, designations]: [Vec<Row>; 4] =
        batch.try_into().map_err(|_| {
            Problem::new(
                ProblemType::InternalError,
                "An object shard answered a batch of four reads with another number.",
            )
        })?;
    let Some(object) = first(objects) else {
        return Err(Problem::new(
            ProblemType::InternalError,
            format!(
                "The index puts object {id} of release {release} in shard {shard}, which does not hold it."
            ),
        ));
    };
    let vocabulary = vocabulary(backend, release).await?;
    let record = read::object(&vocabulary, &object, &entries, &measurements, &designations)
        .map_err(|error| {
            Problem::new(
                ProblemType::InternalError,
                format!("Object {id} of release {release} did not read: {error}."),
            )
        })?;
    Ok(Some(pinned(Response::json(&record)?, release)))
}

/// The record of an object a lookup found, which the release holds, in the shard `known` when
/// the lookup found it there.
async fn present_after_all<B: Backend>(
    backend: &B,
    release: u32,
    id: u64,
    known: Option<u32>,
) -> Result<Response, Problem> {
    record(backend, release, id, known).await?.ok_or_else(|| {
        Problem::new(
            ProblemType::InternalError,
            format!("Release {release} names object {id}, which it does not hold."),
        )
    })
}

/// The 300 answer of a lookup that leads to `objects`.
fn choices(
    release: u32,
    looked_up: LookedUp,
    ambiguity: Ambiguity,
    history: Vec<LedgerChange>,
    objects: impl IntoIterator<Item = (u64, Vec<String>)>,
) -> Result<Response, Problem> {
    let choices = ObjectChoices {
        release,
        looked_up,
        ambiguity,
        replaced_in: history.first().map(|change| change.release),
        history,
        objects: objects
            .into_iter()
            .map(|(id, names)| ObjectChoice {
                id,
                href: href(id, release),
                names,
            })
            .collect(),
    };
    Ok(pinned(Response::json_with_status(300, &choices)?, release))
}

/// The path that reads object `id` in `release`.
fn href(id: u64, release: u32) -> String {
    format!("/v1/objects/{id}?release={release}")
}

/// The 404 of a lookup that leads to no object of `release`, naming what was looked up.
fn not_found(release: u32, looked_up: &LookedUp) -> Problem {
    let what = match looked_up {
        LookedUp::Id { id } => format!("no object {id}"),
        LookedUp::Name { name } => format!("no object named {name:?}"),
        LookedUp::Designation {
            source,
            designation,
        } => format!("no object that {source} designates {designation:?}"),
    };
    Problem::new(
        ProblemType::ObjectNotFound,
        format!("Release {release} holds {what}."),
    )
    .with("release", release)
    .with(
        "looked_up",
        serde_json::to_value(looked_up).unwrap_or_default(),
    )
}

#[cfg(test)]
mod record_tests;
#[cfg(test)]
mod tests;
