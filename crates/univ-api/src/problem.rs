//! Refusals and failures as RFC 9457 problem details (read-the-openapi-document).
//!
//! Every refused or failed request is answered with `application/problem+json`. Its `type` is
//! an absolute URI that names the kind of refusal, `urn:univ:problem:<kind>`, a URN so that no
//! domain is claimed. Each kind is listed in the OpenAPI document ([`ProblemType::ALL`]).

use serde_json::{Map, Value};
use utoipa::openapi::RefOr;
use utoipa::openapi::schema::{ObjectBuilder, Schema, SchemaFormat, Type};

use crate::http::Response;

/// The media type of a problem details document (RFC 9457 §3).
pub const CONTENT_TYPE: &str = "application/problem+json";

/// Declares [`ProblemType`] from one row per kind: its doc, its variant, its type URI, its
/// title and its status. The enum, [`ProblemType::ALL`] and the lookups all come from the
/// rows, so a kind cannot be left out of one of them.
macro_rules! problem_types {
    ($($(#[doc = $doc:literal])* $kind:ident = $uri:literal, $title:literal, $status:literal;)+) => {
        /// Each kind of refusal or failure Univ Server can answer with. It serializes as its
        /// type URI, and its OpenAPI schema lists every URI.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
        pub enum ProblemType {
            $($(#[doc = $doc])* #[serde(rename = $uri)] $kind,)+
        }

        impl ProblemType {
            /// Every kind, in the order the OpenAPI document lists them.
            pub const ALL: &[Self] = &[$(Self::$kind),+];

            /// The problem type URI, `urn:univ:problem:<kind>`.
            #[must_use]
            pub fn uri(self) -> &'static str {
                match self { $(Self::$kind => $uri),+ }
            }

            /// The short, human-readable summary that is the same for every problem of this
            /// kind.
            #[must_use]
            pub fn title(self) -> &'static str {
                match self { $(Self::$kind => $title),+ }
            }

            /// The HTTP status every problem of this kind is answered with.
            #[must_use]
            pub fn status(self) -> u16 {
                match self { $(Self::$kind => $status),+ }
            }
        }
    };
}

problem_types! {
    /// No route of the API version answers the path.
    RouteNotFound = "urn:univ:problem:route-not-found", "No such route", 404;
    /// A route answers the path, but not with the request's method.
    MethodNotAllowed = "urn:univ:problem:method-not-allowed", "Method not allowed", 405;
    /// The request's body could not be read to its end.
    UnreadableBody = "urn:univ:problem:unreadable-body", "Unreadable body", 400;
    /// The request's body is not a document the route takes.
    InvalidBody = "urn:univ:problem:invalid-body", "Invalid body", 400;
    /// The request's API key or owner secret is missing where needed, or not valid.
    Unauthorized = "urn:univ:problem:unauthorized", "Unauthorized", 401;
    /// The client made more requests than its rate limit allows in the window.
    RateLimited = "urn:univ:problem:rate-limited", "Too many requests", 429;
    /// No layer has that id, or it is private to another key: the two answer alike.
    LayerNotFound = "urn:univ:problem:layer-not-found", "No such layer", 404;
    /// The layer holds no item of that id, or no version of that number.
    ItemNotFound = "urn:univ:problem:item-not-found", "No such item", 404;
    /// The item failed the checks its `errors` name, each by JSON pointer.
    ItemInvalid = "urn:univ:problem:item-invalid", "Invalid item", 422;
    /// The item cites no source; any other failure is listed beside it.
    SourceRequired = "urn:univ:problem:source-required", "Source required", 422;
    /// The item's id is already in the layer, or the version named is not its current one.
    ItemConflict = "urn:univ:problem:item-conflict", "Item conflict", 409;
    /// The layer was retired.
    LayerRetired = "urn:univ:problem:layer-retired", "Layer retired", 410;
    /// The layer belongs to another key, which alone may change it.
    NotLayerOwner = "urn:univ:problem:not-layer-owner", "Not the layer's owner", 403;
    /// The write would take the key beyond one of its quotas.
    QuotaExceeded = "urn:univ:problem:quota-exceeded", "Quota exceeded", 403;
    /// The owner has issued no key of that id.
    KeyNotFound = "urn:univ:problem:key-not-found", "No such key", 404;
    /// A query parameter or path segment does not hold a value of its kind.
    InvalidParameter = "urn:univ:problem:invalid-parameter", "Invalid parameter", 400;
    /// No release of that number was ever published.
    ReleaseNotFound = "urn:univ:problem:release-not-found", "No such release", 404;
    /// The release was pruned: its data is gone.
    ReleasePruned = "urn:univ:problem:release-pruned", "Release pruned", 410;
    /// The release has no source of that id.
    SourceNotFound = "urn:univ:problem:source-not-found", "No such source", 404;
    /// No object of the release answers the id, designation or name looked up.
    ObjectNotFound = "urn:univ:problem:object-not-found", "No such object", 404;
    /// The release has no structure of that id.
    StructureNotFound = "urn:univ:problem:structure-not-found", "No such structure", 404;
    /// The release has no scale domain of that name.
    ScaleDomainNotFound = "urn:univ:problem:scale-domain-not-found", "No such scale domain", 404;
    /// The scale domain's tile manifest holds no node of that address.
    TileNotFound = "urn:univ:problem:tile-not-found", "No such tile", 404;
    /// The release has no volume grid of that id.
    GridNotFound = "urn:univ:problem:grid-not-found", "No such volume grid", 404;
    /// The volume grid has no field of that index, or the field no uncertainty.
    GridFieldNotFound = "urn:univ:problem:grid-field-not-found", "No such grid field", 404;
    /// The server failed to answer a request it should have answered.
    InternalError = "urn:univ:problem:internal-error", "Internal error", 500;
}

/// The schema of a problem's `type`: a URI, one of [`ProblemType::ALL`]'s. It is written here
/// rather than derived, because the derive does not read a rename that a macro wrote.
impl utoipa::PartialSchema for ProblemType {
    fn schema() -> RefOr<Schema> {
        ObjectBuilder::new()
            .schema_type(Type::String)
            .format(Some(SchemaFormat::Custom("uri".to_owned())))
            .description(Some("The kind of problem, as a URN."))
            .enum_values(Some(Self::ALL.iter().map(|kind| kind.uri())))
            .into()
    }
}

impl utoipa::ToSchema for ProblemType {}

/// The RFC 9457 members of a problem details document, as it is served and as the OpenAPI
/// document describes it; a problem's extension members follow them.
#[derive(serde::Serialize, utoipa::ToSchema)]
#[schema(
    as = Problem,
    description = "An RFC 9457 problem details document, answered as application/problem+json, with any extension members after these."
)]
pub struct ProblemDocument<'a> {
    #[serde(rename = "type")]
    pub kind: ProblemType,
    pub title: &'static str,
    pub status: u16,
    pub detail: &'a str,
}

/// One problem: its kind, what went wrong in this occurrence, any extension members, and any
/// header its answer carries, such as `Retry-After`.
#[derive(Clone, Debug)]
pub struct Problem {
    pub kind: ProblemType,
    pub detail: String,
    pub extensions: Map<String, Value>,
    pub headers: Vec<(String, String)>,
}

impl Problem {
    /// A problem of that kind with that detail and no extension members.
    #[must_use]
    pub fn new(kind: ProblemType, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
            extensions: Map::new(),
            headers: Vec::new(),
        }
    }

    /// A 404 of `kind`: release `release` has no `noun` of id `id`.
    #[must_use]
    pub fn not_found(kind: ProblemType, noun: &str, release: u32, id: &str) -> Self {
        Self::new(kind, format!("Release {release} has no {noun} {id:?}."))
            .with("id", id)
            .with("release", release)
    }

    /// A 400 naming the query parameter `name` and the value it was sent with.
    #[must_use]
    pub fn invalid_parameter(name: &str, value: &str, detail: impl Into<String>) -> Self {
        Self::new(ProblemType::InvalidParameter, detail)
            .with("parameter", name)
            .with("value", value)
    }

    /// The problem with one more extension member.
    #[must_use]
    pub fn with(mut self, name: &str, value: impl Into<Value>) -> Self {
        self.extensions.insert(name.to_owned(), value.into());
        self
    }

    /// The problem with one more header on its answer.
    #[must_use]
    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }

    /// The problem details document: the RFC 9457 members, then the extension members, none
    /// of which can overwrite a standard member.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let standard = ProblemDocument {
            kind: self.kind,
            title: self.kind.title(),
            status: self.kind.status(),
            detail: &self.detail,
        };
        let mut document = match serde_json::to_value(standard) {
            Ok(Value::Object(members)) => members,
            _ => Map::new(),
        };
        for (name, value) in &self.extensions {
            document
                .entry(name.clone())
                .or_insert_with(|| value.clone());
        }
        Value::Object(document)
    }

    /// The response that carries the problem, with its status and media type.
    #[must_use]
    pub fn into_response(self) -> Response {
        let response = Response::bytes(
            self.kind.status(),
            CONTENT_TYPE,
            self.to_json().to_string().into_bytes(),
        );
        self.headers
            .iter()
            .fold(response, |response, (name, value)| {
                response.with_header(name, value)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::Body;

    #[test]
    fn a_problem_is_answered_as_application_problem_json() {
        let response = Problem::new(ProblemType::RouteNotFound, "no route").into_response();
        assert_eq!(response.status, 404);
        assert_eq!(response.header("Content-Type"), Some(CONTENT_TYPE));
    }

    #[test]
    fn a_problem_document_holds_the_rfc_9457_members_and_its_extensions() {
        let problem =
            Problem::new(ProblemType::MethodNotAllowed, "PUT is not served").with("allow", "GET");
        let response = problem.into_response();
        let Body::Bytes(bytes) = response.body else {
            panic!("a problem has a body");
        };
        let document: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(document["type"], "urn:univ:problem:method-not-allowed");
        assert_eq!(document["title"], "Method not allowed");
        assert_eq!(document["status"], 405);
        assert_eq!(document["detail"], "PUT is not served");
        assert_eq!(document["allow"], "GET");
    }

    #[test]
    fn an_extension_cannot_overwrite_a_standard_member() {
        let document = Problem::new(ProblemType::RouteNotFound, "x")
            .with("status", 200)
            .to_json();
        assert_eq!(document["status"], 404);
    }

    #[test]
    fn every_problem_type_uri_is_absolute_and_distinct() {
        let uris: Vec<&str> = ProblemType::ALL.iter().map(|kind| kind.uri()).collect();
        for uri in &uris {
            let (scheme, rest) = uri.split_once(':').unwrap();
            assert!(!scheme.is_empty() && !rest.is_empty(), "{uri} is absolute");
        }
        let mut unique = uris.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), uris.len());
    }
}
