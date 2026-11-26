//! The Galaxy model of a release (read-the-galaxy-model): its definition as the release sampled
//! it, from the release's `galaxy_models` row.

use serde_json::json;
use univ_model::api::galaxy_model::GalaxyModel;
use univ_model::units::GALAXY_MODEL_DOMAIN;
use univ_model::valid_in;

use crate::backend::{Backend, column, first, json_column};
use crate::http::{Request, Response};
use crate::problem::{Problem, ProblemType};
use crate::releases::{pinned, resolve};

/// Reads the Galaxy model of the release the request reads.
///
/// # Errors
///
/// A problem answer: the release refused, a release that holds no Galaxy model (404, naming
/// it), or the release database unreadable.
#[utoipa::path(
    get,
    path = "/v1/galaxy-model",
    params(
        ("release" = Option<u32>, Query, description = "The release to read; latest when absent")
    ),
    responses(
        (status = 200, description = "The Galaxy model", body = GalaxyModel)
    )
)]
pub async fn get<B: Backend>(request: &Request, backend: &B) -> Result<Response, Problem> {
    let release = resolve(request, backend).await?;
    let rows = backend
        .releases(
            concat!(
                "SELECT domain, points, seed, definition, followed FROM galaxy_models \
                 WHERE domain = ?2 AND ",
                valid_in!("", "?1")
            ),
            &[json!(release), json!(GALAXY_MODEL_DOMAIN)],
        )
        .await?;
    let row = first(rows).ok_or_else(|| {
        Problem::new(
            ProblemType::GalaxyModelNotFound,
            format!("Release {release} holds no Galaxy model."),
        )
        .with("release", release)
    })?;
    let model = GalaxyModel {
        release,
        domain: column(&row, "domain")?,
        points: column(&row, "points")?,
        seed: column(&row, "seed")?,
        definition: json_column(&row, "definition")?,
        followed: json_column(&row, "followed")?,
    };
    Ok(pinned(Response::json(&model)?, release))
}

#[cfg(test)]
#[path = "galaxy_model_tests.rs"]
mod tests;
