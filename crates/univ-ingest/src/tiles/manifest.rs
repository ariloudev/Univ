//! A scale domain's tile manifest, the JSON a client reads before any tile (fetch-a-tile-manifest):
//! its frame, unit and cosmology, the rotations to galactic and supergalactic axes, the tile
//! encoding and the quantization step of each level, the seed and rules of the subsample, the
//! sources its points come from with their credit, and each node's address, bounds, level,
//! point count, byte size and SHA-256.
//!
//! Its numbers are written by `serde_json` as the shortest decimal that reads back to the same
//! `f64`, and every list is in a fixed order, so the same build gives the same bytes.

use serde::Serialize;
use univ_model::cosmology::Cosmology;
use univ_model::position::{Frame, Rotation, from_icrs};
use univ_model::tile::{HEADER_BYTES, MAX_TILE_BYTES, POINT_BYTES, STEPS, VERSION};

use super::TilesError;
use super::rules::TileRules;

/// The name a manifest gives its format.
pub const FORMAT: &str = "univ-tile-manifest";
/// The version of the manifest's own layout.
pub const MANIFEST_VERSION: u32 = 1;

/// A tile manifest, in the order its JSON gives its fields.
#[derive(Clone, Debug, Serialize)]
pub struct Manifest {
    pub format: &'static str,
    pub manifest_version: u32,
    pub release: String,
    pub scale_domain: &'static str,
    pub frame: FrameInfo,
    /// `pc` or `Mpc`.
    pub unit: &'static str,
    /// The release cosmology the positions were derived under.
    pub cosmology: Cosmology,
    pub rotations: Rotations,
    pub encoding: Encoding,
    /// Each level present, root first, with its node edge and quantization step.
    pub levels: Vec<Level>,
    pub sampling: Sampling,
    /// The points of every tile together.
    pub points: usize,
    /// The largest distance of a point from the barycentre, in the unit.
    pub radius: f64,
    pub sources: Vec<SourceEntry>,
    pub nodes: Vec<NodeEntry>,
}

/// The frame of the positions.
#[derive(Clone, Debug, Serialize)]
pub struct FrameInfo {
    pub name: &'static str,
    pub origin: &'static str,
    pub axes: &'static str,
    pub epoch: &'static str,
}

/// The frame every manifest names.
pub const FRAME: FrameInfo = FrameInfo {
    name: "ICRS equatorial Cartesian",
    origin: "Solar System barycentre",
    axes: "x toward (RA, Dec) = (0 deg, 0 deg), y toward (90 deg, 0 deg), z toward Dec = +90 deg",
    epoch: "each object's position at the epoch of its preferred sky position, not propagated",
};

/// The rotations from the manifest's frame, each applied as `matrix · [x, y, z]`.
#[derive(Clone, Debug, Serialize)]
pub struct Rotations {
    pub applied_as: &'static str,
    pub to_galactic: Rotation,
    pub galactic_basis: &'static str,
    pub to_supergalactic: Rotation,
    pub supergalactic_basis: &'static str,
}

impl Rotations {
    /// The rotations univ-model derives every galactic and supergalactic position with.
    ///
    /// # Errors
    ///
    /// Fails only if univ-model has no rotation between the frames, which it always has.
    pub fn from_model() -> Result<Self, TilesError> {
        let rotation = |frame| {
            from_icrs(frame)
                .rotation()
                .ok_or(TilesError::NoRotation(frame))
        };
        Ok(Self {
            applied_as: "row-major matrix times the ICRS column vector [x, y, z]",
            to_galactic: rotation(Frame::Galactic)?,
            galactic_basis: "Hipparcos catalogue (ESA 1997, vol. 1, sec. 1.5.3): the transpose of A_G",
            to_supergalactic: rotation(Frame::Supergalactic)?,
            supergalactic_basis: "Lahav et al. 2000 (MNRAS 312, 166): pole at galactic (47.37, \
                                  6.32) deg, origin at (137.37, 0) deg",
        })
    }
}

/// The tile encoding the manifest's tiles use.
#[derive(Clone, Debug, Serialize)]
pub struct Encoding {
    pub version: u16,
    pub header_bytes: usize,
    pub point_bytes: usize,
    /// The quantization steps along a node's edge: a step is the edge over this.
    pub steps_per_edge: u32,
    pub max_tile_bytes: usize,
    /// The kind-of-value codes, in order from 0.
    pub kinds: [&'static str; 4],
}

/// The encoding of this version.
pub const ENCODING: Encoding = Encoding {
    version: VERSION,
    header_bytes: HEADER_BYTES,
    point_bytes: POINT_BYTES,
    steps_per_edge: STEPS,
    max_tile_bytes: MAX_TILE_BYTES,
    kinds: ["measured", "estimated", "inferred", "reconstructed"],
};

/// A level of the octree: its nodes' edge and its quantization step, in the unit.
#[derive(Clone, Debug, Serialize)]
pub struct Level {
    pub level: u8,
    pub edge: f64,
    pub step: f64,
}

/// How the subsample was drawn, and the rules the octree was split by.
#[derive(Clone, Debug, Serialize)]
pub struct Sampling {
    pub seed: u64,
    pub generator: &'static str,
    /// The `ChaCha20` stream of this domain.
    pub stream: u64,
    pub split_threshold: usize,
    pub level_points: usize,
    pub max_depth: u8,
    pub statement: String,
}

impl Sampling {
    #[must_use]
    pub fn new(rules: &TileRules, stream: u64) -> Self {
        Self {
            seed: rules.seed,
            generator: "ChaCha20, key the seed's 8 little-endian bytes then 24 zero bytes, \
                        stream the domain's number; an inner node's points shuffled by a partial \
                        Fisher-Yates over its points in increasing id, nodes in depth-first order \
                        of address",
            stream,
            split_threshold: rules.split_threshold,
            level_points: rules.level_points,
            max_depth: rules.max_depth,
            statement: rules.statement.clone(),
        }
    }
}

/// A source whose points, or whose sky positions, the tiles hold, at its index in the table.
#[derive(Clone, Debug, Serialize)]
pub struct SourceEntry {
    pub index: u16,
    pub id: String,
    pub title: String,
    pub citation: String,
    /// The credit the source requires, quoted from its terms.
    pub credit: String,
}

/// A node's cube, in the unit.
#[derive(Clone, Debug, Serialize)]
pub struct Bounds {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

/// One octree node and its tile.
#[derive(Clone, Debug, Serialize)]
pub struct NodeEntry {
    pub address: String,
    pub level: u8,
    pub bounds: Bounds,
    /// The points its tile holds.
    pub points: usize,
    /// The points that reached it, those of its cube that no node above holds: its tile holds
    /// an even random subsample of them, or all of them for a leaf.
    pub sampled_from: usize,
    pub bytes: usize,
    pub sha256: String,
}

impl Manifest {
    /// The manifest as JSON, two-space indented, ending with a newline.
    ///
    /// # Errors
    ///
    /// Fails only if `serde_json` cannot write it.
    pub fn to_json(&self) -> Result<Vec<u8>, TilesError> {
        let mut json =
            serde_json::to_vec_pretty(self).map_err(|e| TilesError::Manifest(e.to_string()))?;
        json.push(b'\n');
        Ok(json)
    }
}
