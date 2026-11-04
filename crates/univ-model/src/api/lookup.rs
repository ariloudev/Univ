//! What an object lookup answers when it does not answer with one object (look-up-an-object):
//! the objects a name, a designation or a replaced id leads to, and where a merged id went.

use serde::{Deserialize, Serialize};

/// What the client looked up.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "by", rename_all = "snake_case")]
pub enum LookedUp {
    /// An object id.
    Id { id: u64 },
    /// A common name, as the client wrote it.
    Name { name: String },
    /// A source's designation: the source's id and the identifier it gives.
    Designation { source: String, designation: String },
}

/// Why a lookup leads to more than one object.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum Ambiguity {
    /// More than one object carries the name.
    SharedName,
    /// More than one object carries the designation.
    SharedDesignation,
    /// The id was replaced by more than one object of the release: split, or merged into an
    /// object later split.
    Split,
}

/// The objects a lookup leads to, answered with status 300 instead of choosing one of them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ObjectChoices {
    /// The release the lookup read.
    pub release: u32,
    pub looked_up: LookedUp,
    pub ambiguity: Ambiguity,
    /// For a replaced id, the release that replaced it.
    pub replaced_in: Option<u32>,
    /// For a replaced id, each change of the id ledger followed from it, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<LedgerChange>,
    /// The objects, by id, each with the path that reads it in the same release.
    pub objects: Vec<ObjectChoice>,
}

/// One object a lookup leads to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ObjectChoice {
    pub id: u64,
    /// The path of the object in the release the lookup read.
    pub href: String,
    /// The spellings of the name the object carries, for a name lookup.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub names: Vec<String>,
}

/// Where a merged id went, answered with status 301 and the object's path as `Location`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ObjectMerged {
    /// The release the lookup read.
    pub release: u32,
    /// The id looked up.
    pub id: u64,
    /// The release that replaced it.
    pub replaced_in: u32,
    /// The one object of the release the lookup read that replaced it.
    pub merged_into: u64,
    pub href: String,
    /// Each change of the id ledger followed from it, in order.
    pub history: Vec<LedgerChange>,
}

/// One change the id ledger records: what release `release` did to id `id`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct LedgerChange {
    pub id: u64,
    pub release: u32,
    /// `merged_into`, `split_into` or `retired`.
    pub change: String,
    /// The ids that replaced it; none for a retirement.
    pub replaced_by: Vec<u64>,
}
