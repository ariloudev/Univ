//! Publishing, revising and retracting items (publish-a-layer-item, revise-or-retract-an-item,
//! enforce-quotas-and-rate-limits). Each handler checks its request, every failing member by
//! JSON pointer, and hands the new version to [`store::store`].

mod store;

use serde_json::{Map, Value};
use univ_model::api::layers::{ItemVersion, RevisionKind, VersionBody, VersionKind};
use univ_model::geometry::Aabb;
use univ_model::units::Unit;

use super::validate::{self, Checked, Failures, member, pointer, text, unknown};
use super::{LayerRow, broken, writable};
use crate::access::Caller;
use crate::backend::Backend;
use crate::http::{Request, Response};
use crate::problem::{Problem, ProblemType};
use store::{NewVersion, store};

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
    let mut members: Map<String, Value> = request.json()?;
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
    let layer = writable(backend, caller, id, item.as_deref()).await?;
    let content = Value::Object(members);
    let checked = validate::content(backend, Some(&content), "", &mut failures).await?;
    failures.refusal()?;
    let (Some(item), Some(checked)) = (item, checked) else {
        return Err(broken(
            "An item",
            "it passed its checks with a part missing",
        ));
    };
    if let Some(current) = layer.current_version {
        return Err(conflict(
            &item,
            current,
            "The layer already holds an item of this id.",
        ));
    }
    if layer.items_held >= layer.items_per_layer {
        return Err(super::config::quota_exceeded(
            "items_per_layer",
            layer.items_per_layer,
            layer.items_held,
        ));
    }
    let body = VersionBody {
        content: Some(canonical(&checked)?),
        ..VersionBody::default()
    };
    let new = NewVersion {
        item: &item,
        kind: VersionKind::Published,
        body,
        times: &checked.times,
        bounds: placed(&checked),
    };
    store(backend, &layer, new).await
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
    let layer = writable(backend, caller, id, Some(item)).await?;
    let members: Map<String, Value> = request.json()?;
    let mut failures = Failures::default();
    let names = [
        "replaces",
        "revision",
        "corrects",
        "what_was_wrong",
        "content",
    ];
    unknown(&members, &names, "", "a revision", &mut failures);
    let replaces = version_number(&members, "replaces", &mut failures);
    let revision = member::<RevisionKind>(&members, "", "revision", true, &mut failures);
    let (corrects, what_was_wrong) = correction(&members, replaces, revision, &mut failures);
    let checked =
        validate::content(backend, members.get("content"), "/content", &mut failures).await?;
    failures.refusal()?;
    let (Some(replaces), Some(revision), Some(checked)) = (replaces, revision, checked) else {
        return Err(broken(
            "A revision",
            "it passed its checks with a part missing",
        ));
    };
    current(&layer, item, replaces)?;
    let kind = match revision {
        RevisionKind::Update => VersionKind::Update,
        RevisionKind::Correction => VersionKind::Correction,
    };
    let body = VersionBody {
        content: Some(canonical(&checked)?),
        corrects,
        what_was_wrong,
        reason: None,
    };
    let new = NewVersion {
        item,
        kind,
        body,
        times: &checked.times,
        bounds: placed(&checked),
    };
    store(backend, &layer, new).await
}

/// Retracts an item: a new version that marks it retracted, with the reason.
///
/// # Errors
///
/// A problem answer: no reason (422), a version other than the current one replaced or a
/// retracted item (409), the item absent (404), a quota (403), the layer's access refused, or
/// the database failing.
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
    let layer = writable(backend, caller, id, Some(item)).await?;
    let members: Map<String, Value> = request.json()?;
    let mut failures = Failures::default();
    unknown(
        &members,
        &["replaces", "reason"],
        "",
        "a retraction",
        &mut failures,
    );
    let replaces = version_number(&members, "replaces", &mut failures);
    let reason = text(&members, "", "reason", &mut failures);
    failures.refusal()?;
    let (Some(replaces), Some(reason)) = (replaces, reason) else {
        return Err(broken(
            "A retraction",
            "it passed its checks with a part missing",
        ));
    };
    current(&layer, item, replaces)?;
    let body = VersionBody {
        reason: Some(reason),
        ..VersionBody::default()
    };
    let new = NewVersion {
        item,
        kind: VersionKind::Retraction,
        body,
        times: &[],
        bounds: None,
    };
    store(backend, &layer, new).await
}

/// The version a correction corrects, from 1 to the one it replaces (that one where absent),
/// and what was wrong; an update gives neither.
fn correction(
    members: &Map<String, Value>,
    replaces: Option<u64>,
    revision: Option<RevisionKind>,
    failures: &mut Failures,
) -> (Option<u64>, Option<String>) {
    if revision != Some(RevisionKind::Correction) {
        for name in ["corrects", "what_was_wrong"] {
            if members.contains_key(name) {
                failures.add(pointer("", name), "belongs to a correction alone");
            }
        }
        return (None, None);
    }
    let corrects = match members.get("corrects") {
        None => replaces,
        Some(_) => version_number(members, "corrects", failures),
    };
    if corrects
        .zip(replaces)
        .is_some_and(|(corrects, replaces)| corrects > replaces)
    {
        failures.add(
            "/corrects",
            "is a version from 1 to the one the revision replaces",
        );
    }
    (corrects, text(members, "", "what_was_wrong", failures))
}

/// Whether `replaces` is `item`'s current version, which is not a retraction: a 404 for an
/// item the layer does not hold, a 409 with its current version otherwise.
fn current(layer: &LayerRow, item: &str, replaces: u64) -> Result<(), Problem> {
    let Some(current) = layer.current_version else {
        return Err(super::items::not_found(&layer.id, item));
    };
    if layer.current_retracted == Some(1) {
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
    Ok(())
}

/// The box of checked content's shape in the ICRS, in megaparsecs, as volume queries read it.
/// Every shape that passes its checks rotates into the ICRS and is in a unit of length.
fn placed(checked: &Checked) -> Option<Aabb> {
    checked.content.geometry.icrs_bounds(Unit::Megaparsec)
}

/// Checked content as stored: the item's own types written back, one form for every version.
fn canonical(checked: &Checked) -> Result<Value, Problem> {
    serde_json::to_value(&checked.content).map_err(|e| broken("An item", e))
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

/// Whether `id` is an item id: 1 to 128 ASCII letters, digits, `.`, `_` or `-`.
fn valid_id(id: &str) -> bool {
    (1..=128).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

/// A refusal of a write that conflicts with the item as it stands.
fn conflict(item: &str, current: u64, detail: &str) -> Problem {
    Problem::new(ProblemType::ItemConflict, detail)
        .with("item", item)
        .with("current_version", current)
}
