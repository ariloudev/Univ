//! Publishing, revising and retracting items (publish-a-layer-item, revise-or-retract-an-item,
//! enforce-quotas-and-rate-limits). Each write is one batch: the item's row, its new version,
//! its time records on TT, the layer's sequence and the change take effect together.

mod store;

use serde_json::{Map, Value, json};
use univ_model::api::layers::{ItemVersion, VersionBody, VersionKind};

use super::config::quota_exceeded;
use super::validate::{self, Checked, Failures, pointer};
use super::{LayerRow, now, writable};
use crate::access::Caller;
use crate::backend::Backend;
use crate::http::{Request, Response};
use crate::problem::{Problem, ProblemType};
use store::{Write, body_of, commit, current, quotas, stored, times, version_row};

/// Publishes an item as its first version.
///
/// # Errors
///
/// A problem answer: the item failing its checks or citing no source (422, every failure by
/// JSON pointer), an id the layer holds (409, with its current version), a quota (403), the
/// layer's access refused (401, 403, 404, 410), or the database failing.
#[utoipa::path(
    post,
    path = "/v1/layers/{id}/items",
    params(("id" = String, Path, description = "The layer's id")),
    request_body = univ_model::api::layers::PublishItem,
    security(("api_key" = [])),
    responses((status = 201, description = "The item's first version", body = ItemVersion))
)]
pub async fn publish<B: Backend>(
    request: &Request,
    caller: &Caller,
    id: &str,
    backend: &B,
) -> Result<Response, Problem> {
    let layer = writable(backend, caller, id).await?;
    let mut members = object(request)?;
    let mut failures = Failures::default();
    let item = match members.remove("id") {
        Some(Value::String(item)) if valid_id(&item) => Some(item),
        _ => {
            failures.add(
                "/id",
                "is required: 1 to 128 letters, digits, '.', '_' or '-'",
            );
            None
        }
    };
    let content = Value::Object(members);
    let (checked, no_source) =
        validate::content(backend, Some(&content), "", &mut failures).await?;
    failures.refusal(no_source)?;
    let (Some(item), Some(checked)) = (item, checked) else {
        return Err(unchecked());
    };
    let body = body_of(&VersionBody {
        content: Some(canonical(&checked)?),
        ..VersionBody::default()
    })?;
    let quotas = quotas(backend, &layer).await?;
    if quotas.items >= quotas.items_per_layer {
        return Err(quota_exceeded(
            "items_per_layer",
            quotas.items_per_layer,
            quotas.items,
        ));
    }
    quotas.fits(&body)?;
    if let Some((current, _)) = current(backend, &layer.id, &item).await? {
        return Err(conflict(
            &item,
            current,
            "The layer already holds an item of this id.",
        ));
    }
    let stored_at = now(backend);
    let mut writes = vec![
        Write::new(
            "INSERT INTO items (layer_id, id, version) VALUES (?1, ?2, 1)",
            vec![json!(layer.id), json!(item)],
        ),
        version_row(
            &layer.id,
            &item,
            1,
            &stored_at,
            VersionKind::Published,
            &body,
        ),
    ];
    writes.extend(times(&layer.id, &item, Some(&checked)));
    commit(
        backend,
        &layer,
        &item,
        1,
        "item_published",
        &stored_at,
        writes,
    )
    .await?;
    stored(&layer, &item, 1, stored_at, VersionKind::Published, &body)
}

/// Revises an item: its next version, as an update or a correction.
///
/// # Errors
///
/// A problem answer: a revision failing any check a publish applies (422), a version other
/// than the current one replaced or a retracted item (409, with its current version), the
/// item absent (404), a quota (403), the layer's access refused, or the database failing.
#[utoipa::path(
    post,
    path = "/v1/layers/{id}/items/{item}/revisions",
    params(
        ("id" = String, Path, description = "The layer's id"),
        ("item" = String, Path, description = "The item's id")
    ),
    request_body = univ_model::api::layers::ReviseItem,
    security(("api_key" = [])),
    responses((status = 201, description = "The item's new version", body = ItemVersion))
)]
pub async fn revise<B: Backend>(
    request: &Request,
    caller: &Caller,
    id: &str,
    item: &str,
    backend: &B,
) -> Result<Response, Problem> {
    let layer = writable(backend, caller, id).await?;
    let members = object(request)?;
    let mut failures = Failures::default();
    for name in members.keys() {
        if ![
            "replaces",
            "revision",
            "corrects",
            "what_was_wrong",
            "content",
        ]
        .contains(&name.as_str())
        {
            failures.add(pointer("", name), "is not a member of a revision");
        }
    }
    let replaces = version_number(&members, "replaces", &mut failures);
    let correction = match members.get("revision").and_then(Value::as_str) {
        Some("update") => false,
        Some("correction") => true,
        _ => {
            failures.add("/revision", "is required: update or correction");
            false
        }
    };
    let corrects = if correction {
        correction_of(&members, replaces, &mut failures)
    } else {
        for name in ["corrects", "what_was_wrong"] {
            if members.contains_key(name) {
                failures.add(pointer("", name), "belongs to a correction alone");
            }
        }
        None
    };
    let (checked, no_source) =
        validate::content(backend, members.get("content"), "/content", &mut failures).await?;
    failures.refusal(no_source)?;
    let (Some(replaces), Some(checked)) = (replaces, checked) else {
        return Err(unchecked());
    };
    let kind = if correction {
        VersionKind::Correction
    } else {
        VersionKind::Update
    };
    let body = body_of(&VersionBody {
        content: Some(canonical(&checked)?),
        corrects: corrects.as_ref().map(|(version, _)| *version),
        what_was_wrong: corrects.map(|(_, why)| why),
        reason: None,
    })?;
    replace(
        backend,
        &layer,
        item,
        replaces,
        kind,
        "item_revised",
        &body,
        Some(&checked),
    )
    .await
}

/// Retracts an item: a new version that marks it retracted, with the reason.
///
/// # Errors
///
/// A problem answer: no reason (422), a version other than the current one replaced or a
/// retracted item (409), the item absent (404), the layer's access refused, or the database
/// failing.
#[utoipa::path(
    post,
    path = "/v1/layers/{id}/items/{item}/retraction",
    params(
        ("id" = String, Path, description = "The layer's id"),
        ("item" = String, Path, description = "The item's id")
    ),
    request_body = univ_model::api::layers::RetractItem,
    security(("api_key" = [])),
    responses((status = 201, description = "The item's retraction version", body = ItemVersion))
)]
pub async fn retract<B: Backend>(
    request: &Request,
    caller: &Caller,
    id: &str,
    item: &str,
    backend: &B,
) -> Result<Response, Problem> {
    let layer = writable(backend, caller, id).await?;
    let members = object(request)?;
    let mut failures = Failures::default();
    for name in members
        .keys()
        .filter(|name| !["replaces", "reason"].contains(&name.as_str()))
    {
        failures.add(pointer("", name), "is not a member of a retraction");
    }
    let replaces = version_number(&members, "replaces", &mut failures);
    let reason = match members.get("reason") {
        Some(Value::String(reason)) if !reason.trim().is_empty() => Some(reason.clone()),
        _ => {
            failures.add("/reason", "is required, as text that is not empty");
            None
        }
    };
    failures.refusal(false)?;
    let (Some(replaces), Some(reason)) = (replaces, reason) else {
        return Err(unchecked());
    };
    let body = body_of(&VersionBody {
        reason: Some(reason),
        ..VersionBody::default()
    })?;
    let kind = VersionKind::Retraction;
    replace(
        backend,
        &layer,
        item,
        replaces,
        kind,
        "item_retracted",
        &body,
        None,
    )
    .await
}

/// Stores the version after `replaces` of `item`, which must be its current one and not
/// retracted: a revision with its time records, or a retraction with none.
#[expect(clippy::too_many_arguments, reason = "one write's every part, named")]
async fn replace<B: Backend>(
    backend: &B,
    layer: &LayerRow,
    item: &str,
    replaces: u64,
    kind: VersionKind,
    change: &str,
    body: &str,
    checked: Option<&Checked>,
) -> Result<Response, Problem> {
    let Some((current, retracted)) = current(backend, &layer.id, item).await? else {
        return Err(super::items::not_found(&layer.id, item));
    };
    if retracted {
        return Err(conflict(
            item,
            current,
            "The item is retracted; it takes no new version.",
        ));
    }
    if replaces != current {
        return Err(conflict(
            item,
            current,
            "The version replaced is not the item's current one.",
        ));
    }
    if checked.is_some() {
        quotas(backend, layer).await?.fits(body)?;
    }
    let version = current + 1;
    let stored_at = now(backend);
    let mut writes = vec![
        Write::new(
            "UPDATE items SET version = ?3, retracted = ?4 \
             WHERE layer_id = ?1 AND id = ?2 AND version = ?5",
            vec![
                json!(layer.id),
                json!(item),
                json!(version),
                json!(i32::from(kind == VersionKind::Retraction)),
                json!(current),
            ],
        ),
        version_row(&layer.id, item, version, &stored_at, kind, body),
    ];
    writes.extend(times(&layer.id, item, checked));
    commit(backend, layer, item, version, change, &stored_at, writes).await?;
    stored(layer, item, version, stored_at, kind, body)
}

/// Checked content as stored: the item's own types written back, one form for every version.
fn canonical(checked: &Checked) -> Result<Value, Problem> {
    serde_json::to_value(&checked.content).map_err(|e| {
        Problem::new(
            ProblemType::InternalError,
            format!("An item did not write: {e}."),
        )
    })
}

/// The request's body as a JSON object, or a 400.
fn object(request: &Request) -> Result<Map<String, Value>, Problem> {
    match request.json::<Value>()? {
        Value::Object(members) => Ok(members),
        _ => Err(Problem::new(
            ProblemType::InvalidBody,
            "The body is a JSON object.",
        )),
    }
}

/// Member `name` as a version number, 1 or more, or a failure naming it.
fn version_number(
    members: &Map<String, Value>,
    name: &str,
    failures: &mut Failures,
) -> Option<u64> {
    let number = members
        .get(name)
        .and_then(Value::as_u64)
        .filter(|n| *n >= 1);
    if number.is_none() {
        failures.add(
            pointer("", name),
            "is required: a version number, 1 or more",
        );
    }
    number
}

/// The version a correction corrects, 1 to the one it replaces, and what was wrong.
fn correction_of(
    members: &Map<String, Value>,
    replaces: Option<u64>,
    failures: &mut Failures,
) -> Option<(u64, String)> {
    let corrects = match members.get("corrects") {
        None => replaces,
        Some(value) => value
            .as_u64()
            .filter(|n| *n >= 1 && replaces.is_none_or(|r| *n <= r)),
    };
    if corrects.is_none() && replaces.is_some() {
        failures.add(
            "/corrects",
            "is a version from 1 to the one the revision replaces",
        );
    }
    let why = match members.get("what_was_wrong") {
        Some(Value::String(why)) if !why.trim().is_empty() => Some(why.clone()),
        _ => {
            failures.add("/what_was_wrong", "a correction says what was wrong");
            None
        }
    };
    corrects.zip(why)
}

/// Whether `id` is an item id: 1 to 128 ASCII letters, digits, `.`, `_` or `-`.
fn valid_id(id: &str) -> bool {
    (1..=128).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

pub(super) fn conflict(item: &str, current: u64, detail: &str) -> Problem {
    Problem::new(ProblemType::ItemConflict, detail)
        .with("item", item)
        .with("current_version", current)
}

fn unchecked() -> Problem {
    Problem::new(
        ProblemType::InternalError,
        "An item passed its checks with a part missing.",
    )
}
