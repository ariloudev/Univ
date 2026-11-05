//! The release manifest: what a release is, what it was built under, and every file it holds
//! with its SHA-256. Univ Ingest writes it as YAML at [`super::keys::MANIFEST`]; publishing
//! checks each file against it before uploading.

use serde::{Deserialize, Serialize};

use crate::api::sources::PermissionRequest;

use super::fingerprint::ColumnTotal;
use crate::cosmology::Cosmology;

/// The name a release manifest gives its format.
pub const FORMAT: &str = "univ-release-manifest";
/// The version of the manifest's own layout.
pub const MANIFEST_VERSION: u32 = 1;
/// The most D1 storage one release's tables may take (CON-3: 10 GB, decimal gigabytes).
pub const D1_CAP_BYTES: u64 = 10_000_000_000;
/// The most D1 storage a build lets one release take: under half the cap, so release n + 1 is
/// published beside release n, and checked with it against the cap, before n is pruned.
pub const RELEASE_BUDGET_BYTES: u64 = 4_500_000_000;

/// A release manifest.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseManifest {
    pub format: String,
    pub manifest_version: u32,
    pub release: u32,
    /// RFC 3339, UTC.
    pub built_at: String,
    pub cosmology: Cosmology,
    /// What a reader of the release's distances needs to know of its cosmology, with its
    /// citations: such as the Hubble constant the distance ladder's distances are on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cosmology_note: Option<String>,
    /// The line-of-sight peculiar velocity in every redshift distance's error, km/s.
    pub peculiar_velocity_km_s: f64,
    pub versions: Versions,
    /// The rules files the release was built under, recorded verbatim in the release.
    pub rules: Vec<RecordedFile>,
    pub cell_index: CellIndex,
    pub sources: Vec<ManifestSource>,
    pub counts: Counts,
    pub ids: Ids,
    pub d1: D1Estimate,
    /// Every file of the release but this manifest, sorted by path.
    pub files: Vec<ManifestFile>,
}

/// The versions the release was built with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Versions {
    pub univ_model: String,
    pub univ_ingest: String,
    pub d1_schema: u32,
    pub tile_encoding: u16,
    pub tile_manifest: u32,
}

/// A rules file recorded in the release.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedFile {
    /// `matching`, `ranking`, `tiles` or `release`.
    pub name: String,
    /// The rule's own name, where the file gives one, such as the ranking rule's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
    /// Its path in the release.
    pub path: String,
    pub sha256: String,
}

/// The cell index's scheme, as [`super::cells`] lays it out.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CellIndex {
    pub scheme: String,
    pub frame: String,
    pub spaces: Vec<CellSpaceInfo>,
    pub structure_span: i64,
    pub max_structure_level: u8,
    /// Each structure the index does not hold, with the reason.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub not_indexed: Vec<NotIndexed>,
}

/// A structure the cell index does not hold, and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NotIndexed {
    pub structure: String,
    pub reason: String,
}

/// A cell space: its name, unit and level-0 cell edge.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CellSpaceInfo {
    pub name: String,
    pub unit: String,
    pub edge: f64,
}

/// A source of the release.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestSource {
    pub id: String,
    /// Its redistribution status: `permitted`, `permitted_with_credit` or
    /// `permission_pending`, never `forbidden`.
    pub redistribution: String,
    /// Whether that judgement is `provisional` or `settled`.
    pub review: String,
    /// The clause of its terms that allows only non-commercial use, quoted; none when its terms
    /// allow commercial use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub non_commercial: Option<String>,
    /// Whom permission was asked of and when (YYYY-MM-DD), while its status is
    /// `permission_pending`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission: Option<PermissionRequest>,
    /// Catalogue entries the release holds from it.
    pub entries: u64,
}

/// What the release holds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Counts {
    pub objects: u64,
    pub placed: u64,
    pub entries: u64,
    pub structures: u64,
    /// Structures placed with a bounding box, which the cell index holds.
    pub structures_indexed: u64,
    pub grids: u64,
    pub tiles: u64,
    pub object_cells: u64,
    pub structure_cells: u64,
}

/// The ids the release issued and replaced.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ids {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issued_first: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issued_last: Option<u64>,
    pub replaced: u64,
}

/// The estimated D1 storage of the release's rows (see [`super::sql`]) against the cap.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct D1Estimate {
    pub cap_bytes: u64,
    pub estimated_bytes: u64,
    pub tables: Vec<TableEstimate>,
}

/// One table's rows of the release, their estimated bytes, and their fingerprint, which
/// publishing compares with what D1 holds once they are loaded (see [`super::fingerprint`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableEstimate {
    pub table: String,
    pub rows: u64,
    pub estimated_bytes: u64,
    pub fingerprint: Vec<ColumnTotal>,
    /// For a table checked by its content, the SHA-256, in hex, of its rows in the order of the
    /// primary key, each in [`super::fingerprint::canonical_row`]'s form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_sha256: Option<String>,
}

/// Where a file of the release goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    /// Uploaded to R2 at its key.
    R2,
    /// Executed against D1, in path order.
    D1,
}

/// A file of the release.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestFile {
    /// Its path in the release directory.
    pub path: String,
    pub target: Target,
    /// Its R2 key, for a file uploaded to R2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub bytes: u64,
    pub sha256: String,
}
