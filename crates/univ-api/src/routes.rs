//! The routes Univ Server serves, all under the path of their API version, `/v1`
//! (read-the-openapi-document). The OpenAPI document lists exactly these routes, which a test
//! of `openapi` holds it to.

use crate::backend::Backend;
use crate::http::{Body, Method, Request, Response};
use crate::problem::{Problem, ProblemType};
use crate::{openapi, releases, sources};

/// The values a route's `{name}` segments took in one request, in the template's order.
pub type Params = Vec<(&'static str, String)>;

/// What a route does: each names its handler in [`answer`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Endpoint {
    OpenApi,
    Releases,
    Sources,
}

/// One route: the method it answers, its path template with `{name}` segments, and its
/// endpoint. A `GET` route also answers `HEAD`, with the same headers and no body.
pub struct Route {
    pub method: Method,
    pub path: &'static str,
    pub endpoint: Endpoint,
}

/// Every route served, in the order they are matched.
pub static ROUTES: [Route; 3] = [
    Route {
        method: Method::Get,
        path: "/v1/openapi.json",
        endpoint: Endpoint::OpenApi,
    },
    Route {
        method: Method::Get,
        path: "/v1/releases",
        endpoint: Endpoint::Releases,
    },
    Route {
        method: Method::Get,
        path: "/v1/sources",
        endpoint: Endpoint::Sources,
    },
];

/// The answer of `endpoint` to `request`, a refusal included.
async fn answer<B: Backend>(
    endpoint: Endpoint,
    request: &Request,
    _params: &Params,
    backend: &B,
) -> Response {
    let answered = match endpoint {
        Endpoint::OpenApi => Ok(openapi::serve()),
        Endpoint::Releases => releases::list(backend).await,
        Endpoint::Sources => sources::list(request, backend).await,
    };
    answered.unwrap_or_else(Problem::into_response)
}

/// Answers a request that is not a preflight: the endpoint of the route it names, or a problem.
pub async fn dispatch<B: Backend>(request: &Request, backend: &B) -> Response {
    let head = request.method == Method::Head;
    let mut allowed: Vec<&str> = Vec::new();
    for route in &ROUTES {
        let Some(params) = matches(route.path, &request.path) else {
            continue;
        };
        if route.method == request.method || (head && route.method == Method::Get) {
            let response = answer(route.endpoint, request, &params, backend).await;
            return if head {
                response.with_body(Body::Empty)
            } else {
                response
            };
        }
        allowed.push(route.method.as_str());
        if route.method == Method::Get {
            allowed.push(Method::Head.as_str());
        }
    }
    if allowed.is_empty() {
        return Problem::new(
            ProblemType::RouteNotFound,
            format!("No route answers {}.", request.path),
        )
        .with("path", request.path.as_str())
        .into_response();
    }
    let allow = allowed.join(", ");
    Problem::new(
        ProblemType::MethodNotAllowed,
        format!(
            "{} answers {allow}, not {}.",
            request.path,
            request.method.as_str()
        ),
    )
    .with("allow", allow.as_str())
    .into_response()
    .with_header("allow", &allow)
}

/// The values of the template's `{name}` segments when `path` matches it, segment by segment.
fn matches(template: &'static str, path: &str) -> Option<Params> {
    let mut params = Params::new();
    let mut wanted = template.split('/');
    let mut given = path.split('/');
    loop {
        match (wanted.next(), given.next()) {
            (None, None) => return Some(params),
            (Some(want), Some(got)) => {
                if let Some(name) = want.strip_prefix('{').and_then(|w| w.strip_suffix('}')) {
                    if got.is_empty() {
                        return None;
                    }
                    params.push((name, got.to_owned()));
                } else if want != got {
                    return None;
                }
            }
            _ => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_template_matches_its_literal_segments_and_captures_its_parameters() {
        let params = matches("/v1/objects/{id}", "/v1/objects/42").unwrap();
        assert_eq!(params, vec![("id", "42".to_owned())]);
        assert!(matches("/v1/objects/{id}", "/v1/objects/").is_none());
        assert!(matches("/v1/objects/{id}", "/v1/objects/42/more").is_none());
        assert!(matches("/v1/objects/{id}", "/v2/objects/42").is_none());
    }

    #[test]
    fn every_route_lives_under_the_path_of_its_api_version() {
        for route in &ROUTES {
            assert!(
                route.path.starts_with("/v1/"),
                "{} lies outside /v1",
                route.path
            );
        }
    }
}
