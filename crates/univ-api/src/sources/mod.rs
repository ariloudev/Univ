//! A release's sources (read-a-source).

pub(crate) mod provenance;

pub use provenance::provenance;

use serde_json::json;
use univ_model::api::sources::{
    Coverage, Reference, SourceDetail, SourceList, SourceSummary, Terms,
};

use crate::backend::{Backend, Row, column, first, json_column};
use crate::http::{Request, Response};
use crate::problem::{Problem, ProblemType};
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

/// Reads one source of the release the request reads.
///
/// # Errors
///
/// A problem answer: the release refused, no source of that id (404), or the release database
/// unreadable.
#[utoipa::path(
    get,
    path = "/v1/sources/{id}",
    params(
        ("id" = String, Path, description = "The source's id"),
        ("release" = Option<u32>, Query, description = "The release to read; latest when absent")
    ),
    responses(
        (status = 200, description = "The source", body = SourceDetail)
    )
)]
pub async fn get<B: Backend>(
    request: &Request,
    id: &str,
    backend: &B,
) -> Result<Response, Problem> {
    let release = resolve(request, backend).await?;
    let (n, row) = source_row(backend, release, id).await?;
    let details = details(backend, release, n).await?;
    let detail = SourceDetail {
        release,
        id: column(&row, "id")?,
        title: column(&row, "title")?,
        provider: column(&row, "provider")?,
        scale_domain: column(&row, "scale_domain")?,
        entries: column(&row, "entries")?,
        citation: column(&row, "citation")?,
        reference: details.as_ref().map(|d| d.reference.clone()),
        terms: Terms {
            url: column(&row, "terms_url")?,
            quote: column(&row, "terms_quote")?,
            credit: column(&row, "credit")?,
            redistribution: column(&row, "redistribution")?,
            review: column(&row, "review")?,
        },
        query: json_column(&row, "access")?,
        cuts: json_column(&row, "cuts")?,
        coverage: details.map(|d| d.coverage),
    };
    Ok(pinned(Response::json(&json!(detail)), release))
}

/// The row of source `id` in `release`, with its number, or a 404 naming it.
pub(crate) async fn source_row<B: Backend>(
    backend: &B,
    release: u32,
    id: &str,
) -> Result<(u32, Row), Problem> {
    let rows = backend
        .releases(
            "SELECT * FROM sources WHERE release = ?1 AND id = ?2",
            &[json!(release), json!(id)],
        )
        .await?;
    let row = first(rows).ok_or_else(|| {
        Problem::new(
            ProblemType::SourceNotFound,
            format!("Release {release} has no source {id:?}."),
        )
        .with("id", id)
        .with("release", release)
    })?;
    Ok((column(&row, "n")?, row))
}

/// What schema v3 records of a source beside its row.
struct Details {
    reference: Reference,
    coverage: Coverage,
}

/// The source's reference and coverage, or none in a release database from before schema v3.
async fn details<B: Backend>(
    backend: &B,
    release: u32,
    n: u32,
) -> Result<Option<Details>, Problem> {
    let rows = match backend
        .releases(
            "SELECT bibcode, doi, reference_note, sky, radial FROM source_details \
             WHERE release = ?1 AND source = ?2",
            &[json!(release), json!(n)],
        )
        .await
    {
        Ok(rows) => rows,
        Err(error) if error.is_missing_table() => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let Some(row) = first(rows) else {
        return Ok(None);
    };
    let sky: Option<String> = column(&row, "sky")?;
    let radial: Option<String> = column(&row, "radial")?;
    Ok(Some(Details {
        reference: Reference {
            bibcode: column(&row, "bibcode")?,
            doi: column(&row, "doi")?,
            note: column(&row, "reference_note")?,
        },
        coverage: Coverage {
            sky: sky.map(|text| parse(&text, "sky")).transpose()?,
            radial: radial
                .map(|text| parse(&text, "radial"))
                .transpose()?
                .unwrap_or_default(),
        },
    }))
}

fn parse<T: serde::de::DeserializeOwned>(text: &str, name: &str) -> Result<T, Problem> {
    serde_json::from_str(text).map_err(|error| {
        Problem::new(
            ProblemType::InternalError,
            format!("The release database's column {name} did not read: {error}."),
        )
    })
}

#[cfg(test)]
mod tests;
