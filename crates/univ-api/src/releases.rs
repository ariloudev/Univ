//! Releases (list-and-pin-releases): the listing of every release published, and the release a
//! `release-scoped` read answers from.

use serde_json::json;
use univ_model::api::releases::{ReleaseCounts, ReleaseList, ReleaseSummary};

use crate::backend::{Backend, Row, column, first, json_column};
use crate::http::{Request, Response};
use crate::problem::{Problem, ProblemType};

/// The header that names the release a response was answered from.
pub const RELEASE_HEADER: &str = "univ-release";

/// The query parameter that pins a read to a release.
pub const RELEASE_PARAM: &str = "release";

/// The release a `release-scoped` read answers from: the one its `release` parameter names,
/// or the one `latest` points to.
///
/// # Errors
///
/// A problem answer: a malformed number, a release never published (404), a pruned release
/// (410), or a backend failure.
pub async fn resolve<B: Backend>(request: &Request, backend: &B) -> Result<u32, Problem> {
    let Some(text) = request.param(RELEASE_PARAM) else {
        return latest(backend).await;
    };
    let number = text.parse::<u32>().ok().filter(|n| *n > 0).ok_or_else(|| {
        Problem::new(
            ProblemType::InvalidParameter,
            format!("The release must be a positive whole number, not {text:?}."),
        )
        .with("parameter", RELEASE_PARAM)
        .with("value", text.clone())
    })?;
    published(number, backend).await
}

/// `number`, when it names a release that was published and not pruned.
///
/// # Errors
///
/// A problem answer: a release never published (404), a pruned release (410), or a backend
/// failure.
pub async fn published<B: Backend>(number: u32, backend: &B) -> Result<u32, Problem> {
    let rows = backend
        .releases(
            "SELECT published_at, pruned_at FROM releases WHERE number = ?1",
            &[json!(number)],
        )
        .await?;
    let published = first(rows)
        .map(|row| -> Result<_, Problem> {
            Ok((
                column::<Option<String>>(&row, "published_at")?,
                column::<Option<String>>(&row, "pruned_at")?,
            ))
        })
        .transpose()?;
    match published {
        Some((Some(_), None)) => Ok(number),
        Some((Some(_), Some(pruned_at))) => Err(Problem::new(
            ProblemType::ReleasePruned,
            format!("Release {number} was pruned at {pruned_at}; its data is gone."),
        )
        .with("release", number)
        .with("pruned_at", pruned_at)),
        _ => Err(Problem::new(
            ProblemType::ReleaseNotFound,
            format!("No release {number} was ever published."),
        )
        .with("release", number)),
    }
}

/// The release `latest` points to.
async fn latest<B: Backend>(backend: &B) -> Result<u32, Problem> {
    let rows = backend
        .releases("SELECT release FROM latest WHERE one = 1", &[])
        .await?;
    match first(rows) {
        Some(row) => column(&row, "release"),
        None => Err(Problem::new(
            ProblemType::ReleaseNotFound,
            "No release has been published yet.",
        )),
    }
}

/// The response, marked as answered from `release`.
#[must_use]
pub fn pinned(response: Response, release: u32) -> Response {
    response.with_header(RELEASE_HEADER, &release.to_string())
}

/// Lists every release ever published, newest first, pruned ones included.
///
/// # Errors
///
/// A problem answer when the release database cannot be read.
#[utoipa::path(
    get,
    path = "/v1/releases",
    responses(
        (status = 200, description = "Every release published, newest first", body = ReleaseList)
    )
)]
pub async fn list<B: Backend>(backend: &B) -> Result<Response, Problem> {
    let rows = backend
        .releases(
            "SELECT number, published_at, pruned_at, cosmology, sources, objects, structures, \
             grids, tiles FROM releases WHERE published_at IS NOT NULL ORDER BY number DESC",
            &[],
        )
        .await?;
    let releases = rows
        .iter()
        .map(summary)
        .collect::<Result<Vec<_>, Problem>>()?;
    let listing = ReleaseList {
        latest: latest(backend).await?,
        releases,
    };
    Ok(Response::json(&json!(listing)))
}

fn summary(row: &Row) -> Result<ReleaseSummary, Problem> {
    Ok(ReleaseSummary {
        number: column(row, "number")?,
        published_at: column(row, "published_at")?,
        pruned_at: column(row, "pruned_at")?,
        cosmology: json_column(row, "cosmology")?,
        sources: json_column(row, "sources")?,
        counts: ReleaseCounts {
            objects: column(row, "objects")?,
            structures: column(row, "structures")?,
            grids: column(row, "grids")?,
            tiles: column(row, "tiles")?,
        },
    })
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;
    use crate::http::{Body, Method};
    use crate::testkit::{RELEASES, Sqlite, block_on};

    fn listing() -> Value {
        let db = Sqlite::new();
        db.exec(RELEASES);
        let response = db.answer(&Request::new(Method::Get, "/v1/releases"));
        assert_eq!(response.status, 200);
        let Body::Bytes(bytes) = response.body else {
            panic!("a listing has a body");
        };
        serde_json::from_slice(&bytes).unwrap()
    }

    fn pinned_to(query: &str) -> Result<u32, Problem> {
        let db = Sqlite::new();
        db.exec(RELEASES);
        let mut request = Request::new(Method::Get, "/v1/sources");
        query.clone_into(&mut request.query);
        block_on(resolve(&request, &db))
    }

    #[test]
    fn each_release_gives_its_number_publication_time_cosmology_and_sources() {
        let release = &listing()["releases"][0];
        assert_eq!(release["number"], 2);
        assert_eq!(release["published_at"], "2026-11-02T08:30:00Z");
        assert_eq!(release["cosmology"]["hubble_constant"], 67.66);
        assert_eq!(release["cosmology"]["omega_matter"], 0.3111);
        assert_eq!(
            release["sources"],
            serde_json::json!(["hipparcos-2007", "twomrs"])
        );
    }

    #[test]
    fn each_release_gives_its_counts_of_objects_structures_grids_and_tiles() {
        let counts = &listing()["releases"][0]["counts"];
        assert_eq!(
            counts,
            &serde_json::json!({"objects": 12, "structures": 2, "grids": 1, "tiles": 4})
        );
    }

    #[test]
    fn the_listing_names_the_release_latest_points_to() {
        assert_eq!(listing()["latest"], 2);
    }

    #[test]
    fn every_published_release_is_listed_newest_first_pruned_ones_with_their_pruning_time() {
        let listing = listing();
        let releases = listing["releases"].as_array().unwrap();
        let numbers: Vec<&Value> = releases.iter().map(|r| &r["number"]).collect();
        assert_eq!(numbers, [2, 1], "release 3 was never published");
        assert_eq!(releases[0]["pruned_at"], Value::Null);
        assert_eq!(releases[1]["pruned_at"], "2026-11-02T09:00:00Z");
    }

    #[test]
    fn a_read_that_names_no_release_answers_from_latest() {
        assert_eq!(pinned_to("").unwrap(), 2);
    }

    #[test]
    fn a_read_that_names_a_release_answers_from_it() {
        assert_eq!(pinned_to("release=2").unwrap(), 2);
    }

    #[test]
    fn a_pruned_release_is_refused_with_410_and_its_pruning_time() {
        let problem = pinned_to("release=1").unwrap_err();
        assert_eq!(problem.kind, ProblemType::ReleasePruned);
        assert_eq!(problem.kind.status(), 410);
        assert_eq!(problem.extensions["pruned_at"], "2026-11-02T09:00:00Z");
    }

    #[test]
    fn a_release_never_published_is_refused_with_404() {
        for query in ["release=3", "release=99"] {
            let problem = pinned_to(query).unwrap_err();
            assert_eq!(problem.kind, ProblemType::ReleaseNotFound, "{query}");
            assert_eq!(problem.kind.status(), 404);
        }
    }

    #[test]
    fn a_malformed_release_number_is_refused_naming_the_parameter() {
        for query in ["release=0", "release=-1", "release=six"] {
            let problem = pinned_to(query).unwrap_err();
            assert_eq!(problem.kind, ProblemType::InvalidParameter, "{query}");
            assert_eq!(problem.extensions["parameter"], "release");
        }
    }

    #[test]
    fn a_pinned_response_names_its_release() {
        let response = pinned(Response::empty(200), 2);
        assert_eq!(response.header("Univ-Release"), Some("2"));
    }
}
