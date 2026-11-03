//! API keys as the owner issues, lists and adjusts them (issue-and-revoke-keys,
//! enforce-quotas-and-rate-limits).

use serde::{Deserialize, Serialize};

/// A key's rate-limit tier: how many requests it may make in 60 seconds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum RateLimitTier {
    /// 300 requests in 60 seconds.
    Basic,
    /// 1,200 requests in 60 seconds; a key issued without a tier gets this one.
    #[default]
    Standard,
    /// 6,000 requests in 60 seconds.
    Extended,
}

impl RateLimitTier {
    /// The requests a key of this tier may make in [`RATE_WINDOW_SECONDS`].
    #[must_use]
    pub fn requests(self) -> u32 {
        match self {
            Self::Basic => 300,
            Self::Standard => 1_200,
            Self::Extended => 6_000,
        }
    }
}

/// The window every rate limit counts requests over.
pub const RATE_WINDOW_SECONDS: u32 = 60;

/// The requests an anonymous client address may make in [`RATE_WINDOW_SECONDS`].
pub const ANONYMOUS_REQUESTS: u32 = 300;

/// The largest quota a key can be given: 2^53 − 1, the largest integer every JSON client and
/// D1 carry exactly.
pub const MAX_QUOTA: u64 = (1 << 53) - 1;

/// A key's quotas.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Quotas {
    pub layers_per_key: u64,
    pub items_per_layer: u64,
    pub bytes_per_item_version: u64,
}

impl Default for Quotas {
    /// The default quotas: 100 layers per key, 100,000 items per layer and 64 KiB per item
    /// version.
    fn default() -> Self {
        Self {
            layers_per_key: 100,
            items_per_layer: 100_000,
            bytes_per_item_version: 64 * 1_024,
        }
    }
}

/// Quotas to set, each kept as it is where absent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct QuotaChange {
    pub layers_per_key: Option<u64>,
    pub items_per_layer: Option<u64>,
    pub bytes_per_item_version: Option<u64>,
}

/// What the owner asks to issue.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct IssueKey {
    /// A name for the key, for the owner.
    pub label: String,
    /// The quotas; the default quotas where absent.
    #[serde(default)]
    pub quotas: QuotaChange,
    /// The tier; the standard tier where absent.
    #[serde(default)]
    pub tier: RateLimitTier,
}

/// What the owner asks to change of a key.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct KeyChange {
    pub label: Option<String>,
    #[serde(default)]
    pub quotas: QuotaChange,
    pub tier: Option<RateLimitTier>,
}

/// A key as the owner sees it, without its secret.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct KeyRecord {
    pub id: String,
    pub label: String,
    /// RFC 3339 UTC.
    pub issued_at: String,
    /// RFC 3339 UTC; absent while the key stands.
    pub revoked_at: Option<String>,
    pub quotas: Quotas,
    pub tier: RateLimitTier,
}

/// A key just issued: its record and its secret, which no later response gives.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct IssuedKey {
    #[serde(flatten)]
    pub key: KeyRecord,
    /// The secret to send as `Authorization: Bearer <secret>`: 256 random bits, hex encoded
    /// after the `univ_` prefix. Only its SHA-256 is stored.
    pub secret: String,
}

/// Every key the owner has issued.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct KeyList {
    pub keys: Vec<KeyRecord>,
}
