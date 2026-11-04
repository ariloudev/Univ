//! The OpenAPI 3.1 document of every route served, at `/v1/openapi.json`
//! (read-the-openapi-document).
//!
//! The paths come from each handler's `#[utoipa::path]`, the refusals of each from
//! [`crate::refusals`], and the schemas from the Rust types the server answers with. A test
//! holds the document's paths to [`ROUTES`](crate::ROUTES) exactly.

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
        crate::grids::uncertainty,
        crate::bulk::tile_format,
        crate::keys::issue,
        crate::keys::list,
        crate::keys::change,
        crate::keys::revoke,
        crate::layers::config::create,
        crate::layers::config::list,
        crate::layers::config::read,
        crate::layers::config::configure,
        crate::layers::config::retire,
        crate::layers::changes::list,
        crate::layers::items::read,
        crate::layers::items::version,
        crate::layers::write::publish,
        crate::layers::write::revise,
        crate::layers::write::retract,
        crate::query::volume
    ),
    components(schemas(ProblemDocument, ProblemType, univ_model::tile::manifest::TileManifest)),
    modifiers(&Security)
)]
struct ApiDoc;

/// The bearer schemes: `owner`, the owner secret of the owner's operations, and `api_key`, a
/// key the owner issued, which any request may carry.
struct Security;

impl utoipa::Modify for Security {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
        let components = openapi.components.get_or_insert_with(Default::default);
        for (name, description) in [
            ("owner", "The owner secret the Worker was given."),
            ("api_key", "An API key the owner issued."),
        ] {
            components.add_security_scheme(
                name,
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .description(Some(description))
                        .build(),
                ),
            );
        }
    }
}

/// The name of the problem details schema in the document's components.
pub(crate) const PROBLEM_SCHEMA: &str = "Problem";

/// The OpenAPI document of every route served, with the problem details schema, generated from
/// [`ProblemDocument`], whose `type` lists every kind of problem.
#[must_use]
pub fn document() -> utoipa::openapi::OpenApi {
    let mut document = ApiDoc::openapi();
    crate::refusals::describe(&mut document);
    document
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
pub(crate) mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;
    use crate::routes::ROUTES;

    /// The document as JSON.
    pub(crate) fn value() -> serde_json::Value {
        serde_json::to_value(document()).unwrap()
    }

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
        let value = value();
        assert_eq!(value["openapi"], "3.1.0");
    }

    #[test]
    fn the_problem_schema_lists_every_problem_type() {
        let value = value();
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

    #[test]
    fn each_refusal_a_route_can_give_is_described_with_its_status_and_problem_type() {
        let value = value();
        for route in &ROUTES {
            let method = route.method.as_str().to_lowercase();
            let responses = &value["paths"][route.path][&method]["responses"];
            let mut described: Vec<(u16, String)> = Vec::new();
            for (status, response) in responses.as_object().unwrap() {
                let Some(problem) = response["content"].get(crate::problem::CONTENT_TYPE) else {
                    continue;
                };
                let parts = problem["schema"]["allOf"].as_array().unwrap();
                assert_eq!(parts[0]["$ref"], "#/components/schemas/Problem");
                for uri in parts[1]["properties"]["type"]["enum"].as_array().unwrap() {
                    described.push((status.parse().unwrap(), uri.as_str().unwrap().to_owned()));
                }
            }
            let mut expected: Vec<(u16, String)> = crate::refusals::refusals(route.endpoint)
                .iter()
                .map(|kind| (kind.status(), kind.uri().to_owned()))
                .collect();
            described.sort();
            expected.sort();
            assert_eq!(
                described,
                expected,
                "{} {}",
                route.method.as_str(),
                route.path
            );
        }
    }

    /// The schema named `T::name()` and every schema it refers to, as univ-model generates them.
    fn generated<T: utoipa::ToSchema>(into: &mut BTreeMap<String, serde_json::Value>) {
        let mut schemas = vec![(T::name().into_owned(), T::schema())];
        T::schemas(&mut schemas);
        for (name, schema) in schemas {
            into.insert(name, serde_json::to_value(schema).unwrap());
        }
    }

    #[test]
    fn the_documents_schemas_are_generated_from_univ_models_types() {
        use univ_model::api::{
            bulk, keys, layers, lookup, objects, query, releases, sources, structures,
        };
        type Generate = fn(&mut BTreeMap<String, serde_json::Value>);
        let roots: [Generate; 31] = [
            generated::<releases::ReleaseList>,
            generated::<sources::SourceList>,
            generated::<sources::SourceDetail>,
            generated::<sources::SourceProvenance>,
            generated::<structures::StructurePage>,
            generated::<structures::StructureDetail>,
            generated::<objects::ObjectRecord>,
            generated::<lookup::ObjectChoices>,
            generated::<lookup::ObjectMerged>,
            generated::<bulk::ScaleDomainList>,
            generated::<bulk::GridList>,
            generated::<bulk::TileFormat>,
            generated::<univ_model::tile::manifest::TileManifest>,
            generated::<keys::IssueKey>,
            generated::<keys::IssuedKey>,
            generated::<keys::KeyList>,
            generated::<keys::KeyChange>,
            generated::<keys::KeyRecord>,
            generated::<layers::CreateLayer>,
            generated::<layers::LayerList>,
            generated::<layers::LayerChange>,
            generated::<layers::LayerRecord>,
            generated::<layers::LayerPage>,
            generated::<layers::ChangePage>,
            generated::<layers::ItemRecord>,
            generated::<layers::ItemVersion>,
            generated::<layers::PublishItem>,
            generated::<layers::ReviseItem>,
            generated::<layers::RetractItem>,
            generated::<query::VolumeQuery>,
            generated::<query::VolumePage>,
        ];
        let mut model = BTreeMap::new();
        for generate in roots {
            generate(&mut model);
        }
        let value = value();
        // The server's own: the problem details every refusal is answered with.
        let own = [PROBLEM_SCHEMA, "ProblemType"];
        let documented: BTreeMap<String, serde_json::Value> = value["components"]["schemas"]
            .as_object()
            .unwrap()
            .iter()
            .filter(|(name, _)| !own.contains(&name.as_str()))
            .map(|(name, schema)| (name.clone(), schema.clone()))
            .collect();
        assert_eq!(
            documented, model,
            "every other schema is univ-model's, as generated"
        );
    }
}
