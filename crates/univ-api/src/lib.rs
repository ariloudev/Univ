//! The HTTP contract of Univ Server, written against plain request and response values so that
//! every handler runs natively in tests and inside the Worker alike.
//!
//! The Worker (`univ-server`) turns each runtime request into a [`Request`], calls [`handle`],
//! and turns the [`Response`] back. Nothing here performs I/O.

mod backend;
mod cors;
mod http;
mod openapi;
mod problem;
mod releases;
mod routes;
mod sources;
mod structures;
#[cfg(test)]
mod testkit;

pub use backend::{Backend, BackendError, Row};
pub use http::{Body, Method, Request, Response};
pub use openapi::document;
pub use problem::{Problem, ProblemType};
pub use routes::{ROUTES, Route};

/// The answer to a request the runtime failed to hand over or to answer, such as a body that
/// could not be read: a problem, with the CORS headers every response carries.
#[must_use]
pub fn failure(kind: ProblemType, detail: impl Into<String>) -> Response {
    cors::allow_any_origin(Problem::new(kind, detail).into_response())
}

/// Answers one request from `backend`: a CORS preflight, a route of [`ROUTES`], or a problem.
/// A HEAD is answered with the headers of the GET, and the length of its body, but no body,
/// whatever the answer (RFC 9110 §9.3.2).
pub async fn handle<B: Backend>(request: &Request, backend: &B) -> Response {
    let response = if request.method == Method::Options {
        cors::preflight()
    } else {
        routes::dispatch(request, backend).await
    };
    let response = if request.method == Method::Head {
        let length = response.body.as_bytes().len();
        response
            .with_body(Body::Empty)
            .with_header("content-length", &length.to_string())
    } else {
        response
    };
    cors::allow_any_origin(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::Sqlite;

    fn handle(request: &Request) -> Response {
        Sqlite::new().answer(request)
    }

    fn get(path: &str) -> Request {
        Request::new(Method::Get, path)
    }

    fn problem_type(response: &Response) -> serde_json::Value {
        crate::testkit::body(response)["type"].clone()
    }

    #[test]
    fn a_read_with_no_authorization_header_succeeds() {
        let request = get("/v1/openapi.json");
        assert!(request.header("authorization").is_none());
        assert_eq!(handle(&request).status, 200);
    }

    #[test]
    fn every_response_allows_any_origin() {
        for request in [get("/v1/openapi.json"), get("/v1/nowhere"), get("/")] {
            let response = handle(&request);
            assert_eq!(
                response.header("access-control-allow-origin"),
                Some("*"),
                "{}",
                request.path
            );
        }
    }

    #[test]
    fn a_preflight_for_a_read_is_answered_without_credentials() {
        let mut request = Request::new(Method::Options, "/v1/openapi.json");
        request
            .headers
            .push(("origin".into(), "https://example.org".into()));
        request
            .headers
            .push(("access-control-request-method".into(), "GET".into()));
        let response = handle(&request);
        assert_eq!(response.status, 204);
        assert_eq!(response.header("access-control-allow-origin"), Some("*"));
        let methods = response.header("access-control-allow-methods").unwrap();
        assert!(methods.contains("GET"));
        let headers = response.header("access-control-allow-headers").unwrap();
        assert!(headers.contains("Authorization"));
    }

    #[test]
    fn an_unknown_route_answers_404_with_a_problem() {
        let response = handle(&get("/v1/nowhere"));
        assert_eq!(response.status, 404);
        assert_eq!(
            response.header("content-type"),
            Some("application/problem+json")
        );
        assert_eq!(problem_type(&response), "urn:univ:problem:route-not-found");
    }

    #[test]
    fn a_head_request_is_answered_as_a_get_without_its_body() {
        let get = handle(&get("/v1/openapi.json"));
        let head = handle(&Request::new(Method::Head, "/v1/openapi.json"));
        assert_eq!(head.status, 200);
        assert_eq!(head.body, Body::Empty);
        assert_eq!(head.header("content-type"), get.header("content-type"));
        let length = get.body.as_bytes().len().to_string();
        assert_eq!(head.header("content-length"), Some(length.as_str()));
    }

    #[test]
    fn a_head_request_that_is_refused_has_no_body_either() {
        for path in ["/v1/nowhere", "/v1/sources/nowhere"] {
            let head = handle(&Request::new(Method::Head, path));
            assert_eq!(head.status, 404, "{path}");
            assert_eq!(head.body, Body::Empty, "{path}");
        }
    }

    #[test]
    fn a_method_in_lower_case_is_not_served() {
        let response = handle(&Request::new(Method::parse("get"), "/v1/openapi.json"));
        assert_eq!(response.status, 405);
    }

    #[test]
    fn a_path_outside_the_api_version_is_no_route() {
        assert_eq!(handle(&get("/openapi.json")).status, 404);
    }

    #[test]
    fn a_route_asked_with_another_method_answers_405_naming_the_allowed_ones() {
        let response = handle(&Request::new(Method::Post, "/v1/openapi.json"));
        assert_eq!(response.status, 405);
        assert_eq!(response.header("allow"), Some("GET, HEAD"));
        assert_eq!(
            problem_type(&response),
            "urn:univ:problem:method-not-allowed"
        );
    }
}
