//! A release's sources (read-a-source).

pub(crate) mod provenance;

pub use provenance::provenance;

use serde_json::json;
use univ_model::api::sources::{
    Coverage, PermissionRequest, Reference, SourceDetail, SourceList, Terms,
};

use crate::backend::{Backend, column, first, json_column, rows_as};
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
    let listing = SourceList {
        release,
        sources: rows_as(rows)?,
    };
    Ok(pinned(Response::json(&listing)?, release))
}

/// Reads one source of the release the request reads, with what schema v3 records beside it:
/// its primary reference and its coverage, absent for a release built before schema v3.
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
    let rows = backend
        .releases(
            "SELECT s.id, s.title, s.provider, s.scale_domain, s.entries, s.citation, \
             s.terms_url, s.terms_quote, s.credit, s.redistribution, s.review, s.access, s.cuts, \
             d.source AS detailed, d.bibcode, d.doi, d.reference_note, d.sky, d.radial, \
             t.non_commercial, t.permission_asked_of, t.permission_asked_on \
             FROM sources s LEFT JOIN source_details d \
             ON d.release = s.release AND d.source = s.n \
             LEFT JOIN source_terms t ON t.release = s.release AND t.source = s.n \
             WHERE s.release = ?1 AND s.id = ?2",
            &[json!(release), json!(id)],
        )
        .await?;
    let row = first(rows).ok_or_else(|| not_found(release, id))?;
    let detailed = column::<Option<u32>>(&row, "detailed")?.is_some();
    let detail = SourceDetail {
        release,
        id: column(&row, "id")?,
        title: column(&row, "title")?,
        provider: column(&row, "provider")?,
        scale_domain: column(&row, "scale_domain")?,
        entries: column(&row, "entries")?,
        citation: column(&row, "citation")?,
        reference: if detailed {
            Some(Reference {
                bibcode: column(&row, "bibcode")?,
                doi: column(&row, "doi")?,
                note: column(&row, "reference_note")?,
            })
        } else {
            None
        },
        terms: Terms {
            url: column(&row, "terms_url")?,
            quote: column(&row, "terms_quote")?,
            credit: column(&row, "credit")?,
            redistribution: column(&row, "redistribution")?,
            review: column(&row, "review")?,
            non_commercial: column(&row, "non_commercial")?,
            permission: match (
                column::<Option<String>>(&row, "permission_asked_of")?,
                column::<Option<String>>(&row, "permission_asked_on")?,
            ) {
                (Some(who), Some(date)) => Some(PermissionRequest {
                    asked_of: who,
                    asked_on: date,
                }),
                _ => None,
            },
        },
        query: json_column(&row, "access")?,
        cuts: json_column(&row, "cuts")?,
        coverage: if detailed {
            Some(Coverage {
                sky: json_column(&row, "sky")?,
                radial: json_column::<Option<_>>(&row, "radial")?.unwrap_or_default(),
            })
        } else {
            None
        },
    };
    Ok(pinned(Response::json(&detail)?, release))
}

/// The 404 of a source that `release` does not hold.
pub(crate) fn not_found(release: u32, id: &str) -> Problem {
    Problem::not_found(ProblemType::SourceNotFound, "source", release, id)
}

#[cfg(test)]
mod tests;
