//! The routes Univ Server serves, all under the path of their API version, `/v1`
//! (read-the-openapi-document). The OpenAPI document lists exactly these routes, which a test
//! of `openapi` holds it to.

use crate::access::Caller;
use crate::backend::Backend;
use crate::http::{Method, Request, Response};
use crate::problem::{Problem, ProblemType};
use crate::{bulk, grids, keys, layers, objects, openapi, query, releases, sources, structures};

/// The values a route's `{name}` segments took in one request, in the template's order.
pub type Params = Vec<(&'static str, String)>;

/// What a route does: each names its handler in [`answer`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Endpoint {
    OpenApi,
    Releases,
    Sources,
    Source,
    SourceProvenance,
    Structures,
    Structure,
    Objects,
    Object,
    ScaleDomains,
    TileManifest,
    Tile,
    Grids,
    GridHeader,
    GridSamples,
    GridUncertainty,
    TileFormat,
    IssueKey,
    Keys,
    ChangeKey,
    RevokeKey,
    CreateLayer,
    Layers,
    Layer,
    ConfigureLayer,
    RetireLayer,
    Changes,
    Item,
    ItemVersion,
    PublishItem,
    ReviseItem,
    RetractItem,
    VolumeQuery,
}

/// One route: the method it answers, its path template with `{name}` segments, and its
/// endpoint. A `GET` route also answers `HEAD`, with the same headers and no body.
pub struct Route {
    pub method: Method,
    pub path: &'static str,
    pub endpoint: Endpoint,
}

/// Every route served, in the order they are matched.
pub static ROUTES: [Route; 33] = [
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
    Route {
        method: Method::Get,
        path: "/v1/sources/{id}",
        endpoint: Endpoint::Source,
    },
    Route {
        method: Method::Get,
        path: "/v1/sources/{id}/provenance",
        endpoint: Endpoint::SourceProvenance,
    },
    Route {
        method: Method::Get,
        path: "/v1/structures",
        endpoint: Endpoint::Structures,
    },
    Route {
        method: Method::Get,
        path: "/v1/structures/{id}",
        endpoint: Endpoint::Structure,
    },
    Route {
        method: Method::Get,
        path: "/v1/objects",
        endpoint: Endpoint::Objects,
    },
    Route {
        method: Method::Post,
        path: "/v1/query/volume",
        endpoint: Endpoint::VolumeQuery,
    },
    Route {
        method: Method::Get,
        path: "/v1/objects/{id}",
        endpoint: Endpoint::Object,
    },
    Route {
        method: Method::Get,
        path: "/v1/scale-domains",
        endpoint: Endpoint::ScaleDomains,
    },
    Route {
        method: Method::Get,
        path: "/v1/scale-domains/{name}/manifest",
        endpoint: Endpoint::TileManifest,
    },
    Route {
        method: Method::Get,
        path: "/v1/scale-domains/{name}/tiles/{address}",
        endpoint: Endpoint::Tile,
    },
    Route {
        method: Method::Get,
        path: "/v1/grids",
        endpoint: Endpoint::Grids,
    },
    Route {
        method: Method::Get,
        path: "/v1/grids/{id}",
        endpoint: Endpoint::GridHeader,
    },
    Route {
        method: Method::Get,
        path: "/v1/grids/{id}/fields/{field}",
        endpoint: Endpoint::GridSamples,
    },
    Route {
        method: Method::Get,
        path: "/v1/grids/{id}/fields/{field}/uncertainty",
        endpoint: Endpoint::GridUncertainty,
    },
    Route {
        method: Method::Post,
        path: "/v1/owner/keys",
        endpoint: Endpoint::IssueKey,
    },
    Route {
        method: Method::Get,
        path: "/v1/owner/keys",
        endpoint: Endpoint::Keys,
    },
    Route {
        method: Method::Patch,
        path: "/v1/owner/keys/{id}",
        endpoint: Endpoint::ChangeKey,
    },
    Route {
        method: Method::Post,
        path: "/v1/owner/keys/{id}/revoke",
        endpoint: Endpoint::RevokeKey,
    },
    Route {
        method: Method::Post,
        path: "/v1/layers",
        endpoint: Endpoint::CreateLayer,
    },
    Route {
        method: Method::Get,
        path: "/v1/layers",
        endpoint: Endpoint::Layers,
    },
    Route {
        method: Method::Get,
        path: "/v1/layers/{id}",
        endpoint: Endpoint::Layer,
    },
    Route {
        method: Method::Patch,
        path: "/v1/layers/{id}",
        endpoint: Endpoint::ConfigureLayer,
    },
    Route {
        method: Method::Delete,
        path: "/v1/layers/{id}",
        endpoint: Endpoint::RetireLayer,
    },
    Route {
        method: Method::Get,
        path: "/v1/layers/{id}/changes",
        endpoint: Endpoint::Changes,
    },
    Route {
        method: Method::Get,
        path: "/v1/layers/{id}/items/{item}",
        endpoint: Endpoint::Item,
    },
    Route {
        method: Method::Get,
        path: "/v1/layers/{id}/items/{item}/versions/{version}",
        endpoint: Endpoint::ItemVersion,
    },
    Route {
        method: Method::Post,
        path: "/v1/layers/{id}/items",
        endpoint: Endpoint::PublishItem,
    },
    Route {
        method: Method::Post,
        path: "/v1/layers/{id}/items/{item}/revisions",
        endpoint: Endpoint::ReviseItem,
    },
    Route {
        method: Method::Post,
        path: "/v1/layers/{id}/items/{item}/retraction",
        endpoint: Endpoint::RetractItem,
    },
    Route {
        method: Method::Get,
        path: "/v1/tile-format",
        endpoint: Endpoint::TileFormat,
    },
];

/// The answer of `endpoint` to `request`, a refusal included.
async fn answer<B: Backend>(
    endpoint: Endpoint,
    request: &Request,
    caller: &Caller,
    params: &Params,
    backend: &B,
) -> Response {
    let answered = match endpoint {
        Endpoint::OpenApi => Ok(openapi::serve()),
        Endpoint::Releases => releases::list(backend).await,
        Endpoint::Sources => sources::list(request, backend).await,
        Endpoint::Source => sources::get(request, param(params, "id"), backend).await,
        Endpoint::SourceProvenance => {
            sources::provenance(request, param(params, "id"), backend).await
        }
        Endpoint::Structures => structures::list(request, backend).await,
        Endpoint::Structure => structures::get(request, param(params, "id"), backend).await,
        Endpoint::Objects => objects::find(request, backend).await,
        Endpoint::VolumeQuery => query::volume(request, caller, backend).await,
        Endpoint::Object => objects::get(request, param(params, "id"), backend).await,
        Endpoint::ScaleDomains => bulk::domains(request, backend).await,
        Endpoint::TileManifest => bulk::manifest(request, param(params, "name"), backend).await,
        Endpoint::Tile => {
            bulk::tile(
                request,
                param(params, "name"),
                param(params, "address"),
                backend,
            )
            .await
        }
        Endpoint::Grids => grids::list(request, backend).await,
        Endpoint::TileFormat => bulk::tile_format(request, backend).await,
        Endpoint::IssueKey => keys::issue(request, backend).await,
        Endpoint::Keys => keys::list(backend).await,
        Endpoint::ChangeKey => keys::change(request, param(params, "id"), backend).await,
        Endpoint::RevokeKey => keys::revoke(param(params, "id"), backend).await,
        Endpoint::CreateLayer => layers::config::create(request, caller, backend).await,
        Endpoint::Layers => layers::config::list(caller, backend).await,
        Endpoint::Layer => {
            layers::config::read(request, caller, param(params, "id"), backend).await
        }
        Endpoint::ConfigureLayer => {
            layers::config::configure(request, caller, param(params, "id"), backend).await
        }
        Endpoint::RetireLayer => layers::config::retire(caller, param(params, "id"), backend).await,
        Endpoint::Changes => {
            layers::changes::list(request, caller, param(params, "id"), backend).await
        }
        Endpoint::Item => {
            let (id, item) = (param(params, "id"), param(params, "item"));
            layers::items::read(request, caller, id, item, backend).await
        }
        Endpoint::PublishItem => {
            layers::write::publish(request, caller, param(params, "id"), backend).await
        }
        Endpoint::ReviseItem => {
            let (id, item) = (param(params, "id"), param(params, "item"));
            layers::write::revise(request, caller, id, item, backend).await
        }
        Endpoint::RetractItem => {
            let (id, item) = (param(params, "id"), param(params, "item"));
            layers::write::retract(request, caller, id, item, backend).await
        }
        Endpoint::ItemVersion => {
            let (id, item) = (param(params, "id"), param(params, "item"));
            layers::items::version(request, caller, id, item, param(params, "version"), backend)
                .await
        }
        Endpoint::GridUncertainty => {
            grids::uncertainty(
                request,
                param(params, "id"),
                param(params, "field"),
                backend,
            )
            .await
        }
        Endpoint::GridHeader => grids::header(request, param(params, "id"), backend).await,
        Endpoint::GridSamples => {
            grids::samples(
                request,
                param(params, "id"),
                param(params, "field"),
                backend,
            )
            .await
        }
    };
    // Every test that meets a refusal checks that its route lists it among the refusals the
    // OpenAPI document describes (read-the-openapi-document).
    #[cfg(test)]
    if let Err(problem) = &answered {
        assert!(
            crate::refusals::refusals(endpoint).contains(&problem.kind),
            "{endpoint:?} answered {}, which its refusals do not list",
            problem.kind.uri()
        );
    }
    answered.unwrap_or_else(Problem::into_response)
}

/// Answers a request that is not a preflight: the endpoint of the route it names, or a problem.
/// A HEAD is answered as the GET of its route; [`crate::handle`] leaves out the body.
pub async fn dispatch<B: Backend>(request: &Request, caller: &Caller, backend: &B) -> Response {
    let head = request.method == Method::Head;
    let mut allowed: Vec<&str> = Vec::new();
    for route in &ROUTES {
        let Some(params) = matches(route.path, &request.path) else {
            continue;
        };
        if route.method == request.method || (head && route.method == Method::Get) {
            return answer(route.endpoint, request, caller, &params, backend).await;
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

/// The value of the route's `{name}` segment; every endpoint asks only for segments its own
/// template has.
fn param<'a>(params: &'a Params, name: &str) -> &'a str {
    params
        .iter()
        .find(|(key, _)| *key == name)
        .map_or("", |(_, value)| value.as_str())
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
