//! Refusals and failures as RFC 9457 problem details (read-the-openapi-document).
//!
//! Every refused or failed request is answered with `application/problem+json`. Its `type` is
//! an absolute URI that names the kind of refusal, `urn:univ:problem:<kind>`, a URN so that no
//! domain is claimed. Each kind is listed in the OpenAPI document ([`ProblemType::ALL`]).

use serde_json::{Map, Value};

use crate::http::Response;

/// The media type of a problem details document (RFC 9457 §3).
pub const CONTENT_TYPE: &str = "application/problem+json";

/// Each kind of refusal or failure Univ Server can answer with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProblemType {
    /// No route of the API version answers the path.
    RouteNotFound,
    /// A route answers the path, but not with the request's method.
    MethodNotAllowed,
    /// The request's body could not be read to its end.
    UnreadableBody,
    /// A query parameter or path segment does not hold a value of its kind.
    InvalidParameter,
    /// No release of that number was ever published.
    ReleaseNotFound,
    /// The release was pruned: its data is gone.
    ReleasePruned,
    /// The release has no source of that id.
    SourceNotFound,
    /// The server failed to answer a request it should have answered.
    InternalError,
}

impl ProblemType {
    /// Every kind, in the order the OpenAPI document lists them.
    pub const ALL: &[Self] = &[
        Self::RouteNotFound,
        Self::MethodNotAllowed,
        Self::UnreadableBody,
        Self::InvalidParameter,
        Self::ReleaseNotFound,
        Self::ReleasePruned,
        Self::SourceNotFound,
        Self::InternalError,
    ];

    /// The problem type URI, `urn:univ:problem:<kind>`.
    #[must_use]
    pub fn uri(self) -> &'static str {
        match self {
            Self::RouteNotFound => "urn:univ:problem:route-not-found",
            Self::MethodNotAllowed => "urn:univ:problem:method-not-allowed",
            Self::UnreadableBody => "urn:univ:problem:unreadable-body",
            Self::InvalidParameter => "urn:univ:problem:invalid-parameter",
            Self::ReleaseNotFound => "urn:univ:problem:release-not-found",
            Self::ReleasePruned => "urn:univ:problem:release-pruned",
            Self::SourceNotFound => "urn:univ:problem:source-not-found",
            Self::InternalError => "urn:univ:problem:internal-error",
        }
    }

    /// The short, human-readable summary that is the same for every problem of this kind.
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Self::RouteNotFound => "No such route",
            Self::MethodNotAllowed => "Method not allowed",
            Self::UnreadableBody => "Unreadable body",
            Self::InvalidParameter => "Invalid parameter",
            Self::ReleaseNotFound => "No such release",
            Self::ReleasePruned => "Release pruned",
            Self::SourceNotFound => "No such source",
            Self::InternalError => "Internal error",
        }
    }

    /// The HTTP status every problem of this kind is answered with.
    #[must_use]
    pub fn status(self) -> u16 {
        match self {
            Self::RouteNotFound | Self::ReleaseNotFound | Self::SourceNotFound => 404,
            Self::MethodNotAllowed => 405,
            Self::UnreadableBody | Self::InvalidParameter => 400,
            Self::ReleasePruned => 410,
            Self::InternalError => 500,
        }
    }
}

/// One problem: its kind, what went wrong in this occurrence, and any extension members.
#[derive(Clone, Debug)]
pub struct Problem {
    pub kind: ProblemType,
    pub detail: String,
    pub extensions: Map<String, Value>,
}

impl Problem {
    /// A problem of that kind with that detail and no extension members.
    #[must_use]
    pub fn new(kind: ProblemType, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
            extensions: Map::new(),
        }
    }

    /// The problem with one more extension member.
    #[must_use]
    pub fn with(mut self, name: &str, value: impl Into<Value>) -> Self {
        self.extensions.insert(name.to_owned(), value.into());
        self
    }

    /// The problem details document: the RFC 9457 members, then the extension members.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut document = Map::new();
        document.insert("type".to_owned(), Value::from(self.kind.uri()));
        document.insert("title".to_owned(), Value::from(self.kind.title()));
        document.insert("status".to_owned(), Value::from(self.kind.status()));
        document.insert("detail".to_owned(), Value::String(self.detail.clone()));
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
        Response::bytes(
            self.kind.status(),
            CONTENT_TYPE,
            self.to_json().to_string().into_bytes(),
        )
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
