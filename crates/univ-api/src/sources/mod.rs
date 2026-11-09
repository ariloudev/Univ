//! A release's sources (read-a-source).

pub(crate) mod provenance;

pub use provenance::provenance;

use serde_json::json;
use univ_model::api::sources::{
    Coverage, DesignationForm, Permission, Reference, SourceDetail, SourceList, Terms,
};
use univ_model::valid_in;

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
            concat!(
                "SELECT id, title, scale_domain, entries, citation, credit FROM sources WHERE ",
                valid_in!("", "?1"),
                " ORDER BY n"
            ),
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
            concat!(
                "SELECT s.id, s.title, s.provider, s.scale_domain, s.entries, s.citation, \
                 s.terms_url, s.terms_quote, s.credit, s.redistribution, s.review, s.access, \
                 s.cuts, d.source AS detailed, d.bibcode, d.doi, d.reference_note, d.sky, \
                 d.radial, t.non_commercial, t.permission, r.designation_form, \
                 r.designation_cite, r.share_alike, r.flags \
                 FROM sources s LEFT JOIN source_details d ON d.source = s.n AND ",
                valid_in!("d.", "?1"),
                " LEFT JOIN source_terms t ON t.source = s.n AND ",
                valid_in!("t.", "?1"),
                " LEFT JOIN source_reading r ON r.source = s.n AND ",
                valid_in!("r.", "?1"),
                " WHERE s.id = ?2 AND ",
                valid_in!("s.", "?1")
            ),
            &[json!(release), json!(id)],
        )
        .await?;
    let row = first(rows).ok_or_else(|| not_found(release, id))?;
    let detailed = column::<Option<u32>>(&row, "detailed")?.is_some();
    let (permission, permission_to_ask) = json_column::<Option<Permission>>(&row, "permission")?
        .map_or((None, None), Permission::served);
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
            share_alike: column(&row, "share_alike")?,
            permission,
            permission_to_ask,
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
        designation: match (
            column::<Option<String>>(&row, "designation_form")?,
            column::<Option<String>>(&row, "designation_cite")?,
        ) {
            (Some(form), Some(cite)) => Some(DesignationForm { form, cite }),
            _ => None,
        },
        flags: json_column::<Option<_>>(&row, "flags")?.unwrap_or_default(),
    };
    Ok(pinned(Response::json(&detail)?, release))
}

/// The 404 of a source that `release` does not hold.
pub(crate) fn not_found(release: u32, id: &str) -> Problem {
    Problem::not_found(ProblemType::SourceNotFound, "source", release, id)
}

#[cfg(test)]
mod tests;
