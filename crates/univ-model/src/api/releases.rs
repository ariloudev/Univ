//! The listing of releases (list-and-pin-releases).

use serde::{Deserialize, Serialize};

use crate::cosmology::Cosmology;

/// Every release ever published, newest first, and the one that `latest` points to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ReleaseList {
    /// The number of the release that reads naming no release answer from.
    pub latest: u32,
    /// Every published release, pruned ones included, newest first.
    pub releases: Vec<ReleaseSummary>,
}

/// One published release.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ReleaseSummary {
    /// Its number, from 1, never reused.
    pub number: u32,
    /// When `latest` moved to it.
    pub published_at: String,
    /// When it was pruned, if it was: its data is gone, and reads pinned to it are refused.
    pub pruned_at: Option<String>,
    /// The cosmology every derived value of the release was computed under.
    pub cosmology: Cosmology,
    /// The ids of the sources it was built from.
    pub sources: Vec<String>,
    /// How much it holds.
    pub counts: ReleaseCounts,
}

/// How many of each thing a release holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ReleaseCounts {
    pub objects: u64,
    pub structures: u64,
    pub grids: u64,
    pub tiles: u64,
}
