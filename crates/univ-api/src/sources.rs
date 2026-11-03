//! A release's sources (read-a-source).

use serde_json::json;
use univ_model::api::sources::{SourceList, SourceSummary};

use crate::backend::{Backend, Row, column};
use crate::http::{Request, Response};
use crate::problem::Problem;
use crate::releases::{pinned, resolve};

/// Lists the sources of the release the request reads.
///
/// # Errors
///
/// A problem answer: the release refused, or the release database unreadable.
#[utoipa::path(
    get,
    path = "/v1/sources",
    params(("release" = Option<u32>, Query, description = "The release to read; latest when absent")),
    responses(
        (status = 200, description = "Every source of the release", body = SourceList)
    )
)]
pub async fn list<B: Backend>(request: &Request, backend: &B) -> Result<Response, Problem> {
    let release = resolve(request, backend).await?;
    let rows = backend
        .releases(
            "SELECT id, title, scale_domain, entries, citation, credit FROM sources \
             WHERE release = ?1 ORDER BY n",
            &[json!(release)],
        )
        .await?;
    let sources = rows
        .iter()
        .map(summary)
        .collect::<Result<Vec<_>, Problem>>()?;
    let listing = SourceList { release, sources };
    Ok(pinned(Response::json(&json!(listing)), release))
}

fn summary(row: &Row) -> Result<SourceSummary, Problem> {
    Ok(SourceSummary {
        id: column(row, "id")?,
        title: column(row, "title")?,
        scale_domain: column(row, "scale_domain")?,
        entries: column(row, "entries")?,
        citation: column(row, "citation")?,
        credit: column(row, "credit")?,
    })
}
