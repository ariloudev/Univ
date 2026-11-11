//! The octree tiles of a release (build-octree-tiles): each scale domain's placed objects laid
//! out as an octree of tiles over their derived ICRS Cartesian positions, each node's points in
//! the univ-model tile encoding, and a JSON tile manifest per domain.
//!
//! - **Domains.** An object belongs to the scale domain that the source definition of the entry
//!   holding its preferred distance names; stars are in parsecs, nearby galaxies and the cosmic
//!   web in megaparsecs. An object not placed is in no tile.
//! - **Root.** A cube centred on the barycentre, its half-edge the smallest power of two of the
//!   unit not below the largest |coordinate| of the domain's points.
//! - **Splits and levels.** The octree is laid out level by level. A node that more points
//!   than the rules' split threshold reach, above the maximum depth, is inner; any other node
//!   is a leaf. Each level has one sampling fraction, `f = level_points / m` with `m` the most
//!   points reaching any of its inner nodes, and an inner node of `n` points keeps an even
//!   random subsample of `floor(level_points × n / m)` of them, so no node keeps more than the
//!   level points and every inner node of a level shows the same fraction of its points,
//!   keeping their densities in proportion; it passes the rest to its children by octant. A
//!   leaf keeps every point reaching it: it is the finest level of its region. Each point is in
//!   one node only, so a finer level adds points and repeats none.
//! - **Seed.** The subsample is drawn by `ChaCha20` under the rules' seed, which every manifest
//!   records with each level's fraction; the points are in increasing id and the draws made
//!   level by level, nodes in order of address, so the same inputs and seed give
//!   byte-identical tiles and manifests.
//! - **Size.** A tile of `n` points is `48 + 17 n` bytes, at most 61,678 points within 1 MiB.
//!   The rules' split threshold and level points are checked against it when read, so only a
//!   leaf at the maximum depth could hold more, and such a build is refused naming the node,
//!   before anything is written; the encoder refuses one too.
//! - **Points.** Each point gives its object id, the manifest's index of the source of its
//!   preferred distance, and that measurement's kind of value. The manifest's source table
//!   also credits the sources of the points' sky positions, and gives each source's role:
//!   `distance`, `position` or `distance_and_position`.

mod domain;
mod manifest;
mod octree;
mod rules;
mod write;

use std::borrow::Cow;
use std::collections::BTreeMap;

use sha2::{Digest, Sha256};
use thiserror::Error;
use univ_model::cosmology::Cosmology;
use univ_model::ids::{ObjectId, SourceId};
use univ_model::position::Frame;
use univ_model::tile::{MAX_TILE_BYTES, NodeBox, TileError, TilePoint, encode};

pub use domain::{
    Assigned, DOMAINS, DomainPoint, DomainPoints, NEARBY_GALAXIES_DEPTH_MPC, assign, colour_of,
    domain_of, log_age_of, name_of, unit_of, unit_symbol,
};
pub use manifest::{
    Bounds, ENCODING, Encoding, FORMAT, FRAME, FrameInfo, GENERATOR, Level, MANIFEST_VERSION,
    NodeEntry, Rotations, Sampling, SourceEntry, SourceRole, TileManifest, rotations, sampling,
};
pub use octree::{Node, Octree, generator, root_box};
pub use rules::{DEEPEST, MAX_SEED, TileRules};
pub use write::{MANIFEST_FILE, tile_file, write_all};

use crate::crossmatch::ReleaseObject;
use crate::fetch::hex;
use crate::source::{ScaleDomain, SourceDefinition};
use crate::watch::Watch;

/// Tiles that cannot be built.
#[derive(Debug, Error, PartialEq)]
pub enum TilesError {
    #[error("object {0} is placed but has no preferred distance with a kind of value")]
    NoPreferred(ObjectId),
    #[error("object {id} takes a measurement of {source_id}, which has no source definition")]
    UnknownSource { id: ObjectId, source_id: SourceId },
    #[error("object {id}'s position was derived under {found}, not the release cosmology")]
    Cosmology { id: ObjectId, found: String },
    #[error("object {0}'s position is not finite")]
    NotFinite(ObjectId),
    #[error(
        "the {domain} node {address} lies at the maximum depth and holds {points} points, a tile \
         of {bytes} bytes, beyond the {MAX_TILE_BYTES} bytes (1 MiB) a tile may hold; no tile was \
         written. Raise max_depth in rules/tiles.yaml, or find why so many objects share one cell"
    )]
    TileTooLarge {
        domain: String,
        address: String,
        points: usize,
        bytes: usize,
    },
    #[error("the {domain} node {address} cannot be encoded: {error}")]
    Encode {
        domain: String,
        address: String,
        error: TileError,
    },
    #[error("the {domain} tiles draw on {sources} sources, more than a u16 index can name")]
    TooManySources { domain: String, sources: usize },
    #[error("the {domain} sources do not fit its points: {problem}")]
    SourceList { domain: String, problem: String },
    #[error("univ-model gives no rotation from the ICRS to {0:?}")]
    NoRotation(Frame),
    #[error("the manifest cannot be written: {0}")]
    Manifest(String),
    #[error("{0}")]
    Stopped(String),
}

/// What a build is made under.
#[derive(Clone, Copy, Debug)]
pub struct Context<'a> {
    /// The release the tiles belong to.
    pub release: &'a str,
    /// The release cosmology every position was derived under.
    pub cosmology: &'a Cosmology,
    pub rules: &'a TileRules,
    /// The source definitions, which name each source's domain and credit.
    pub definitions: &'a [SourceDefinition],
}

/// One node's tile.
#[derive(Clone, Debug, PartialEq)]
pub struct BuiltNode {
    pub address: String,
    pub level: u8,
    pub bounds: NodeBox,
    pub sampled_from: usize,
    /// The points its tile holds.
    pub points: usize,
    pub tile: Vec<u8>,
    pub sha256: String,
}

/// A scale domain's tiles and manifest.
#[derive(Clone, Debug)]
pub struct BuiltDomain {
    pub domain: ScaleDomain,
    pub nodes: Vec<BuiltNode>,
    pub manifest: TileManifest,
    /// The manifest as written.
    pub json: Vec<u8>,
}

impl BuiltDomain {
    /// The points of every tile together.
    #[must_use]
    pub fn points(&self) -> usize {
        self.manifest.points
    }

    /// What the domain came to, as a run's line gives it: `nearby galaxies: 1 nodes, 9 points,
    /// largest tile 201 bytes`.
    #[must_use]
    pub fn line(&self) -> String {
        format!(
            "{}: {} nodes, {} points, largest tile {} bytes",
            name_of(self.domain).replace('_', " "),
            self.nodes.len(),
            self.points(),
            self.largest_tile()
        )
    }

    /// The size of the largest tile, in bytes; 0 for a domain without points.
    #[must_use]
    pub fn largest_tile(&self) -> usize {
        self.nodes.iter().map(|n| n.tile.len()).max().unwrap_or(0)
    }
}

/// Every scale domain's tiles, in the order of [`DOMAINS`].
#[derive(Clone, Debug)]
pub struct Built {
    pub domains: Vec<BuiltDomain>,
    /// Objects without a derived position, in no tile.
    pub unplaced: usize,
}

/// Builds every scale domain's tiles from the release's `objects`, in memory.
///
/// # Errors
///
/// Fails when an object cannot be assigned a domain, or a tile would exceed 1 MiB.
pub fn build(objects: &[ReleaseObject], context: &Context<'_>) -> Result<Built, TilesError> {
    build_watched(objects, context, Watch::unwatched())
}

/// [`build`], telling `watch` of each domain as it is built, and stopping before the next domain
/// once the owner asks it to.
///
/// # Errors
///
/// Fails as [`build`] does, or when the owner asked it to stop.
pub fn build_watched(
    objects: &[ReleaseObject],
    context: &Context<'_>,
    watch: Watch<'_>,
) -> Result<Built, TilesError> {
    let assigned = assign(objects, context.definitions, context.cosmology)?;
    let none = DomainPoints::default();
    let mut domains = Vec::with_capacity(DOMAINS.len());
    for d in &DOMAINS {
        watch.check().map_err(TilesError::Stopped)?;
        let points = assigned.domains.get(d).unwrap_or(&none);
        let built = build_domain(*d, &points.points, &points.sources, context)?;
        watch.step(built.line());
        domains.push(built);
    }
    Ok(Built {
        domains,
        unplaced: assigned.unplaced,
    })
}

/// The SHA-256 of `bytes` in lowercase hexadecimal.
#[must_use]
pub fn sha256(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// The source table of a domain's `points`: its `sources`, each with the role it plays for
/// them.
///
/// # Errors
///
/// Fails on a source with no definition, a source no point names, or a point naming an index
/// beyond the sources.
fn source_table(
    domain: &str,
    points: &[DomainPoint],
    sources: &[SourceId],
    definitions: &[SourceDefinition],
) -> Result<Vec<SourceEntry>, TilesError> {
    let mismatch = |problem: String| TilesError::SourceList {
        domain: domain.to_owned(),
        problem,
    };
    // For each source, the first point taking its distance or its sky position from it.
    let mut uses: Vec<[Option<ObjectId>; 2]> = vec![[None; 2]; sources.len()];
    for p in points {
        for (role, index) in [p.source, p.sky_source].into_iter().enumerate() {
            let first = uses.get_mut(usize::from(index)).ok_or_else(|| {
                mismatch(format!(
                    "object {} names source {index} of {}",
                    p.id,
                    sources.len()
                ))
            })?;
            first[role].get_or_insert(p.id);
        }
    }
    let mut table = Vec::with_capacity(sources.len());
    for (n, (source, [distance, position])) in sources.iter().zip(uses).enumerate() {
        let index = u16::try_from(n).map_err(|_| TilesError::TooManySources {
            domain: domain.to_owned(),
            sources: sources.len(),
        })?;
        let (role, id) = match (distance, position) {
            (Some(id), Some(_)) => (SourceRole::DistanceAndPosition, id),
            (Some(id), None) => (SourceRole::Distance, id),
            (None, Some(id)) => (SourceRole::Position, id),
            (None, None) => return Err(mismatch(format!("no point names {source}"))),
        };
        let d = definitions
            .iter()
            .find(|d| &d.id == source)
            .ok_or_else(|| TilesError::UnknownSource {
                id,
                source_id: source.clone(),
            })?;
        table.push(SourceEntry {
            index,
            id: d.id.to_string(),
            role,
            title: d.title.clone(),
            citation: d.citation.clone(),
            credit: d.terms.credit.clone(),
        });
    }
    Ok(table)
}

/// Builds one scale domain's octree, tiles and manifest from its `points`, in increasing id,
/// and the `sources` their indices name.
///
/// # Errors
///
/// Fails when a leaf at the maximum depth would hold a tile beyond 1 MiB, naming it, or the
/// sources do not fit the points or have no definition.
pub fn build_domain(
    domain: ScaleDomain,
    points: &[DomainPoint],
    sources: &[SourceId],
    context: &Context<'_>,
) -> Result<BuiltDomain, TilesError> {
    let name = name_of(domain);
    let stream = DOMAINS
        .iter()
        .position(|d| *d == domain)
        .and_then(|n| u64::try_from(n).ok())
        .unwrap_or_default();
    let encode_error = |address: &str, error| TilesError::Encode {
        domain: name.to_owned(),
        address: address.to_owned(),
        error,
    };
    let sources = source_table(name, points, sources, context.definitions)?;
    let root = root_box(points).map_err(|e| encode_error("r", e))?;
    let mut rng = generator(context.rules.seed, stream);
    let tree = octree::build(name, points, root, context.rules, &mut rng)?;

    let mut built = Vec::with_capacity(tree.nodes.len());
    for node in tree.nodes {
        let tile_points: Vec<TilePoint> = node
            .points
            .iter()
            .map(|&i| {
                let p = &points[i];
                TilePoint {
                    position: p.position,
                    id: p.id,
                    source: p.source,
                    kind: p.kind,
                    value: p.value,
                }
            })
            .collect();
        let tile =
            encode(&node.bounds, &tile_points).map_err(|e| encode_error(&node.address, e))?;
        built.push(BuiltNode {
            sha256: sha256(&tile),
            points: tile_points.len(),
            address: node.address,
            level: node.level,
            bounds: node.bounds,
            sampled_from: node.sampled_from,
            tile,
        });
    }
    let manifest = manifest_of(
        domain,
        stream,
        points,
        sources,
        &tree.fractions,
        &built,
        context,
    )?;
    let json = manifest::to_json(&manifest)?;
    Ok(BuiltDomain {
        domain,
        nodes: built,
        manifest,
        json,
    })
}

/// The manifest of a domain's built nodes.
fn manifest_of(
    domain: ScaleDomain,
    stream: u64,
    points: &[DomainPoint],
    sources: Vec<SourceEntry>,
    fractions: &[f64],
    nodes: &[BuiltNode],
    context: &Context<'_>,
) -> Result<TileManifest, TilesError> {
    // The first node of each level, whose cube gives the level's edge and step.
    let mut first: BTreeMap<u8, &NodeBox> = BTreeMap::new();
    for n in nodes {
        first.entry(n.level).or_insert(&n.bounds);
    }
    let levels = first
        .into_iter()
        .map(|(level, bounds)| Level {
            level,
            edge: bounds.edge(),
            step: bounds.step(),
            fraction: fractions.get(usize::from(level)).copied().unwrap_or(1.0),
        })
        .collect();
    let radius = points
        .iter()
        .map(|p| p.position.iter().map(|c| c * c).sum::<f64>().sqrt())
        .fold(0.0_f64, f64::max);
    Ok(TileManifest {
        format: Cow::Borrowed(FORMAT),
        manifest_version: MANIFEST_VERSION,
        release: context.release.to_owned(),
        scale_domain: Cow::Borrowed(name_of(domain)),
        frame: FRAME,
        unit: Cow::Borrowed(unit_symbol(domain)),
        cosmology: context.cosmology.clone(),
        rotations: rotations()?,
        encoding: ENCODING,
        levels,
        sampling: sampling(context.rules, stream),
        points: points.len(),
        radius,
        sources,
        nodes: nodes
            .iter()
            .map(|n| NodeEntry {
                address: n.address.clone(),
                level: n.level,
                bounds: Bounds {
                    min: n.bounds.min(),
                    max: n.bounds.max(),
                },
                points: n.points,
                sampled_from: n.sampled_from,
                bytes: n.tile.len(),
                sha256: n.sha256.clone(),
            })
            .collect(),
    })
}
