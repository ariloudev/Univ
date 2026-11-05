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

/// One source in full: where its data came from and on what terms.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SourceDetail {
    /// The release it was read from.
    pub release: u32,
    pub id: String,
    pub title: String,
    /// The service it was fetched from, such as `vizier`.
    pub provider: String,
    pub scale_domain: String,
    /// How many of its catalogue entries the release holds.
    pub entries: u64,
    /// Its citation, as its source definition gives it.
    pub citation: String,
    /// Its primary reference; absent from a release built before references were recorded.
    pub reference: Option<Reference>,
    /// Its terms of use and the credit they require, verbatim.
    pub terms: Terms,
    /// The query the fetch ran: the access method, table, constraints and slices, verbatim.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub query: serde_json::Value,
    /// The cuts the fetch made and why, as the source definition states them.
    pub cuts: Vec<String>,
    /// Where on the sky and how far its entries reach; absent from a release built before
    /// coverage was recorded.
    pub coverage: Option<Coverage>,
}

/// A source's primary reference: its ADS bibcode or DOI, or why it has no single one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Reference {
    pub bibcode: Option<String>,
    pub doi: Option<String>,
    /// Why there is no single primary reference, as for a compilation cited paper by paper.
    pub note: Option<String>,
}

/// A source's terms of use, as its definition quotes them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Terms {
    pub url: String,
    /// The terms, quoted verbatim.
    pub quote: String,
    /// The credit the terms require.
    pub credit: String,
    /// Whether the terms allow the source to be published, and on what condition:
    /// `permitted`, `permitted_with_credit` or `permission_pending`.
    pub redistribution: String,
    /// How that reading of the terms was reviewed.
    pub review: String,
    /// The clause of the terms that allows only non-commercial use, quoted; absent when the
    /// terms allow commercial use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub non_commercial: Option<String>,
    /// Whom permission to redistribute the source was asked of and when, while its
    /// redistribution is `permission_pending`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission: Option<PermissionRequest>,
}

/// A request for permission to redistribute a source, awaiting its answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PermissionRequest {
    /// Whom it was asked of.
    pub asked_of: String,
    /// The date it was asked, YYYY-MM-DD.
    pub asked_on: String,
}

/// Where a source's catalogue entries lie.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Coverage {
    /// The sky its entries lie on; absent when none has a sky position.
    pub sky: Option<SkyFootprint>,
    /// The range of each radial measure its entries hold: redshift, distance or parallax.
    pub radial: Vec<RadialRange>,
}

/// A sky footprint as nested `HEALPix` cells of one order, in ICRS (Górski et al. 2005).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SkyFootprint {
    pub order: u8,
    /// The cells holding at least one entry, ascending.
    pub cells: Vec<u64>,
}

/// The span of one radial measure over a source's entries.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RadialRange {
    /// `redshift`, `distance` or `parallax`.
    pub measure: String,
    pub min: f64,
    pub max: f64,
    pub unit: String,
    /// What the values are, such as "heliocentric redshift as published".
    pub basis: String,
    /// How many entries hold the measure.
    pub entries: u64,
}

/// The record of a source's acquisition and normalization.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SourceProvenance {
    pub release: u32,
    pub id: String,
    /// Each payload fetched, with its SHA-256 and when it was retrieved.
    pub payloads: Vec<Payload>,
    /// How many catalogue entries were fetched, published and left out; absent for a source
    /// that was not normalized from fetched rows, such as the researched structures.
    pub counts: Option<EntryCounts>,
    /// How many entries were left out, by the reason each was left out.
    pub left_out: std::collections::BTreeMap<String, u64>,
    /// Each fix of a known defect of the source, with its reason and how many entries it
    /// changed in this release.
    pub fixes: Vec<Fix>,
}

/// One payload fetched.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Payload {
    pub item: String,
    pub url: String,
    pub sha256: String,
    pub bytes: u64,
    /// When it was retrieved, RFC 3339 UTC.
    pub retrieved: String,
    /// The rows it held, where the format counts rows.
    pub rows: Option<u64>,
}

/// The catalogue entries of a source: fetched is published and left out together.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct EntryCounts {
    pub fetched: u64,
    pub published: u64,
    pub left_out: u64,
}

/// A fix of a known defect of a source.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Fix {
    /// The defect: what is wrong in the source as published.
    pub reason: String,
    /// What normalization does about it.
    pub change: String,
    /// How many entries it changed in this release.
    pub entries_changed: u64,
}
