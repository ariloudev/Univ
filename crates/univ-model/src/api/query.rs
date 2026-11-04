//! A bounded volume query and its answer (query-a-bounded-volume, filter-by-time).

use serde::{Deserialize, Serialize};

use crate::position::Vec3;
use crate::time::{At, TimeKind, Timescale};
use crate::volume::Volume;

/// The most results one page holds.
pub const RESULTS_PER_PAGE: usize = 1_000;
/// The most results a query returns over all its pages.
pub const MAX_RESULTS: usize = 10_000;
/// The most tile nodes given for one scale domain.
pub const MAX_TILE_NODES: usize = 4_096;

/// A query of the objects, structures and layer items in a volume. Its later pages send the
/// same query with the cursor the page before gave.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct VolumeQuery {
    pub volume: Volume,
    #[serde(default)]
    pub filters: Filters,
    /// Layer items whose time records of a kind meet an interval.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<TimeFilter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tiles: Option<TileRequest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// What narrows the results. Each filter given keeps only the results that carry one of the
/// values it names, so a result that carries none, such as a layer item under `kinds`, is left
/// out.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Filters {
    /// Kinds of object (terms such as `galaxy`) and of structure (such as `void`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kinds: Option<Vec<String>>,
    /// Sources of the release, by id: objects with an entry from one of them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sources: Option<Vec<String>>,
    /// Scale domains, by name: their objects and structures.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale_domains: Option<Vec<String>>,
    /// Layers, by id: their items.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layers: Option<Vec<String>>,
}

/// Layer items with a time record of `kind` whose uncertainty interval meets `from` to `to`,
/// compared on TT.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TimeFilter {
    pub kind: TimeKind,
    pub from: TimeBound,
    pub to: TimeBound,
}

/// A bound of an interval of time, in the form of a time record's time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TimeBound {
    pub timescale: Timescale,
    #[serde(flatten)]
    pub at: At,
}

/// The tile nodes wanted with the first page.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TileRequest {
    /// The deepest octree level to give; the deepest a manifest holds when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_level: Option<u8>,
}

/// A page of a volume query's results.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct VolumePage {
    pub release: u32,
    /// Objects by id, then structures by id, then layer items by layer and item.
    pub results: Vec<QueryResult>,
    /// The results given so far, this page's included.
    pub served: usize,
    /// The cursor of the next page, while results remain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
    /// Whether more than [`MAX_RESULTS`] results matched and only the first were given.
    pub truncated: bool,
    /// For each scale domain the filters leave, the tile nodes that cover the volume; on the
    /// first page alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tiles: Option<Vec<TileCovering>>,
}

/// One result of a volume query.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum QueryResult {
    Object(ObjectHit),
    Structure(StructureHit),
    Item(ItemHit),
}

/// An object whose derived position lies in the volume.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ObjectHit {
    pub id: u64,
    pub kind: String,
    pub scale_domain: String,
    pub position: HitPosition,
}

/// A derived position: ICRS Cartesian about the barycentre, in its scale domain's unit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct HitPosition {
    /// Always `icrs`.
    pub frame: String,
    /// `parsec` or `megaparsec`.
    pub unit: String,
    #[cfg_attr(feature = "openapi", schema(value_type = Vec<f64>, min_items = 3, max_items = 3))]
    pub at: Vec3,
}

/// A structure that meets the volume, and what of it was tested.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct StructureHit {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub scale_domain: String,
    pub matched_by: MatchedBy,
}

/// What of a structure a volume was tested against, the first the release holds of: its placed
/// shape, its placed member list, its bounding box, its centre.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum MatchedBy {
    Shape,
    Members,
    /// Approximate: the release holds no placed shape for the structure.
    BoundingBox,
    Centre,
}

/// A layer item whose current shape meets the volume.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ItemHit {
    pub layer: String,
    pub item: String,
    pub version: u64,
    pub title: String,
}

/// The tile nodes of a scale domain that cover the volume, at the level chosen for it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TileCovering {
    pub scale_domain: String,
    /// The deepest level, no deeper than asked, at which at most [`MAX_TILE_NODES`] nodes
    /// meet the volume.
    pub level: u8,
    /// Node addresses, each fetched as `/v1/scale-domains/{scale_domain}/tiles/{address}`.
    pub nodes: Vec<String>,
}
