//! The OpenAPI 3.1 document of every route served, at `/v1/openapi.json`
//! (read-the-openapi-document).
//!
//! The paths come from each handler's `#[utoipa::path]`, and the schemas from the Rust types the
//! server answers with. A test holds the document's paths to [`ROUTES`](crate::ROUTES) exactly.

use std::sync::LazyLock;

use utoipa::OpenApi;

use crate::http::{Body, Response};
use crate::problem::{Problem, ProblemDocument, ProblemType};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Univ Server",
        description = "Real, normalized astronomical and cosmological data, and the layers clients author on top of it."
    ),
    paths(
        serve,
        crate::releases::list,
        crate::sources::list,
        crate::sources::get,
        crate::sources::provenance::provenance,
        crate::structures::list,
        crate::structures::get,
        crate::objects::find,
        crate::objects::get,
        crate::bulk::domains,
        crate::bulk::manifest,
        crate::bulk::tile,
        crate::grids::list,
        crate::grids::header,
        crate::grids::samples,
        crate::bulk::tile_format
    ),
    components(schemas(ProblemDocument, ProblemType, univ_model::tile::manifest::TileManifest))
)]
struct ApiDoc;

/// The OpenAPI document of every route served, with the problem details schema, generated from
/// [`ProblemDocument`], whose `type` lists every kind of problem.
#[must_use]
pub fn document() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

/// Serves the OpenAPI document.
#[utoipa::path(
    get,
    path = "/v1/openapi.json",
    responses(
        (status = 200, description = "The OpenAPI 3.1 document of every route served", content_type = "application/json")
    )
)]
pub fn serve() -> Response {
    match &*SERIALIZED {
        Ok(bytes) => Response::empty(200)
            .with_header("content-type", "application/json")
            .with_body(Body::Static(bytes)),
        Err(error) => Problem::new(
            ProblemType::InternalError,
            format!("The OpenAPI document did not serialize: {error}."),
        )
        .into_response(),
    }
}

/// The document as JSON, built once per isolate: it depends on nothing but the code.
static SERIALIZED: LazyLock<Result<Vec<u8>, String>> =
    LazyLock::new(|| serde_json::to_vec(&document()).map_err(|error| error.to_string()));

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::routes::ROUTES;

    /// The name of the problem details schema in the document's components.
    const PROBLEM_SCHEMA: &str = "Problem";

    /// Each `(method, path)` the document describes.
    fn documented() -> BTreeSet<(String, String)> {
        let mut operations = BTreeSet::new();
        for (path, item) in &document().paths.paths {
            let methods = [
                ("GET", item.get.is_some()),
                ("PUT", item.put.is_some()),
                ("POST", item.post.is_some()),
                ("PATCH", item.patch.is_some()),
                ("DELETE", item.delete.is_some()),
                ("HEAD", item.head.is_some()),
                ("OPTIONS", item.options.is_some()),
                ("TRACE", item.trace.is_some()),
            ];
            for (method, present) in methods {
                if present {
                    operations.insert((method.to_owned(), path.clone()));
                }
            }
        }
        operations
    }

    #[test]
    fn the_document_describes_every_route_served_and_no_other() {
        let served: BTreeSet<(String, String)> = ROUTES
            .iter()
            .map(|route| (route.method.as_str().to_owned(), route.path.to_owned()))
            .collect();
        assert_eq!(documented(), served);
    }

    #[test]
    fn the_document_is_openapi_3_1() {
        let value = serde_json::to_value(document()).unwrap();
        assert_eq!(value["openapi"], "3.1.0");
    }

    #[test]
    fn the_problem_schema_lists_every_problem_type() {
        let value = serde_json::to_value(document()).unwrap();
        let schemas = &value["components"]["schemas"];
        let kind = &schemas[PROBLEM_SCHEMA]["properties"]["type"]["$ref"];
        assert_eq!(kind, "#/components/schemas/ProblemType");
        for member in ["type", "title", "status", "detail"] {
            assert!(
                schemas[PROBLEM_SCHEMA]["required"]
                    .as_array()
                    .unwrap()
                    .contains(&member.into()),
                "{member} is required"
            );
        }
        let listed = &schemas["ProblemType"]["enum"];
        let uris: Vec<&str> = ProblemType::ALL.iter().map(|kind| kind.uri()).collect();
        assert_eq!(listed, &serde_json::json!(uris));
    }
}
