//! A release's volume grids (fetch-a-volume-grid): listed from the release database, and their
//! headers and samples served from R2 as published, never decoded by the Worker. A header
//! names its quantity, kind of value, frame, axis order, shape, origin, cell size (rescaled
//! where it is in h⁻¹ Mpc), label table, citation and release; samples are little-endian
//! 32-bit floats in the header's axis order.

use serde_json::json;
use univ_model::api::bulk::{GridField, GridList, GridSummary};
use univ_model::release::keys;
use univ_model::units::Unit;

use crate::backend::{Backend, Row, column, first, json_column};
use crate::bulk::IMMUTABLE;
use crate::http::{Body, Request, Response};
use crate::problem::{Problem, ProblemType};
use crate::releases::{RELEASE_PARAM, pinned, resolve};

/// A field of a grid as the release database lists it.
#[derive(serde::Deserialize)]
struct FieldRow {
    field: String,
    kind: String,
    unit: Unit,
}

/// Lists the volume grids of the release the request reads.
///
/// # Errors
///
/// A problem answer: the release refused, or the release database unreadable.
#[utoipa::path(
    get,
    path = "/v1/grids",
    params(("release" = Option<u32>, Query, description = "The release to read; latest when absent")),
    responses((status = 200, description = "The volume grids of the release", body = GridList))
)]
pub async fn list<B: Backend>(request: &Request, backend: &B) -> Result<Response, Problem> {
    let release = resolve(request, backend).await?;
    let rows = backend
        .releases(
            "SELECT g.id, s.id AS source, g.unit, g.shape, g.fields FROM grids g \
             JOIN sources s ON s.release = g.release AND s.n = g.source \
             WHERE g.release = ?1 ORDER BY g.id",
            &[json!(release)],
        )
        .await?;
    let grids = rows
        .iter()
        .map(|row| summary(row, release))
        .collect::<Result<Vec<_>, Problem>>()?;
    Ok(pinned(
        Response::json(&GridList { release, grids })?,
        release,
    ))
}

fn summary(row: &Row, release: u32) -> Result<GridSummary, Problem> {
    let id: String = column(row, "id")?;
    let fields: Vec<FieldRow> = json_column(row, "fields")?;
    Ok(GridSummary {
        source: column(row, "source")?,
        unit: column(row, "unit")?,
        shape: json_column(row, "shape")?,
        fields: (0_u32..)
            .zip(fields)
            .map(|(index, f)| GridField {
                index,
                field: f.field,
                kind: f.kind,
                unit: f.unit,
                samples: format!("/v1/grids/{id}/fields/{index}?release={release}"),
            })
            .collect(),
        header: format!("/v1/grids/{id}?release={release}"),
        id,
    })
}

/// Serves a volume grid's header as published.
///
/// # Errors
///
/// A problem answer: the release refused, a grid the release does not have (404, naming it),
/// or the release database unreadable.
#[utoipa::path(
    get,
    path = "/v1/grids/{id}",
    params(
        ("id" = String, Path, description = "The grid's id"),
        ("release" = Option<u32>, Query, description = "The release to read; latest when absent")
    ),
    responses((status = 200, description = "The grid's header, as Univ Ingest published it", content_type = "application/json"))
)]
pub async fn header<B: Backend>(
    request: &Request,
    id: &str,
    backend: &B,
) -> Result<Response, Problem> {
    let release = resolve(request, backend).await?;
    let (key, _) = grid(backend, release, id).await?;
    let response = Response::empty(200)
        .with_header("content-type", "application/json")
        .with_body(Body::Object(key));
    Ok(pinned(cached(response, request), release))
}

/// Serves the samples of one field of a volume grid as published: little-endian 32-bit floats.
///
/// # Errors
///
/// A problem answer: the release refused, a grid or field the release does not have (404,
/// naming it), or the release database unreadable.
#[utoipa::path(
    get,
    path = "/v1/grids/{id}/fields/{field}",
    params(
        ("id" = String, Path, description = "The grid's id"),
        ("field" = u32, Path, description = "The field's index, from 0, as the listing and the header give it"),
        ("release" = Option<u32>, Query, description = "The release to read; latest when absent")
    ),
    responses((status = 200, description = "The field's samples: little-endian 32-bit floats in the header's axis order", content_type = "application/octet-stream"))
)]
pub async fn samples<B: Backend>(
    request: &Request,
    id: &str,
    field: &str,
    backend: &B,
) -> Result<Response, Problem> {
    let release = resolve(request, backend).await?;
    let (_, fields) = grid(backend, release, id).await?;
    let index = field
        .parse::<usize>()
        .ok()
        .filter(|index| *index < fields)
        .ok_or_else(|| {
            Problem::new(
                ProblemType::GridNotFound,
                format!("Grid {id:?} of release {release} has no field {field:?}; it has {fields}, from 0."),
            )
            .with("id", id)
            .with("field", field)
            .with("release", release)
        })?;
    let key = keys::key(release, &keys::grid_samples(id, &format!("field-{index}")));
    let response = Response::empty(200)
        .with_header("content-type", "application/octet-stream")
        .with_body(Body::Object(key));
    Ok(pinned(cached(response, request), release))
}

/// The header key and the number of fields of grid `id` of `release`, or a 404 naming it.
async fn grid<B: Backend>(backend: &B, release: u32, id: &str) -> Result<(String, usize), Problem> {
    let rows = backend
        .releases(
            "SELECT header_key, json_array_length(fields) AS fields FROM grids \
             WHERE release = ?1 AND id = ?2",
            &[json!(release), json!(id)],
        )
        .await?;
    let row = first(rows)
        .ok_or_else(|| Problem::not_found(ProblemType::GridNotFound, "volume grid", release, id))?;
    Ok((column(&row, "header_key")?, column(&row, "fields")?))
}

/// The response, cacheable for good when the request pinned its release, and revalidated on
/// every use when it did not.
fn cached(response: Response, request: &Request) -> Response {
    if request.param(RELEASE_PARAM).is_some() {
        response.with_header("cache-control", IMMUTABLE)
    } else {
        response.with_header("cache-control", "no-cache")
    }
}
