//! A release's structures (read-a-structure), listed by kind and scale domain and read one by
//! one with every number cited.
//!
//! A listing comes in pages of [`PAGE`] structures ordered by id. Its cursor names the release
//! it reads and the last id given, so every page of one listing reads the same release even
//! when `latest` moves between them.

use serde_json::{Value, json};
use univ_model::api::structures::{Centre, StructureDetail, StructurePage, StructureSummary};

use crate::backend::{Backend, BackendError, Row, column, first};
use crate::http::{Request, Response};
use crate::problem::{Problem, ProblemType};
use crate::releases::{pinned, published, resolve};

/// The most structures a page of a listing holds.
pub const PAGE: usize = 500;

/// The frame and origin every placed position of a release is in, as the tile manifests name
/// them.
const FRAME: &str = "ICRS equatorial Cartesian";
const ORIGIN: &str = "Solar System barycentre";

/// Lists the structures of the release the request reads, of the kinds and scale domains it
/// names (each a comma-separated list; all when absent).
///
/// # Errors
///
/// A problem answer: the release refused, a malformed cursor, or the release database
/// unreadable.
#[utoipa::path(
    get,
    path = "/v1/structures",
    params(
        ("kind" = Option<String>, Query, description = "Kinds to list, comma-separated"),
        ("scale_domain" = Option<String>, Query, description = "Scale domains to list, comma-separated"),
        ("cursor" = Option<String>, Query, description = "The cursor of the page to read, from the previous page"),
        ("release" = Option<u32>, Query, description = "The release to read; latest when absent")
    ),
    responses(
        (status = 200, description = "A page of structures, by id", body = StructurePage)
    )
)]
pub async fn list<B: Backend>(request: &Request, backend: &B) -> Result<Response, Problem> {
    let (release, after) = match request.param("cursor") {
        Some(cursor) => {
            let (release, after) = parse_cursor(&cursor)?;
            (published(release, backend).await?, after)
        }
        None => (resolve(request, backend).await?, String::new()),
    };
    let kinds = list_param(request, "kind");
    let domains = list_param(request, "scale_domain");
    let limit = PAGE + 1;
    let rows = backend
        .releases(
            "SELECT s.id, s.name, s.kind, s.scale_domain, s.centre_x, s.centre_y, s.centre_z, \
             d.unit FROM structures s LEFT JOIN scale_domains d \
             ON d.release = s.release AND d.name = s.scale_domain \
             WHERE s.release = ?1 AND s.id > ?2 \
             AND (?3 IS NULL OR s.kind IN (SELECT value FROM json_each(?3))) \
             AND (?4 IS NULL OR s.scale_domain IN (SELECT value FROM json_each(?4))) \
             ORDER BY s.id LIMIT ?5",
            &[json!(release), json!(after), kinds, domains, json!(limit)],
        )
        .await?;
    let more = rows.len() > PAGE;
    let structures = rows
        .iter()
        .take(PAGE)
        .map(summary)
        .collect::<Result<Vec<_>, Problem>>()?;
    let next = more
        .then(|| {
            structures
                .last()
                .map(|last| format!("{release}:{}", last.id))
        })
        .flatten();
    let page = StructurePage {
        release,
        structures,
        next,
    };
    Ok(pinned(Response::json(&json!(page)), release))
}

/// Reads one structure of the release the request reads.
///
/// # Errors
///
/// A problem answer: the release refused, no structure of that id (404), or the release
/// database unreadable.
#[utoipa::path(
    get,
    path = "/v1/structures/{id}",
    params(
        ("id" = String, Path, description = "The structure's id"),
        ("release" = Option<u32>, Query, description = "The release to read; latest when absent")
    ),
    responses(
        (status = 200, description = "The structure", body = StructureDetail)
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
            "SELECT s.*, d.unit FROM structures s LEFT JOIN scale_domains d \
             ON d.release = s.release AND d.name = s.scale_domain \
             WHERE s.release = ?1 AND s.id = ?2",
            &[json!(release), json!(id)],
        )
        .await?;
    let row = first(rows).ok_or_else(|| {
        Problem::new(
            ProblemType::StructureNotFound,
            format!("Release {release} has no structure {id:?}."),
        )
        .with("id", id)
        .with("release", release)
    })?;
    let details = details(backend, release, id).await?;
    let detail = StructureDetail {
        release,
        id: column(&row, "id")?,
        name: column(&row, "name")?,
        kind: column(&row, "kind")?,
        status: column(&row, "status")?,
        scale_domain: column(&row, "scale_domain")?,
        record: json_text(&row, "record")?.unwrap_or(Value::Null),
        centre: centre(&row)?,
        bounds: json_text(&row, "bounds")?,
        shape: json_text(&details, "shape")?,
        derivation: json_text(&details, "derivation")?,
        members: json_text(&row, "members")?,
        outline: json_text(&details, "outline")?,
        rescaled: match json_text(&details, "rescaled")? {
            Some(Value::Array(items)) => items,
            _ => Vec::new(),
        },
    };
    Ok(pinned(Response::json(&json!(detail)), release))
}

/// What schema v3 records of a structure beside its row: an empty row in a release database
/// from before it, or for a structure it has nothing on.
async fn details<B: Backend>(backend: &B, release: u32, id: &str) -> Result<Row, Problem> {
    let read = backend
        .releases(
            "SELECT shape, derivation, outline, rescaled FROM structure_details \
             WHERE release = ?1 AND id = ?2",
            &[json!(release), json!(id)],
        )
        .await;
    match read {
        Ok(rows) => Ok(first(rows).unwrap_or_default()),
        Err(error) if BackendError::is_missing_table(&error) => Ok(Row::new()),
        Err(error) => Err(error.into()),
    }
}

fn summary(row: &Row) -> Result<StructureSummary, Problem> {
    Ok(StructureSummary {
        id: column(row, "id")?,
        name: column(row, "name")?,
        kind: column(row, "kind")?,
        scale_domain: column(row, "scale_domain")?,
        centre: centre(row)?,
    })
}

/// The structure's placed centre, in its scale domain's unit, or none when it is not placed.
fn centre(row: &Row) -> Result<Option<Centre>, Problem> {
    let x: Option<f64> = column(row, "centre_x")?;
    let y: Option<f64> = column(row, "centre_y")?;
    let z: Option<f64> = column(row, "centre_z")?;
    let unit: Option<String> = column(row, "unit")?;
    Ok(match (x, y, z) {
        (Some(x), Some(y), Some(z)) => Some(Centre {
            frame: FRAME.to_owned(),
            origin: ORIGIN.to_owned(),
            unit: unit.unwrap_or_else(|| "Mpc".to_owned()),
            x,
            y,
            z,
        }),
        _ => None,
    })
}

/// The JSON document a text column holds, or none when it is NULL or absent.
fn json_text(row: &Row, name: &str) -> Result<Option<Value>, Problem> {
    let text: Option<String> = column(row, name)?;
    text.map(|text| {
        serde_json::from_str(&text).map_err(|error| {
            Problem::new(
                ProblemType::InternalError,
                format!("The release database's column {name} did not read: {error}."),
            )
        })
    })
    .transpose()
}

/// The values of a comma-separated query parameter, as a JSON array for `json_each`, or null
/// when the parameter is absent.
fn list_param(request: &Request, name: &str) -> Value {
    request.param(name).map_or(Value::Null, |text| {
        json!(
            text.split(',')
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .collect::<Vec<_>>()
        )
    })
}

/// The release and the last id a cursor names, or a problem naming the cursor.
fn parse_cursor(cursor: &str) -> Result<(u32, String), Problem> {
    cursor
        .split_once(':')
        .and_then(|(release, id)| Some((release.parse().ok()?, id.to_owned())))
        .filter(|(release, _): &(u32, String)| *release > 0)
        .ok_or_else(|| {
            Problem::new(
                ProblemType::InvalidParameter,
                format!("The cursor {cursor:?} is not one a page of structures gave."),
            )
            .with("parameter", "cursor")
            .with("value", cursor)
        })
}

#[cfg(test)]
#[path = "structures_tests.rs"]
mod tests;
