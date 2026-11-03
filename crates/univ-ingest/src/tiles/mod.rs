//! The octree tiles of a release (build-octree-tiles): each scale domain's placed objects laid
//! out as an octree of tiles over their derived ICRS Cartesian positions, each node's points in
//! the univ-model tile encoding, and a JSON tile manifest per domain.
//!
//! - **Domains.** An object belongs to the scale domain that the source definition of the entry
//!   holding its preferred distance names; stars are in parsecs, nearby galaxies and the cosmic
//!   web in megaparsecs. An object not placed is in no tile.
//! - **Root.** A cube centred on the barycentre, its half-edge the smallest power of two of the
//!   unit not below the largest |coordinate| of the domain's points.
//! - **Splits and levels.** A node receiving more points than the rules' split threshold, above
//!   the maximum depth, keeps an even random subsample of the rules' level points and passes the
//!   rest to its children by octant; any other node keeps all it receives. Each point is in one
//!   node only, so a finer level adds points and repeats none.
//! - **Seed.** The subsample is drawn by `ChaCha20` under the rules' seed, which every manifest
//!   records; the points are in increasing id and the nodes in depth-first order, so the same
//!   inputs and seed give byte-identical tiles and manifests.
//! - **Size.** A tile of `n` points is `48 + 17 n` bytes, at most 61,678 points within 1 MiB.
//!   The rules' split threshold and level points are checked against it when read, so only a
//!   leaf at the maximum depth could hold more, and such a build is refused naming the node,
//!   before anything is written; the encoder refuses one too.
//! - **Points.** Each point gives its object id, the manifest's index of the source of its
//!   preferred distance, and that measurement's kind of value. The manifest's source table also
//!   credits the sources of the points' sky positions.

mod domain;
mod manifest;
mod octree;
mod rules;
mod write;

use std::collections::{BTreeMap, BTreeSet};

use sha2::{Digest, Sha256};
use thiserror::Error;
use univ_model::cosmology::Cosmology;
use univ_model::ids::{ObjectId, SourceId};
use univ_model::position::Frame;
use univ_model::tile::{MAX_TILE_BYTES, NodeBox, TileError, TilePoint, encode};

pub use domain::{Assigned, DOMAINS, DomainPoint, assign, name_of, unit_of, unit_symbol};
pub use manifest::{
    Bounds, ENCODING, FORMAT, FRAME, Level, MANIFEST_VERSION, Manifest, NodeEntry, Rotations,
    Sampling, SourceEntry,
};
pub use octree::{Node, generator, root_box};
pub use rules::{DEEPEST, MAX_SEED, TileRules};
pub use write::{MANIFEST_FILE, tile_file, write_all};

use crate::crossmatch::ReleaseObject;
use crate::fetch::hex;
use crate::source::{ScaleDomain, SourceDefinition};

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
    #[error("univ-model gives no rotation from the ICRS to {0:?}")]
    NoRotation(Frame),
    #[error("the manifest cannot be written: {0}")]
    Manifest(String),
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
    /// The objects of its tile, in increasing id, the order the tile holds them in.
    pub ids: Vec<ObjectId>,
    pub tile: Vec<u8>,
    pub sha256: String,
}

/// A scale domain's tiles and manifest.
#[derive(Clone, Debug)]
pub struct BuiltDomain {
    pub domain: ScaleDomain,
    pub nodes: Vec<BuiltNode>,
    pub manifest: Manifest,
    /// The manifest as written.
    pub json: Vec<u8>,
}

impl BuiltDomain {
    /// The points of every tile together.
    #[must_use]
    pub fn points(&self) -> usize {
        self.manifest.points
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
    let assigned = assign(objects, context.definitions, context.cosmology)?;
    let domains = DOMAINS
        .iter()
        .map(|d| {
            let points = assigned.domains.get(d).map_or(&[][..], Vec::as_slice);
            build_domain(*d, points, context)
        })
        .collect::<Result<_, _>>()?;
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

/// The table of sources of `points`, both of their distances and of their sky positions, in
/// order of id, and the index of each.
fn source_table(
    domain: &str,
    points: &[DomainPoint],
    definitions: &[SourceDefinition],
) -> Result<(Vec<SourceEntry>, BTreeMap<SourceId, u16>), TilesError> {
    let mut used: BTreeMap<&SourceId, ObjectId> = BTreeMap::new();
    for p in points {
        used.entry(&p.source).or_insert(p.id);
        used.entry(&p.sky_source).or_insert(p.id);
    }
    let too_many = || TilesError::TooManySources {
        domain: domain.to_owned(),
        sources: used.len(),
    };
    let mut table = Vec::new();
    let mut index = BTreeMap::new();
    for (n, (source, id)) in used.iter().enumerate() {
        let d = definitions
            .iter()
            .find(|d| &d.id == *source)
            .ok_or_else(|| TilesError::UnknownSource {
                id: *id,
                source_id: (*source).clone(),
            })?;
        let i = u16::try_from(n).map_err(|_| too_many())?;
        index.insert((*source).clone(), i);
        table.push(SourceEntry {
            index: i,
            id: d.id.to_string(),
            title: d.title.clone(),
            citation: d.citation.clone(),
            credit: d.terms.credit.clone(),
        });
    }
    Ok((table, index))
}

/// Builds one scale domain's octree, tiles and manifest from its `points`, in increasing id.
///
/// # Errors
///
/// Fails when a leaf at the maximum depth would hold a tile beyond 1 MiB, naming it, or a
/// point's source has no definition.
pub fn build_domain(
    domain: ScaleDomain,
    points: &[DomainPoint],
    context: &Context<'_>,
) -> Result<BuiltDomain, TilesError> {
    let name = name_of(domain);
    let stream = DOMAINS
        .iter()
        .position(|d| *d == domain)
        .and_then(|n| u64::try_from(n).ok())
        .unwrap_or_default();
    let positions: Vec<_> = points.iter().map(|p| p.position).collect();
    let encode_error = |address: &str, error| TilesError::Encode {
        domain: name.to_owned(),
        address: address.to_owned(),
        error,
    };
    let root = root_box(&positions).map_err(|e| encode_error("r", e))?;
    let mut rng = generator(context.rules.seed, stream);
    let nodes = octree::build(name, &positions, root, context.rules, &mut rng)?;
    let (sources, index) = source_table(name, points, context.definitions)?;

    let mut built = Vec::with_capacity(nodes.len());
    for node in nodes {
        let tile_points: Vec<TilePoint> = node
            .points
            .iter()
            .map(|&i| {
                let p = &points[i];
                TilePoint {
                    position: p.position,
                    id: p.id,
                    source: index.get(&p.source).copied().unwrap_or_default(),
                    kind: p.kind,
                }
            })
            .collect();
        let tile =
            encode(&node.bounds, &tile_points).map_err(|e| encode_error(&node.address, e))?;
        built.push(BuiltNode {
            sha256: sha256(&tile),
            ids: tile_points.iter().map(|p| p.id).collect(),
            address: node.address,
            level: node.level,
            bounds: node.bounds,
            sampled_from: node.sampled_from,
            tile,
        });
    }
    let manifest = manifest_of(domain, stream, root, points, sources, &built, context)?;
    let json = manifest.to_json()?;
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
    root: NodeBox,
    points: &[DomainPoint],
    sources: Vec<SourceEntry>,
    nodes: &[BuiltNode],
    context: &Context<'_>,
) -> Result<Manifest, TilesError> {
    let levels: BTreeSet<u8> = nodes.iter().map(|n| n.level).collect();
    let levels = levels
        .into_iter()
        .map(|level| {
            let edge = root.edge() / f64::from(1u32 << level.min(31));
            let step = NodeBox::new([0.0; 3], edge).map_or(0.0, |b| b.step());
            Level { level, edge, step }
        })
        .collect();
    let radius = points
        .iter()
        .map(|p| p.position.iter().map(|c| c * c).sum::<f64>().sqrt())
        .fold(0.0_f64, f64::max);
    Ok(Manifest {
        format: FORMAT,
        manifest_version: MANIFEST_VERSION,
        release: context.release.to_owned(),
        scale_domain: name_of(domain),
        frame: FRAME,
        unit: unit_symbol(domain),
        cosmology: context.cosmology.clone(),
        rotations: Rotations::from_model()?,
        encoding: ENCODING,
        levels,
        sampling: Sampling::new(context.rules, stream),
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
                points: n.ids.len(),
                sampled_from: n.sampled_from,
                bytes: n.tile.len(),
                sha256: n.sha256.clone(),
            })
            .collect(),
    })
}
