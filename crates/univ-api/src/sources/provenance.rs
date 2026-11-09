//! A source's provenance (read-a-source): what was fetched, what normalization kept, left out
//! and fixed.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::json;
use univ_model::api::sources::{EntryCounts, Fix, SourceProvenance};
use univ_model::valid_in;

use crate::backend::{Backend, column, first, json_column, rows_as};
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
    let rows = backend
        .releases(
            concat!(
                "SELECT s.n, s.fixes, nm.published, nm.left_out, nm.fixes AS applied \
                 FROM sources s LEFT JOIN normalizations nm ON nm.source = s.n AND ",
                valid_in!("nm.", "?1"),
                " WHERE s.id = ?2 AND ",
                valid_in!("s.", "?1")
            ),
            &[json!(release), json!(id)],
        )
        .await?;
    let source = first(rows).ok_or_else(|| super::not_found(release, id))?;
    let n: u32 = column(&source, "n")?;
    let payloads = backend
        .releases(
            concat!(
                "SELECT item, url, sha256, bytes, retrieved, rows FROM payloads \
                 WHERE source = ?2 AND ",
                valid_in!("", "?1"),
                " ORDER BY item"
            ),
            &[json!(release), json!(n)],
        )
        .await?;
    let defined: Vec<DefinedFix> = json_column(&source, "fixes")?;
    let (counts, left_out, applied) = match column::<Option<u64>>(&source, "published")? {
        Some(published) => {
            let left_out: BTreeMap<String, u64> = json_column(&source, "left_out")?;
            let dropped: u64 = left_out.values().sum();
            let counts = EntryCounts {
                fetched: published + dropped,
                published,
                left_out: dropped,
            };
            let applied: Vec<AppliedFix> = json_column(&source, "applied")?;
            (Some(counts), left_out, applied)
        }
        None => (None, BTreeMap::new(), Vec::new()),
    };
    let record = SourceProvenance {
        release,
        id: id.to_owned(),
        payloads: rows_as(payloads)?,
        counts,
        left_out,
        fixes: fixes(defined, &applied),
    };
    Ok(pinned(Response::json(&record)?, release))
}

/// Each fix the source definition states, with how many entries normalization changed by it.
/// A report's fix is matched to its definition by the change, which normalization copies;
/// each reported fix is counted for one definition only, in order, so two definitions that
/// state the same change are not both credited with each other's entries.
fn fixes(defined: Vec<DefinedFix>, applied: &[AppliedFix]) -> Vec<Fix> {
    let mut used = vec![false; applied.len()];
    defined
        .into_iter()
        .map(|fix| {
            let found = applied
                .iter()
                .zip(used.iter_mut())
                .find(|(a, used)| !**used && a.change == fix.change);
            let entries_changed = found.map_or(0, |(a, used)| {
                *used = true;
                a.entries_changed
            });
            Fix {
                reason: fix.defect,
                change: fix.change,
                entries_changed,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_fixes_stating_the_same_change_are_each_credited_with_their_own_entries() {
        let defined = vec![
            DefinedFix {
                defect: "a".into(),
                change: "same".into(),
            },
            DefinedFix {
                defect: "b".into(),
                change: "same".into(),
            },
        ];
        let applied = [
            AppliedFix {
                change: "same".into(),
                entries_changed: 10,
            },
            AppliedFix {
                change: "same".into(),
                entries_changed: 5,
            },
        ];
        let fixes = fixes(defined, &applied);
        assert_eq!(fixes[0].entries_changed, 10);
        assert_eq!(fixes[1].entries_changed, 5);
    }
}
