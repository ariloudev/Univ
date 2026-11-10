//! A release's bulk data as listed (fetch-a-tile-manifest, fetch-a-volume-grid): its scale
//! domains, whose tiles and manifests are served from R2 as published, and its volume grids.

use serde::{Deserialize, Serialize};

use crate::tile::format::EncodingLayout;
use crate::units::Unit;

/// The scale domains of a release.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ScaleDomainList {
    pub release: u32,
    pub domains: Vec<ScaleDomainSummary>,
}

/// One scale domain: a set of points tiled into one octree.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ScaleDomainSummary {
    /// `stars`, `galaxy_tracers`, `nearby_galaxies` or `cosmic_web`.
    pub name: String,
    /// The unit of its positions and of its radius.
    pub unit: Unit,
    /// The radius it extends to from the Solar System barycentre, in its unit.
    pub radius: f64,
    /// The points its tiles hold.
    pub points: u64,
    /// The nodes of its octree, one tile each.
    pub nodes: u64,
    /// The path of its tile manifest in the same release.
    pub manifest: String,
}

/// The volume grids of a release.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GridList {
    pub release: u32,
    pub grids: Vec<GridSummary>,
}

/// One volume grid as listed; its header gives the rest.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GridSummary {
    pub id: String,
    /// The source it was published by, by id.
    pub source: String,
    /// The unit of its axes, such as `megaparsec_over_h`.
    pub unit: Unit,
    /// Its number of cells along each axis, in the order of its axes.
    pub shape: Vec<u64>,
    /// The quantities it samples, in the order of their samples files.
    pub fields: Vec<GridField>,
    /// The path of its header in the same release.
    pub header: String,
}

/// One quantity a grid samples.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GridField {
    /// Its number among the grid's fields, from 0: the path of its samples ends in it.
    pub index: u32,
    /// The quantity, such as `density` or `basin_label`.
    pub field: String,
    /// Its kind of value, `reconstructed` for a reconstruction.
    pub kind: String,
    pub unit: Unit,
    /// The path of its samples in the same release.
    pub samples: String,
    /// The path of the samples of its uncertainty, where it was published with one.
    pub uncertainty: Option<String>,
}

/// How to decode a release's tiles (decode-the-tile-format): the byte layout of each tile
/// encoding its scale domains use, and the schema of a tile manifest.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TileFormat {
    pub release: u32,
    /// The byte layout of each encoding version the release's tiles use, by version.
    pub encodings: Vec<EncodingLayout>,
    /// The JSON Schema of a tile manifest, as the OpenAPI document gives it.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub manifest_schema: serde_json::Value,
}
