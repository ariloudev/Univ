//! A source's provenance (read-a-source): what was fetched, what normalization kept, left out
//! and fixed.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::json;
use univ_model::api::sources::{EntryCounts, Fix, Payload, SourceProvenance};

use super::source_row;
use crate::backend::{Backend, Row, column, first, json_column};
use crate::http::{Request, Response};
use crate::problem::Problem;
use crate::releases::{pinned, resolve};

/// A fix as the source definition states it.
#[derive(Deserialize)]
struct DefinedFix {
    defect: String,
    change: String,
}

/// A fix as normalization reports it applied.
#[derive(Deserialize)]
struct AppliedFix {
    change: String,
    entries_changed: u64,
}

/// Reads the provenance of one source of the release the request reads.
///
/// # Errors
///
/// A problem answer: the release refused, no source of that id (404), or the release database
/// unreadable.
#[utoipa::path(
    get,
    path = "/v1/sources/{id}/provenance",
    params(
        ("id" = String, Path, description = "The source's id"),
        ("release" = Option<u32>, Query, description = "The release to read; latest when absent")
    ),
    responses(
        (status = 200, description = "The source's provenance", body = SourceProvenance)
    )
)]
pub async fn provenance<B: Backend>(
    request: &Request,
    id: &str,
    backend: &B,
) -> Result<Response, Problem> {
    let release = resolve(request, backend).await?;
    let (n, source) = source_row(backend, release, id).await?;
    let payloads = backend
        .releases(
            "SELECT item, url, sha256, bytes, retrieved, rows FROM payloads \
             WHERE release = ?1 AND source = ?2 ORDER BY item",
            &[json!(release), json!(n)],
        )
        .await?
        .iter()
        .map(payload)
        .collect::<Result<Vec<_>, Problem>>()?;
    let normalization = first(
        backend
            .releases(
                "SELECT published, left_out, fixes FROM normalizations \
                 WHERE release = ?1 AND source = ?2",
                &[json!(release), json!(n)],
            )
            .await?,
    );
    let defined: Vec<DefinedFix> = json_column(&source, "fixes")?;
    let (counts, left_out, applied) = match &normalization {
        Some(row) => {
            let left_out: BTreeMap<String, u64> = json_column(row, "left_out")?;
            let published: u64 = column(row, "published")?;
            let dropped: u64 = left_out.values().sum();
            let counts = EntryCounts {
                fetched: published + dropped,
                published,
                left_out: dropped,
            };
            let applied: Vec<AppliedFix> = json_column(row, "fixes")?;
            (Some(counts), left_out, applied)
        }
        None => (None, BTreeMap::new(), Vec::new()),
    };
    let record = SourceProvenance {
        release,
        id: id.to_owned(),
        payloads,
        counts,
        left_out,
        fixes: fixes(defined, &applied),
    };
    Ok(pinned(Response::json(&json!(record)), release))
}

fn payload(row: &Row) -> Result<Payload, Problem> {
    Ok(Payload {
        item: column(row, "item")?,
        url: column(row, "url")?,
        sha256: column(row, "sha256")?,
        bytes: column(row, "bytes")?,
        retrieved: column(row, "retrieved")?,
        rows: column(row, "rows")?,
    })
}

/// Each fix the source definition states, with how many entries normalization changed by it:
/// a report's fix is matched to its definition by the change, which normalization copies.
fn fixes(defined: Vec<DefinedFix>, applied: &[AppliedFix]) -> Vec<Fix> {
    defined
        .into_iter()
        .map(|fix| {
            let entries_changed = applied
                .iter()
                .filter(|a| a.change == fix.change)
                .map(|a| a.entries_changed)
                .sum();
            Fix {
                reason: fix.defect,
                change: fix.change,
                entries_changed,
            }
        })
        .collect()
}
