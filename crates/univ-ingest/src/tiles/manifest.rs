//! A scale domain's tile manifest: univ-model's [`TileManifest`], whose module documents it, and
//! what only a build knows, the statement of how its subsample was drawn.

use std::borrow::Cow;

pub use univ_model::tile::manifest::{
    Bounds, ENCODING, Encoding, FORMAT, FRAME, FrameInfo, Level, MANIFEST_VERSION, NodeEntry,
    Rotations, Sampling, SourceEntry, SourceRole, TRACERS_ENCODING, TileManifest,
};

use super::TilesError;
use super::rules::TileRules;

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
