//! A release's volume grids (fetch-a-volume-grid): listed from the release database, and their
//! headers and samples served from R2 as published, never decoded by the Worker. A header
//! names its quantity, kind of value, frame, axis order, shape, origin, cell size (rescaled
//! where it is in h⁻¹ Mpc), label table, citation and release; samples are little-endian
//! 32-bit floats in the header's axis order.

use std::sync::Arc;

use serde::Deserialize;
use serde_json::json;
use univ_model::api::bulk::{GridField, GridList, GridSummary};
use univ_model::units::Unit;

use crate::backend::{Backend, Row, column, first, json_column};
use crate::http::{Body, Request, Response};
use crate::problem::{Problem, ProblemType};
use crate::releases::{pinned, pinned_file, resolve};

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
    let mut grids = Vec::with_capacity(rows.len());
    for row in &rows {
        let id: String = column(row, "id")?;
        let keys = grid_keys(backend, release, &id).await?;
        grids.push(summary(row, release, &keys)?);
    }
    Ok(pinned(
        Response::json(&GridList { release, grids })?,
        release,
    ))
}

fn summary(row: &Row, release: u32, keys: &GridKeys) -> Result<GridSummary, Problem> {
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
                uncertainty: keys
                    .fields
                    .get(index as usize)
                    .and_then(|keys| keys.uncertainty.as_ref())
                    .map(|_| {
                        format!("/v1/grids/{id}/fields/{index}/uncertainty?release={release}")
                    }),
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
    let key = grid_keys(backend, release, id).await?.header.clone();
    let response = Response::empty(200)
        .with_header("content-type", "application/json")
        .with_body(Body::Object(key));
    Ok(pinned_file(response, request, release))
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
    let keys = grid_keys(backend, release, id).await?;
    let key = field_keys(&keys, release, id, field)?.samples.clone();
    let response = Response::empty(200)
        .with_header("content-type", "application/octet-stream")
        .with_body(Body::Object(key));
    Ok(pinned_file(response, request, release))
}

/// Serves the uncertainty of one field of a volume grid as published: one little-endian
/// 32-bit float per sample, of the form the header names (such as a standard deviation).
///
/// # Errors
///
/// A problem answer: the release refused, a grid or field the release does not have, or a
/// field published without an uncertainty (404, naming it), or the release unreadable.
#[utoipa::path(
    get,
    path = "/v1/grids/{id}/fields/{field}/uncertainty",
    params(
        ("id" = String, Path, description = "The grid's id"),
        ("field" = u32, Path, description = "The field's index, from 0"),
        ("release" = Option<u32>, Query, description = "The release to read; latest when absent")
    ),
    responses((status = 200, description = "The field's uncertainty samples, in the header's axis order", content_type = "application/octet-stream"))
)]
pub async fn uncertainty<B: Backend>(
    request: &Request,
    id: &str,
    field: &str,
    backend: &B,
) -> Result<Response, Problem> {
    let release = resolve(request, backend).await?;
    let keys = grid_keys(backend, release, id).await?;
    let Some(key) = field_keys(&keys, release, id, field)?.uncertainty.clone() else {
        return Err(Problem::new(
            ProblemType::GridFieldNotFound,
            format!("Field {field} of grid {id:?} was published with no uncertainty."),
        )
        .with("id", id)
        .with("field", field)
        .with("release", release));
    };
    let response = Response::empty(200)
        .with_header("content-type", "application/octet-stream")
        .with_body(Body::Object(key));
    Ok(pinned_file(response, request, release))
}

/// Where a grid's files are, as its header names them.
pub struct GridKeys {
    header: String,
    fields: Vec<FieldKeys>,
}

/// Where a field's samples, and its uncertainty's where published, are.
#[derive(serde::Deserialize)]
struct FieldKeys {
    samples: String,
    #[serde(default, deserialize_with = "uncertainty_samples")]
    uncertainty: Option<String>,
}

/// The samples key of a field's uncertainty, which the header gives beside its form.
fn uncertainty_samples<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    #[derive(serde::Deserialize)]
    struct Uncertainty {
        samples: Option<String>,
    }
    Ok(Option::<Uncertainty>::deserialize(deserializer)?.and_then(|u| u.samples))
}

#[derive(serde::Deserialize)]
struct HeaderFields {
    fields: Vec<FieldKeys>,
}

/// The keys of field `field` (its index) of a grid, or a 404 naming it.
fn field_keys<'k>(
    keys: &'k GridKeys,
    release: u32,
    id: &str,
    field: &str,
) -> Result<&'k FieldKeys, Problem> {
    field
        .parse::<usize>()
        .ok()
        .and_then(|index| keys.fields.get(index))
        .ok_or_else(|| {
            Problem::new(
                ProblemType::GridFieldNotFound,
                format!(
                    "Grid {id:?} of release {release} has no field {field:?}; it has {}, from 0.",
                    keys.fields.len()
                ),
            )
            .with("id", id)
            .with("field", field)
            .with("release", release)
        })
}

/// Where the files of grid `id` of `release` are, read once from its header and kept, or a
/// 404 naming it.
async fn grid_keys<B: Backend>(
    backend: &B,
    release: u32,
    id: &str,
) -> Result<Arc<GridKeys>, Problem> {
    let kept_key = (release, id.to_owned());
    if let Some(keys) = backend.kept().grids.get(&kept_key) {
        return Ok(keys);
    }
    let rows = backend
        .releases(
            "SELECT header_key FROM grids WHERE release = ?1 AND id = ?2",
            &[json!(release), json!(id)],
        )
        .await?;
    let row = first(rows)
        .ok_or_else(|| Problem::not_found(ProblemType::GridNotFound, "volume grid", release, id))?;
    let header: String = column(&row, "header_key")?;
    let bytes = backend.blob(&header).await?.ok_or_else(|| {
        Problem::new(
            ProblemType::InternalError,
            format!("The releases bucket holds no {header}, which release {release} names."),
        )
    })?;
    let fields: HeaderFields = serde_json::from_slice(&bytes).map_err(|error| {
        Problem::new(
            ProblemType::InternalError,
            format!("The grid header {header} did not read: {error}."),
        )
    })?;
    let keys = GridKeys {
        header,
        fields: fields.fields,
    };
    Ok(backend.kept().grids.keep(kept_key, keys))
}
