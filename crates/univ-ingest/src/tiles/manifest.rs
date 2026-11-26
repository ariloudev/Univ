//! A scale domain's tile manifest: univ-model's [`TileManifest`], whose module documents it, and
//! what only a build knows, the statement of how its subsample was drawn.

use std::borrow::Cow;
use std::collections::BTreeMap;

pub use univ_model::tile::manifest::{
    Bounds, ENCODING, Encoding, FORMAT, FRAME, FrameInfo, Level, MANIFEST_VERSION, MODEL_ENCODING,
    NodeEntry, PopulationCode, Rotations, Sampling, SourceEntry, SourceRole, TRACERS_ENCODING,
    TileManifest,
};

use univ_model::tile::NodeBox;

use super::domain::{name_of, unit_symbol};
use super::rules::TileRules;
use super::{BuiltNode, Context, TilesError};
use crate::source::ScaleDomain;

/// How the subsample is drawn, as every manifest states it.
pub const GENERATOR: &str = "ChaCha20, key the seed's 8 little-endian bytes then 24 zero bytes, \
                             stream the domain's number; level by level from the root, the nodes \
                             of a level in order of address, an inner node of n points keeps \
                             floor(level_points * n / m) of them, m the most points reaching any \
                             inner node of its level, drawn by a partial Fisher-Yates shuffle \
                             over its points in increasing id; a leaf keeps every point reaching \
                             it, the finest level of its region";

/// The sampling of a domain drawn under `rules` on `ChaCha20` stream `stream`.
#[must_use]
pub fn sampling(rules: &TileRules, stream: u64) -> Sampling {
    Sampling {
        seed: rules.seed,
        model_seed: None,
        generator: Cow::Borrowed(GENERATOR),
        stream,
        split_threshold: rules.split_threshold,
        level_points: rules.level_points,
        max_depth: rules.max_depth,
        statement: rules.statement.clone(),
    }
}

/// The rotations univ-model derives every galactic and supergalactic position with.
///
/// # Errors
///
/// Fails only if univ-model has no rotation between the frames, which it always has.
pub fn rotations() -> Result<Rotations, TilesError> {
    Rotations::from_model().map_err(TilesError::NoRotation)
}

/// The manifest as written, two-space indented JSON ending with a newline.
///
/// # Errors
///
/// Fails only if `serde_json` cannot write it.
pub fn to_json(manifest: &TileManifest) -> Result<Vec<u8>, TilesError> {
    manifest
        .to_json()
        .map_err(|e| TilesError::Manifest(e.to_string()))
}

/// What a domain's manifest records beside its octree: its sources, its encoding and, for the
/// Galaxy model, the seed its points were sampled with.
pub(super) struct Laid {
    pub(super) sources: Vec<SourceEntry>,
    pub(super) encoding: Encoding,
    pub(super) model_seed: Option<u64>,
}

/// The manifest of a domain's built nodes, of `points` many points whose farthest lies `radius`
/// from the barycentre, recording what `laid` gives.
pub(super) fn manifest_of(
    domain: ScaleDomain,
    stream: u64,
    (points, radius): (usize, f64),
    laid: Laid,
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
    Ok(TileManifest {
        format: Cow::Borrowed(FORMAT),
        manifest_version: MANIFEST_VERSION,
        release: context.release.to_owned(),
        scale_domain: Cow::Borrowed(name_of(domain)),
        frame: FRAME,
        unit: Cow::Borrowed(unit_symbol(domain)),
        cosmology: context.cosmology.clone(),
        rotations: rotations()?,
        encoding: laid.encoding,
        levels,
        sampling: Sampling {
            model_seed: laid.model_seed,
            ..sampling(context.rules, stream)
        },
        points,
        radius,
        sources: laid.sources,
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
