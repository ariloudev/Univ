//! A scale domain's tile manifest, the JSON a client reads before any tile (fetch-a-tile-manifest):
//! its frame, unit and cosmology, the rotations to galactic and supergalactic axes, the tile
//! encoding, the quantization step and sampling fraction of each level, the seed and rules of
//! the subsample, the sources its points come from with their role and credit, and each node's
//! address, bounds, level, point count, byte size and SHA-256.
//!
//! Univ Ingest writes it as `releases/{n}/tiles/{domain}/manifest.json` with
//! [`TileManifest::to_json`]; Univ Server reads it back with this type.
//!
//! - **Levels.** Every inner node of a level keeps the level's `fraction` of the points
//!   reaching it (`sampled_from`), rounded down, so the points a client shows down to any level
//!   keep the densities of the domain in proportion. A leaf keeps every point reaching it: it is
//!   the finest level of its region, and nothing lies below it.
//! - **Sources.** A point's source index names the source of its preferred distance, which
//!   gives its kind of value too. The table also credits the sources of the points' sky
//!   positions, and each entry's `role` says which: `distance`, `position`, or
//!   `distance_and_position`.
//!
//! Its numbers are written by `serde_json` as the shortest decimal that reads back to the same
//! `f64`, and every list is in a fixed order, so the same build gives the same bytes.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use super::{
    COLOUR_BANDS, ColourBands, HEADER_BYTES, KINDS, MAX_TILE_BYTES, POINT_BYTES, STEPS, VERSION,
};
use crate::cosmology::Cosmology;
use crate::position::{BARYCENTRE, Frame, ICRS_CARTESIAN_NAME, Rotation, from_icrs};
use crate::value::KindOfValue;

/// The name a manifest gives its format.
pub const FORMAT: &str = "univ-tile-manifest";
/// The version of the manifest's own layout.
pub const MANIFEST_VERSION: u32 = 1;

/// A tile manifest, in the order its JSON gives its fields.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TileManifest {
    /// Always `univ-tile-manifest`.
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub format: Cow<'static, str>,
    pub manifest_version: u32,
    pub release: String,
    /// `stars`, `nearby_galaxies` or `cosmic_web`.
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub scale_domain: Cow<'static, str>,
    pub frame: FrameInfo,
    /// `pc` or `Mpc`.
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub unit: Cow<'static, str>,
    /// The release cosmology the positions were derived under.
    pub cosmology: Cosmology,
    pub rotations: Rotations,
    pub encoding: Encoding,
    /// Each level present, root first, with its node edge, quantization step and sampling
    /// fraction.
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct FrameInfo {
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub name: Cow<'static, str>,
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub origin: Cow<'static, str>,
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub axes: Cow<'static, str>,
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub epoch: Cow<'static, str>,
}

/// The frame every manifest names.
pub const FRAME: FrameInfo = FrameInfo {
    name: Cow::Borrowed(ICRS_CARTESIAN_NAME),
    origin: Cow::Borrowed(BARYCENTRE),
    axes: Cow::Borrowed(
        "x toward (RA, Dec) = (0 deg, 0 deg), y toward (90 deg, 0 deg), z toward Dec = +90 deg",
    ),
    epoch: Cow::Borrowed(
        "each object's position at the epoch of its preferred sky position, not propagated",
    ),
};

/// The rotations from the manifest's frame, each applied as `matrix · [x, y, z]`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Rotations {
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub applied_as: Cow<'static, str>,
    pub to_galactic: Rotation,
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub galactic_basis: Cow<'static, str>,
    pub to_supergalactic: Rotation,
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub supergalactic_basis: Cow<'static, str>,
}

impl Rotations {
    /// The rotations univ-model derives every galactic and supergalactic position with.
    ///
    /// # Errors
    ///
    /// Fails, naming the frame, only if univ-model has no rotation from the ICRS to it, which
    /// it always has.
    pub fn from_model() -> Result<Self, Frame> {
        let rotation = |frame| from_icrs(frame).rotation().ok_or(frame);
        Ok(Self {
            applied_as: Cow::Borrowed("row-major matrix times the ICRS column vector [x, y, z]"),
            to_galactic: rotation(Frame::Galactic)?,
            galactic_basis: Cow::Borrowed(
                "Hipparcos catalogue (ESA 1997, vol. 1, sec. 1.5.3): the transpose of A_G",
            ),
            to_supergalactic: rotation(Frame::Supergalactic)?,
            supergalactic_basis: Cow::Borrowed(
                "Lahav et al. 2000 (MNRAS 312, 166): pole at galactic (47.37, 6.32) deg, origin \
                 at (137.37, 0) deg",
            ),
        })
    }
}

/// The tile encoding the manifest's tiles use.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Encoding {
    pub version: u16,
    pub header_bytes: usize,
    pub point_bytes: usize,
    /// The quantization steps along a node's edge: a step is the edge over this.
    pub steps_per_edge: u32,
    pub max_tile_bytes: usize,
    /// What a point's source index names.
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub point_source: Cow<'static, str>,
    /// The kind-of-value codes, in order from 0.
    #[cfg_attr(
        feature = "openapi",
        schema(value_type = Vec<KindOfValue>, min_items = 4, max_items = 4)
    )]
    pub kinds: [KindOfValue; 4],
    /// The colour bands, coded by their index plus one, 0 coding none; absent before version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colour_bands: Option<[ColourBands; 2]>,
}

/// The encoding of this version.
pub const ENCODING: Encoding = Encoding {
    version: VERSION,
    header_bytes: HEADER_BYTES,
    point_bytes: POINT_BYTES,
    steps_per_edge: STEPS,
    max_tile_bytes: MAX_TILE_BYTES,
    point_source: Cow::Borrowed(
        "the index in sources of the source of the point's preferred distance, which gives its \
         kind of value too",
    ),
    kinds: KINDS,
    colour_bands: Some(COLOUR_BANDS),
};

/// A level of the octree: its nodes' edge and its quantization step, in the unit, and the
/// fraction of the points reaching it that each of its inner nodes keeps.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Level {
    pub level: u8,
    pub edge: f64,
    pub step: f64,
    /// `level_points / m`, `m` the most points reaching any inner node of the level: an inner
    /// node of `n` points keeps `floor(level_points × n / m)`. 1 for a level of leaves alone;
    /// a leaf keeps every point reaching it.
    pub fraction: f64,
}

/// How the subsample was drawn, and the rules the octree was split by.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Sampling {
    pub seed: u64,
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub generator: Cow<'static, str>,
    /// The `ChaCha20` stream of this domain.
    pub stream: u64,
    pub split_threshold: usize,
    pub level_points: usize,
    pub max_depth: u8,
    pub statement: String,
}

/// What a source gives the points of a domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum SourceRole {
    /// The preferred distance of some points, so their source index and kind of value.
    Distance,
    /// The sky position of some points, whose distance comes from another source.
    Position,
    /// Both, for some points or for different ones.
    DistanceAndPosition,
}

/// A source whose points, or whose sky positions, the tiles hold, at its index in the table.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SourceEntry {
    pub index: u16,
    pub id: String,
    pub role: SourceRole,
    pub title: String,
    pub citation: String,
    /// The credit the source requires, quoted from its terms.
    pub credit: String,
}

/// A node's cube, in the unit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Bounds {
    #[cfg_attr(feature = "openapi", schema(value_type = Vec<f64>, min_items = 3, max_items = 3))]
    pub min: [f64; 3],
    #[cfg_attr(feature = "openapi", schema(value_type = Vec<f64>, min_items = 3, max_items = 3))]
    pub max: [f64; 3],
}

/// One octree node and its tile.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct NodeEntry {
    /// `r`, then one octant digit, 0 to 7, per level below the root.
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

impl TileManifest {
    /// The manifest as written: JSON, two-space indented, ending with a newline.
    ///
    /// # Errors
    ///
    /// Fails only if `serde_json` cannot write it.
    pub fn to_json(&self) -> Result<Vec<u8>, serde_json::Error> {
        let mut json = serde_json::to_vec_pretty(self)?;
        json.push(b'\n');
        Ok(json)
    }
}

/// What a server reads of a tile manifest to serve its tiles and the nodes that cover a
/// volume: the encoding version and each node's address, level, cube and SHA-256, the rest of
/// the manifest left unread.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct TileManifestIndex {
    pub encoding: EncodingIndex,
    pub nodes: Vec<NodeIndex>,
}

/// The encoding version of a [`TileManifestIndex`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub struct EncodingIndex {
    pub version: u16,
}

/// A node of a [`TileManifestIndex`].
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct NodeIndex {
    pub address: String,
    pub level: u8,
    pub bounds: Bounds,
    pub sha256: String,
}

impl NodeIndex {
    /// The address of the node above this one: its own without its last octant digit; none
    /// for the root, `r`.
    #[must_use]
    pub fn parent(&self) -> Option<&str> {
        (self.address.len() > 1).then(|| &self.address[..self.address.len() - 1])
    }
}
