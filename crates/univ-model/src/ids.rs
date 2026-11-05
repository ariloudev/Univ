//! Object ids, source ids and designations.

use std::fmt;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The largest integer a JavaScript number holds exactly, 2^53 − 1: ids reach TypeScript clients.
const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;

/// An object's id in Univ releases: a positive integer that, once given, is never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(try_from = "u64", into = "u64")]
pub struct ObjectId(u64);

/// A source's id: a slug of lowercase letters, digits and hyphens, starting with a letter, such
/// as `gaia-dr3` or `cosmicflows-4`. Its text is shared by every copy: each of millions of
/// designations names its source.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SourceId(Arc<str>);

/// A source's own name for an entry: the source and its identifier, such as Gaia DR3 and a
/// `source_id`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(try_from = "RawDesignation")]
pub struct Designation {
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub source: SourceId,
    // Shared by every copy: an entry's designation is copied into its object's preferred
    // measurements and the id maps, for each of tens of millions of entries. A plain comment, as
    // a doc comment would enter the API's OpenAPI document.
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub identifier: Arc<str>,
}

/// A designation as read, before its checks.
#[derive(Deserialize)]
struct RawDesignation {
    source: SourceId,
    identifier: String,
}

impl TryFrom<RawDesignation> for Designation {
    type Error = IdError;
    fn try_from(raw: RawDesignation) -> Result<Self, IdError> {
        Self::new(raw.source, raw.identifier)
    }
}

/// An id that cannot stand.
#[derive(Debug, Error, PartialEq)]
pub enum IdError {
    #[error("object id {0} lies outside 1 to 2^53 - 1")]
    ObjectId(u64),
    #[error(
        "source id {0:?} is not lowercase letters, digits and single hyphens starting with a letter"
    )]
    SourceId(String),
    #[error("a designation from {0} has an empty identifier")]
    EmptyIdentifier(SourceId),
}

impl ObjectId {
    /// # Errors
    ///
    /// Fails on zero or a value beyond 2^53 − 1.
    pub fn new(value: u64) -> Result<Self, IdError> {
        if (1..=MAX_SAFE_INTEGER).contains(&value) {
            Ok(Self(value))
        } else {
            Err(IdError::ObjectId(value))
        }
    }

    #[must_use]
    pub fn get(self) -> u64 {
        self.0
    }
}

impl TryFrom<u64> for ObjectId {
    type Error = IdError;
    fn try_from(value: u64) -> Result<Self, IdError> {
        Self::new(value)
    }
}

impl From<ObjectId> for u64 {
    fn from(id: ObjectId) -> Self {
        id.0
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Whether `text` is a slug: lowercase letters, digits and single hyphens, starting with a
/// letter and not ending with a hyphen, such as `tully-2019`.
#[must_use]
pub fn is_slug(text: &str) -> bool {
    text.starts_with(|c: char| c.is_ascii_lowercase())
        && text
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !text.ends_with('-')
        && !text.contains("--")
}

impl SourceId {
    /// # Errors
    ///
    /// Fails unless `slug` is lowercase letters, digits and single hyphens, starting with a letter
    /// and not ending with a hyphen.
    pub fn new(slug: impl Into<String>) -> Result<Self, IdError> {
        let slug = slug.into();
        if is_slug(&slug) {
            Ok(Self(Arc::from(slug)))
        } else {
            Err(IdError::SourceId(slug))
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for SourceId {
    type Error = IdError;
    fn try_from(slug: String) -> Result<Self, IdError> {
        Self::new(slug)
    }
}

impl From<SourceId> for String {
    fn from(id: SourceId) -> Self {
        id.0.as_ref().to_owned()
    }
}

impl fmt::Display for SourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Designation {
    /// # Errors
    ///
    /// Fails on an empty or blank identifier.
    pub fn new(source: SourceId, identifier: impl Into<Arc<str>>) -> Result<Self, IdError> {
        let identifier = identifier.into();
        if identifier.trim().is_empty() {
            Err(IdError::EmptyIdentifier(source))
        } else {
            Ok(Self { source, identifier })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_ids_are_slugs() {
        for good in ["gaia-dr3", "cosmicflows-4", "twomrs", "erass1-clusters"] {
            assert_eq!(SourceId::new(good).unwrap().as_str(), good);
        }
        for bad in [
            "",
            "Gaia",
            "4most",
            "gaia_dr3",
            "gaia-",
            "-gaia",
            "gaia--dr3",
            "gaia dr3",
        ] {
            assert_eq!(
                SourceId::new(bad),
                Err(IdError::SourceId(bad.to_owned())),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn object_ids_are_positive_and_safe_in_javascript() {
        assert_eq!(ObjectId::new(1).unwrap().get(), 1);
        assert_eq!(
            ObjectId::new((1 << 53) - 1).unwrap().to_string(),
            "9007199254740991"
        );
        assert!(ObjectId::new(0).is_err());
        assert!(ObjectId::new(1 << 53).is_err());
    }

    #[test]
    fn ids_read_through_serde_are_checked_too() {
        // Serde reads both through these conversions (`try_from` on the types).
        assert!(ObjectId::try_from(0).is_err());
        assert!(SourceId::try_from(String::from("Bad Slug")).is_err());
    }

    #[test]
    fn a_designation_keeps_its_source_and_needs_an_identifier() {
        let gaia = SourceId::new("gaia-dr3").unwrap();
        let d = Designation::new(gaia.clone(), "4295806720").unwrap();
        assert_eq!(d.source, gaia);
        assert!(Designation::new(gaia, "  ").is_err());
    }
}
