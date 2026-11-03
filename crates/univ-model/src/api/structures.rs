//! A release's structures (read-a-structure): voids, walls, filaments, superclusters, groups,
//! clusters and basins of attraction, each number cited.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A page of a release's structures, by id.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct StructurePage {
    /// The release the page was read from; every page of one listing reads the same release.
    pub release: u32,
    /// The structures of this page, ordered by id.
    pub structures: Vec<StructureSummary>,
    /// The cursor of the next page, while structures remain.
    pub next: Option<String>,
}

/// A structure as a listing gives it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct StructureSummary {
    pub id: String,
    pub name: String,
    /// Its kind, such as `void`, `wall` or `supercluster`.
    pub kind: String,
    pub scale_domain: String,
    /// The centre of its geometry; absent when it cannot be placed.
    pub centre: Option<Centre>,
}

/// A point placed by the release: ICRS equatorial Cartesian about the Solar System barycentre,
/// in its scale domain's unit, under the release cosmology.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Centre {
    /// "ICRS equatorial Cartesian", as the tile manifests name it.
    pub frame: String,
    /// "Solar System barycentre".
    pub origin: String,
    pub unit: String,
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// One structure in full.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct StructureDetail {
    pub release: u32,
    pub id: String,
    pub name: String,
    pub kind: String,
    /// `observed`, `reconstructed` or `statistical_claim`: how the structure is known.
    pub status: String,
    pub scale_domain: String,
    /// The structure as its researched file records it, verbatim: each published number with
    /// its kind of value, its citation (paper and table, figure or sentence), and the geometry
    /// in the frames the papers used.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub record: Value,
    pub centre: Option<Centre>,
    /// The placed geometry's axis-aligned bounds, in the centre's frame and unit.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub bounds: Option<Value>,
    /// The placed geometry, one of univ-model's geometry kinds, in ICRS.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub shape: Option<Value>,
    /// How the placed numbers were derived from the published ones.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub derivation: Option<Value>,
    /// Its members placed whole: objects of the release, or tracers with their own positions
    /// and kinds.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub members: Option<Value>,
    /// For a structure outlined from a volume grid: the grid, the threshold, the test and its
    /// verdict.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub outline: Option<Value>,
    /// Each size published under another cosmology, as published and rescaled to the release
    /// cosmology.
    #[cfg_attr(feature = "openapi", schema(value_type = Vec<Object>))]
    pub rescaled: Vec<Value>,
}
