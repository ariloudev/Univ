//! What an object is, as its sources list it, and the typed relations between objects that
//! merge nothing: a galaxy group, the galaxy its source names as its dominant member, and the
//! galaxies a galaxy source assigns to it.

use serde::{Deserialize, Serialize};

use crate::ids::Designation;

/// What an object's sources list it as.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ObjectKind {
    /// One body: a star, a galaxy, or a cluster listed as one entry.
    #[default]
    Body,
    /// A group of galaxies, listed by a source of group rows whose values are the group's,
    /// such as a group-mean velocity or a group distance.
    GalaxyGroup,
}

/// How one object stands to another.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    /// The other object is this group's dominant galaxy, as the group's source names it.
    DominantMember,
    /// The other object is the group this galaxy dominates.
    DominantMemberOf,
    /// The other object is a galaxy its source assigns to this group.
    Member,
    /// The other object is the group this galaxy's source assigns it to.
    MemberOf,
}

/// A relation from this object's entry `entry` to the other object's entry `other`, found by
/// the identifier `identifier` of `scheme` that the group's source gives. The two objects stay
/// apart, and neither lends the other a measurement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Relation {
    pub kind: RelationKind,
    pub entry: Designation,
    pub other: Designation,
    pub scheme: String,
    pub identifier: String,
}
