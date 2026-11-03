//! A release's sources and their provenance (read-a-source).

use serde::{Deserialize, Serialize};

/// Every source of a release.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SourceList {
    /// The release the listing was answered from.
    pub release: u32,
    /// Its sources, in the release's order.
    pub sources: Vec<SourceSummary>,
}

/// One source as a listing gives it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SourceSummary {
    /// Its id, such as `gaia-dr3-bright`.
    pub id: String,
    pub title: String,
    /// The scale domain its points are tiled in: `stars`, `nearby_galaxies` or `cosmic_web`.
    pub scale_domain: String,
    /// How many of its catalogue entries the release holds.
    pub entries: u64,
    /// Its citation, as its source definition gives it.
    pub citation: String,
    /// The credit its terms of use require, as its source definition gives it.
    pub credit: String,
}
