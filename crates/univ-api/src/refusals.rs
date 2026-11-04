//! Each refusal a route can give, by its status and problem type (read-the-openapi-document):
//! the table [`refusals`] that the OpenAPI document describes for each route, and that the
//! test kit holds every problem a route answers in the tests to.
//!
//! [`ProblemType::RouteNotFound`] and [`ProblemType::MethodNotAllowed`] answer a request that
//! no route takes, so no route lists them; the problem schema lists them with every other type.

use std::collections::BTreeMap;

use utoipa::openapi::path::{Operation, PathItem};
use utoipa::openapi::schema::{AllOfBuilder, ObjectBuilder, Schema, Type};
use utoipa::openapi::{ContentBuilder, OpenApi, Ref, RefOr, ResponseBuilder};

use crate::http::Method;
use crate::openapi::PROBLEM_SCHEMA;
use crate::problem::{CONTENT_TYPE, ProblemType};
use crate::routes::{Endpoint, ROUTES};

use ProblemType::{
    GridFieldNotFound, GridNotFound, InternalError, InvalidBody, InvalidParameter, ItemConflict,
    ItemInvalid, ItemNotFound, KeyNotFound, LayerNotFound, LayerRetired, NotLayerOwner,
    ObjectNotFound, QueryInvalid, QueryLayersChanged, QuotaExceeded, RateLimited, ReleaseNotFound,
    ReleasePruned, ScaleDomainNotFound, SourceNotFound, SourceRequired, StructureNotFound,
    TileNotFound, Unauthorized, UnreadableBody,
};

/// What any route can answer: a key or owner secret that is not valid, a rate limit reached,
/// or a failure of the server.
const EVERY_ROUTE: &[ProblemType] = &[Unauthorized, RateLimited, InternalError];

/// What a read of a release can answer: a `release` that is not a number, never published, or
/// pruned.
const RELEASE: &[ProblemType] = &[InvalidParameter, ReleaseNotFound, ReleasePruned];

/// What a request with a JSON body can answer: a body that could not be read, or is not a
/// document the route takes.
const BODY: &[ProblemType] = &[UnreadableBody, InvalidBody];

/// What a read of a layer can answer: no such layer to the caller, or a retired one.
const LAYER: &[ProblemType] = &[LayerNotFound, LayerRetired];

/// What a write of an item can answer, beside a layer's refusals.
const ITEM_WRITE: &[ProblemType] = &[
    NotLayerOwner,
    ItemInvalid,
    SourceRequired,
    ItemConflict,
    QuotaExceeded,
];

/// Every refusal the route of `endpoint` can give, beside those of [`EVERY_ROUTE`].
#[must_use]
pub fn refusals(endpoint: Endpoint) -> Vec<ProblemType> {
    use Endpoint as E;
    let parts: &[&[ProblemType]] = match endpoint {
        E::OpenApi | E::Keys | E::Layers => &[],
        // Before any release is published, the list has none to give.
        E::Releases => &[&[ReleaseNotFound]],
        E::Sources | E::ScaleDomains | E::Grids | E::TileFormat | E::Structures => &[RELEASE],
        E::Source | E::SourceProvenance => &[RELEASE, &[SourceNotFound]],
        E::Structure => &[RELEASE, &[StructureNotFound]],
        E::Objects | E::Object => &[RELEASE, &[ObjectNotFound]],
        E::TileManifest => &[RELEASE, &[ScaleDomainNotFound]],
        E::Tile => &[RELEASE, &[ScaleDomainNotFound, TileNotFound]],
        E::GridHeader => &[RELEASE, &[GridNotFound]],
        E::GridSamples | E::GridUncertainty => &[RELEASE, &[GridNotFound, GridFieldNotFound]],
        E::IssueKey => &[BODY],
        E::ChangeKey => &[BODY, &[KeyNotFound]],
        E::RevokeKey => &[&[KeyNotFound]],
        E::CreateLayer => &[BODY, &[QuotaExceeded]],
        // A layer's or an item's links are resolved in a release, `release` or the latest.
        E::Layer => &[LAYER, RELEASE],
        E::ConfigureLayer => &[BODY, LAYER, &[NotLayerOwner]],
        E::RetireLayer => &[LAYER, &[NotLayerOwner]],
        E::Changes => &[LAYER, &[InvalidParameter]],
        E::Item | E::ItemVersion => &[LAYER, RELEASE, &[ItemNotFound]],
        E::PublishItem => &[BODY, LAYER, ITEM_WRITE],
        E::ReviseItem | E::RetractItem => &[BODY, LAYER, ITEM_WRITE, &[ItemNotFound]],
        E::VolumeQuery => &[BODY, RELEASE, &[QueryInvalid, QueryLayersChanged]],
    };
    parts
        .iter()
        .copied()
        .flatten()
        .chain(EVERY_ROUTE)
        .copied()
        .collect()
}

/// The operation of `item` that answers `method`.
fn operation_mut<'a>(item: &'a mut PathItem, method: &Method) -> Option<&'a mut Operation> {
    match method {
        Method::Get => item.get.as_mut(),
        Method::Post => item.post.as_mut(),
        Method::Patch => item.patch.as_mut(),
        Method::Delete => item.delete.as_mut(),
        Method::Head | Method::Options | Method::Other(_) => None,
    }
}

/// Writes each route's refusals into `openapi`: one `application/problem+json` response for
/// each status, whose problem `type` is one of the route's types of that status.
pub(crate) fn describe(openapi: &mut OpenApi) {
    for route in &ROUTES {
        let Some(item) = openapi.paths.paths.get_mut(route.path) else {
            continue;
        };
        let Some(operation) = operation_mut(item, &route.method) else {
            continue;
        };
        let mut by_status: BTreeMap<u16, Vec<ProblemType>> = BTreeMap::new();
        for kind in refusals(route.endpoint) {
            by_status.entry(kind.status()).or_default().push(kind);
        }
        for (status, kinds) in by_status {
            let titles: Vec<&str> = kinds.iter().map(|kind| kind.title()).collect();
            let narrowed = ObjectBuilder::new().property(
                "type",
                ObjectBuilder::new()
                    .schema_type(Type::String)
                    .enum_values(Some(kinds.iter().map(|kind| kind.uri()))),
            );
            let schema = AllOfBuilder::new()
                .item(Ref::from_schema_name(PROBLEM_SCHEMA))
                .item(narrowed)
                .build();
            let schema: RefOr<Schema> = Schema::AllOf(schema).into();
            let response = ResponseBuilder::new()
                .description(titles.join("; "))
                .content(
                    CONTENT_TYPE,
                    ContentBuilder::new().schema(Some(schema)).build(),
                )
                .build();
            operation
                .responses
                .responses
                .insert(status.to_string(), RefOr::T(response));
        }
    }
}
