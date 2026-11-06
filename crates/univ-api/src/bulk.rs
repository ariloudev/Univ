//! A release's scale domains, their tile manifests and their tiles (fetch-a-tile-manifest,
//! fetch-point-tiles). Manifests and tiles are served from R2 as Univ Ingest published them:
//! the Worker hands their objects to the runtime to stream, and never decodes them. R2 keeps
//! each file once, by its SHA-256 (`files/{sha256}`): a domain's row names its manifest's key,
//! and the manifest gives each tile's SHA-256, so a tile shared by releases is one object.
//!
//! A tile or manifest read pinned to a release never changes, so it is cacheable for a year
//! and immutable. Read unpinned, a tile is answered with a redirect to the same tile pinned to
//! the release `latest` names, which a cache must not keep; a manifest read unpinned is
//! answered, but must be revalidated.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, LazyLock};

use serde::Deserialize;
use serde_json::{Value, json};
use univ_model::api::bulk::{ScaleDomainList, ScaleDomainSummary, TileFormat};
use univ_model::geometry::Aabb;
use univ_model::release::valid_in;
use univ_model::tile::format::layout;
use univ_model::tile::manifest::{NodeIndex, TileManifest, TileManifestIndex};
use univ_model::units::scale_domain_unit;
use utoipa::{PartialSchema, ToSchema};

use crate::backend::{Backend, rows_as};
use crate::http::{Body, Request, Response};
use crate::problem::{Problem, ProblemType};
use crate::releases::{RELEASE_PARAM, pinned, pinned_file, resolve};

/// What is kept of a scale domain's tile manifest: its encoding version, each node's address
/// with its tile's SHA-256, and each node's cube.
pub struct TileNodes {
    version: u16,
    nodes: HashMap<String, String>,
    pub(crate) cubes: Vec<NodeCube>,
}

/// A node's place in its octree: its address, level and cube in its domain's unit (ICRS
/// about the barycentre), and whether it is a leaf, a node no node lies below.
pub struct NodeCube {
    pub address: String,
    pub level: u8,
    pub bounds: Aabb,
    pub leaf: bool,
}

/// A scale domain of a release, as the release database lists it.
#[derive(Deserialize)]
pub struct DomainRow {
    pub(crate) name: String,
    radius: f64,
    points: u64,
    nodes: u64,
    pub(crate) manifest_key: String,
}

/// The JSON Schema of a tile manifest, built once and whole: the schemas it refers to are its
/// own `$defs`, so it validates a manifest without the OpenAPI document beside it.
static MANIFEST_SCHEMA: LazyLock<Result<Value, String>> = LazyLock::new(manifest_schema);

fn manifest_schema() -> Result<Value, String> {
    let mut nested = Vec::new();
    <TileManifest as ToSchema>::schemas(&mut nested);
    let mut schema = serde_json::to_value(<TileManifest as PartialSchema>::schema())
        .map_err(|e| e.to_string())?;
    let defs = nested
        .into_iter()
        .map(|(name, nested)| Ok((name, serde_json::to_value(nested)?)))
        .collect::<Result<serde_json::Map<String, Value>, serde_json::Error>>()
        .map_err(|e| e.to_string())?;
    if let Value::Object(members) = &mut schema {
        members.insert("$defs".to_owned(), Value::Object(defs));
    }
    let text = schema
        .to_string()
        .replace("\"#/components/schemas/", "\"#/$defs/");
    serde_json::from_str(&text).map_err(|e| e.to_string())
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
    let domains = domains_of(backend, release)
        .await?
        .iter()
        .map(|d| ScaleDomainSummary {
            name: d.name.clone(),
            unit: scale_domain_unit(&d.name),
            radius: d.radius,
            points: d.points,
            nodes: d.nodes,
            manifest: format!("/v1/scale-domains/{}/manifest?release={release}", d.name),
        })
        .collect();
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
    let key = domain(backend, release, name).await?.manifest_key.clone();
    let response = Response::empty(200)
        .with_header("content-type", "application/json")
        .with_body(Body::Object(key));
    Ok(pinned_file(response, request, release))
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
    let manifest_key = domain(backend, release, name).await?.manifest_key.clone();
    let nodes = tile_nodes(backend, release, name, &manifest_key).await?;
    let Some(sha256) = nodes.nodes.get(address) else {
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
    let key = univ_model::release::keys::file(sha256);
    let response = Response::empty(200)
        .with_header("content-type", "application/octet-stream")
        .with_header("etag", &format!("\"{sha256}\""))
        .with_body(Body::Object(key));
    Ok(pinned_file(response, request, release))
}

/// Describes how to decode the tiles of the release the request reads.
///
/// # Errors
///
/// A problem answer: the release refused, an encoding version this server cannot describe
/// (500), or the release unreadable.
#[utoipa::path(
    get,
    path = "/v1/tile-format",
    params(("release" = Option<u32>, Query, description = "The release to read; latest when absent")),
    responses((status = 200, description = "The byte layout of each encoding the release's tiles use, and the tile manifest's schema", body = TileFormat))
)]
pub async fn tile_format<B: Backend>(request: &Request, backend: &B) -> Result<Response, Problem> {
    let release = resolve(request, backend).await?;
    let mut versions = BTreeSet::new();
    for d in domains_of(backend, release).await?.iter() {
        versions.insert(
            tile_nodes(backend, release, &d.name, &d.manifest_key)
                .await?
                .version,
        );
    }
    let encodings = versions
        .into_iter()
        .map(|version| {
            layout(u32::from(version)).ok_or_else(|| {
                Problem::new(
                    ProblemType::InternalError,
                    format!("Release {release} uses tile encoding {version}, which this server cannot describe."),
                )
            })
        })
        .collect::<Result<Vec<_>, Problem>>()?;
    let manifest_schema = MANIFEST_SCHEMA.clone().map_err(|error| {
        Problem::new(
            ProblemType::InternalError,
            format!("The tile manifest's schema did not serialize: {error}."),
        )
    })?;
    let format = TileFormat {
        release,
        encodings,
        manifest_schema,
    };
    Ok(pinned_file(Response::json(&format)?, request, release))
}

/// The scale domains of `release`, by name, read once and kept.
pub(crate) async fn domains_of<B: Backend>(
    backend: &B,
    release: u32,
) -> Result<Arc<Vec<DomainRow>>, Problem> {
    if let Some(domains) = backend.kept().domains.get(&release) {
        return Ok(domains);
    }
    let rows = backend
        .releases(
            &format!(
                "SELECT name, radius, points, nodes, manifest_key FROM scale_domains \
                 WHERE {} ORDER BY name",
                valid_in("", "?1")
            ),
            &[json!(release)],
        )
        .await?;
    Ok(backend.kept().domains.keep(release, rows_as(rows)?))
}

/// Scale domain `name` of `release`, or a 404 that lists the domains the release has.
async fn domain<B: Backend>(backend: &B, release: u32, name: &str) -> Result<DomainRef, Problem> {
    let domains = domains_of(backend, release).await?;
    if let Some(index) = domains.iter().position(|d| d.name == name) {
        return Ok(DomainRef { domains, index });
    }
    let names: Vec<&str> = domains.iter().map(|d| d.name.as_str()).collect();
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

/// One scale domain among its release's, as kept.
struct DomainRef {
    domains: Arc<Vec<DomainRow>>,
    index: usize,
}

impl std::ops::Deref for DomainRef {
    type Target = DomainRow;

    fn deref(&self) -> &DomainRow {
        &self.domains[self.index]
    }
}

/// The nodes of the tile manifest of scale domain `name` of `release`, read once and kept.
pub(crate) async fn tile_nodes<B: Backend>(
    backend: &B,
    release: u32,
    name: &str,
    key: &str,
) -> Result<Arc<TileNodes>, Problem> {
    let kept_key = (release, name.to_owned());
    if let Some(nodes) = backend.kept().tile_nodes.get(&kept_key) {
        return Ok(nodes);
    }
    let bytes = backend.blob(key).await?.ok_or_else(|| {
        Problem::new(
            ProblemType::InternalError,
            format!("The releases bucket holds no {key}, which release {release} names."),
        )
    })?;
    let manifest: TileManifestIndex = serde_json::from_slice(&bytes).map_err(|error| {
        Problem::new(
            ProblemType::InternalError,
            format!("The tile manifest {key} did not read: {error}."),
        )
    })?;
    let parents: BTreeSet<&str> = manifest
        .nodes
        .iter()
        .filter_map(NodeIndex::parent)
        .collect();
    let cubes = manifest
        .nodes
        .iter()
        .map(|node| NodeCube {
            address: node.address.clone(),
            level: node.level,
            bounds: Aabb {
                min: node.bounds.min,
                max: node.bounds.max,
            },
            leaf: !parents.contains(node.address.as_str()),
        })
        .collect();
    let nodes = TileNodes {
        version: manifest.encoding.version,
        nodes: manifest
            .nodes
            .into_iter()
            .map(|node| (node.address, node.sha256))
            .collect(),
        cubes,
    };
    Ok(backend.kept().tile_nodes.keep(kept_key, nodes))
}

#[cfg(test)]
#[path = "bulk_tests.rs"]
mod tests;
