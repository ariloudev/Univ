//! The routes Univ Server serves, all under the path of their API version, `/v1`
//! (read-the-openapi-document). The OpenAPI document lists exactly these routes, which a test
//! of `openapi` holds it to.

use serde_json::Value;

use crate::http::{Request, Response};
use crate::openapi;
use crate::problem::{Problem, ProblemType};

/// The values a route's `{name}` segments took in one request, in the template's order.
pub type Params = Vec<(&'static str, String)>;

/// One route: the method it answers, its path template with `{name}` segments, and its
/// handler.
pub struct Route {
    pub method: &'static str,
    pub path: &'static str,
    pub handler: fn(&Request, &Params) -> Response,
}

/// Every route served, in the order they are matched.
pub static ROUTES: [Route; 1] = [Route {
    method: "GET",
    path: "/v1/openapi.json",
    handler: openapi::serve,
}];

/// Answers a request that is not a preflight: the handler of the route it names, or a problem.
pub fn dispatch(request: &Request) -> Response {
    let mut allowed: Vec<&str> = Vec::new();
    for route in &ROUTES {
        let Some(params) = matches(route.path, &request.path) else {
            continue;
        };
        if route.method == request.method.as_str() {
            return (route.handler)(request, &params);
        }
        allowed.push(route.method);
    }
    if allowed.is_empty() {
        return Problem::new(
            ProblemType::RouteNotFound,
            format!("No route answers {}.", request.path),
        )
        .with("path", Value::String(request.path.clone()))
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
    .with("allow", Value::String(allow.clone()))
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
