//! A release's scale domains, their tile manifests and their tiles (fetch-a-tile-manifest,
//! fetch-point-tiles). Manifests and tiles are served from R2 as Univ Ingest published them:
//! the Worker hands their objects to the runtime to stream, and never decodes them.
//!
//! A tile or manifest read pinned to a release never changes, so it is cacheable for a year
//! and immutable. Read unpinned, a tile is answered with a redirect to the same tile pinned to
//! the release `latest` names, which a cache must not keep; a manifest read unpinned is
//! answered, but must be revalidated.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::json;
use univ_model::api::bulk::{ScaleDomainList, ScaleDomainSummary};
use univ_model::units::scale_domain_unit;

use crate::backend::{Backend, Row, column, first};
use crate::http::{Body, Request, Response};
use crate::problem::{Problem, ProblemType};
use crate::releases::{RELEASE_PARAM, pinned, resolve};

/// The `Cache-Control` of what a pinned read answers: a release's files never change.
pub const IMMUTABLE: &str = "public, max-age=31536000, immutable";

/// The nodes a scale domain's tile manifest holds: each address with its tile's SHA-256.
pub struct TileNodes(HashMap<String, String>);

/// What is read of a tile manifest to serve its tiles.
#[derive(Deserialize)]
struct ManifestNodes {
    nodes: Vec<ManifestNode>,
}

#[derive(Deserialize)]
struct ManifestNode {
    address: String,
    sha256: String,
}

/// Lists the scale domains of the release the request reads.
///
/// # Errors
///
/// A problem answer: the release refused, or the release database unreadable.
#[utoipa::path(
    get,
    path = "/v1/scale-domains",
    params(("release" = Option<u32>, Query, description = "The release to read; latest when absent")),
    responses((status = 200, description = "The scale domains of the release", body = ScaleDomainList))
)]
pub async fn domains<B: Backend>(request: &Request, backend: &B) -> Result<Response, Problem> {
    let release = resolve(request, backend).await?;
    let rows = domain_rows(backend, release).await?;
    let domains = rows
        .iter()
        .map(|row| {
            let name: String = column(row, "name")?;
            Ok(ScaleDomainSummary {
                unit: scale_domain_unit(&name),
                radius: column(row, "radius")?,
                points: column(row, "points")?,
                nodes: column(row, "nodes")?,
                manifest: format!("/v1/scale-domains/{name}/manifest?release={release}"),
                name,
            })
        })
        .collect::<Result<Vec<_>, Problem>>()?;
    Ok(pinned(
        Response::json(&ScaleDomainList { release, domains })?,
        release,
    ))
}

/// Serves a scale domain's tile manifest as published.
///
/// # Errors
///
/// A problem answer: the release refused, a scale domain the release does not have (404,
/// listing those it has), or the release database unreadable.
#[utoipa::path(
    get,
    path = "/v1/scale-domains/{name}/manifest",
    params(
        ("name" = String, Path, description = "The scale domain, such as cosmic_web"),
        ("release" = Option<u32>, Query, description = "The release to read; latest when absent")
    ),
    responses((status = 200, description = "The tile manifest, as Univ Ingest published it", content_type = "application/json"))
)]
pub async fn manifest<B: Backend>(
    request: &Request,
    name: &str,
    backend: &B,
) -> Result<Response, Problem> {
    let release = resolve(request, backend).await?;
    let key = manifest_key(backend, release, name).await?;
    let response = Response::empty(200)
        .with_header("content-type", "application/json")
        .with_body(Body::Object(key));
    Ok(pinned(cached(response, request), release))
}

/// Serves one tile of a scale domain, by its octree node's address, as published.
///
/// # Errors
///
/// A problem answer: the release refused, an unknown scale domain (404) or a node its manifest
/// does not hold (404, naming the address), or the release unreadable.
#[utoipa::path(
    get,
    path = "/v1/scale-domains/{name}/tiles/{address}",
    params(
        ("name" = String, Path, description = "The scale domain, such as cosmic_web"),
        ("address" = String, Path, description = "The octree node's address, as the manifest gives it"),
        ("release" = Option<u32>, Query, description = "The release to read; latest when absent")
    ),
    responses(
        (status = 200, description = "The tile's bytes, as published", content_type = "application/octet-stream"),
        (status = 302, description = "Read unpinned: the same tile pinned to the latest release")
    )
)]
pub async fn tile<B: Backend>(
    request: &Request,
    name: &str,
    address: &str,
    backend: &B,
) -> Result<Response, Problem> {
    let release = resolve(request, backend).await?;
    let nodes = tile_nodes(backend, release, name).await?;
    let Some(sha256) = nodes.0.get(address) else {
        return Err(Problem::new(
            ProblemType::TileNotFound,
            format!("The {name} tile manifest of release {release} holds no node {address:?}."),
        )
        .with("scale_domain", name)
        .with("address", address)
        .with("release", release));
    };
    let href = format!("/v1/scale-domains/{name}/tiles/{address}?release={release}");
    if request.param(RELEASE_PARAM).is_none() {
        let redirect = Response::empty(302)
            .with_header("location", &href)
            .with_header("cache-control", "no-store");
        return Ok(pinned(redirect, release));
    }
    let key =
        univ_model::release::keys::key(release, &univ_model::release::keys::tile(name, address));
    let response = Response::empty(200)
        .with_header("content-type", "application/octet-stream")
        .with_header("cache-control", IMMUTABLE)
        .with_header("etag", &format!("\"{sha256}\""))
        .with_body(Body::Object(key));
    Ok(pinned(response, release))
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

/// The scale domains of `release`, by name.
async fn domain_rows<B: Backend>(backend: &B, release: u32) -> Result<Vec<Row>, Problem> {
    Ok(backend
        .releases(
            "SELECT name, radius, points, nodes, manifest_key FROM scale_domains \
             WHERE release = ?1 ORDER BY name",
            &[json!(release)],
        )
        .await?)
}

/// The key of the tile manifest of scale domain `name` of `release`, or a 404 that lists the
/// domains the release has.
async fn manifest_key<B: Backend>(
    backend: &B,
    release: u32,
    name: &str,
) -> Result<String, Problem> {
    let rows = backend
        .releases(
            "SELECT manifest_key FROM scale_domains WHERE release = ?1 AND name = ?2",
            &[json!(release), json!(name)],
        )
        .await?;
    if let Some(row) = first(rows) {
        return column(&row, "manifest_key");
    }
    let names = domain_rows(backend, release)
        .await?
        .iter()
        .map(|row| column::<String>(row, "name"))
        .collect::<Result<Vec<_>, Problem>>()?;
    Err(Problem::new(
        ProblemType::ScaleDomainNotFound,
        format!(
            "Release {release} has no scale domain {name:?}; it has {}.",
            names.join(", ")
        ),
    )
    .with("scale_domain", name)
    .with("release", release)
    .with("scale_domains", names))
}

/// The nodes of the tile manifest of scale domain `name` of `release`, read once and kept.
async fn tile_nodes<B: Backend>(
    backend: &B,
    release: u32,
    name: &str,
) -> Result<std::sync::Arc<TileNodes>, Problem> {
    let kept_key = (release, name.to_owned());
    if let Some(nodes) = backend.kept().tile_nodes.get(&kept_key) {
        return Ok(nodes);
    }
    let key = manifest_key(backend, release, name).await?;
    let bytes = backend.blob(&key).await?.ok_or_else(|| {
        Problem::new(
            ProblemType::InternalError,
            format!("The releases bucket holds no {key}, which release {release} names."),
        )
    })?;
    let manifest: ManifestNodes = serde_json::from_slice(&bytes).map_err(|error| {
        Problem::new(
            ProblemType::InternalError,
            format!("The tile manifest {key} did not read: {error}."),
        )
    })?;
    let nodes = TileNodes(
        manifest
            .nodes
            .into_iter()
            .map(|node| (node.address, node.sha256))
            .collect(),
    );
    Ok(backend.kept().tile_nodes.keep(kept_key, nodes))
}

#[cfg(test)]
#[path = "bulk_tests.rs"]
mod tests;
