//! An object: one body resolved from the catalogue entries that several sources hold for it,
//! each entry kept with how it was resolved, the candidate matches kept apart from it, its
//! designations and names with their sources, and its preferred measurements.

mod preferred;
mod relation;
mod resolved;

use serde::{Deserialize, Serialize};
use thin_vec::ThinVec;
use thiserror::Error;

pub use preferred::{
    Decided, FractionalUncertainty, MeasurementRef, MethodRank, NotPlacedReason, Placement,
    PositionClass, PreferredDistance, PreferredPosition,
};
pub use relation::{ObjectKind, Relation, RelationKind};
pub use resolved::Resolved;

use crate::entry::CatalogueEntry;
use crate::ids::{Designation, SourceId};

/// A confidence in a match, between 0 and 1. Checked when built and when read.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema), schema(value_type = f64))]
#[serde(try_from = "f64", into = "f64")]
pub struct Confidence(f64);

/// A confidence outside [0, 1].
#[derive(Debug, Error, PartialEq)]
#[error("the confidence {0} lies outside [0, 1]")]
pub struct ConfidenceError(pub f64);

impl Confidence {
    /// # Errors
    ///
    /// Fails on a value outside [0, 1] or not a number.
    pub fn new(value: f64) -> Result<Self, ConfidenceError> {
        if (0.0..=1.0).contains(&value) {
            Ok(Self(value))
        } else {
            Err(ConfidenceError(value))
        }
    }

    /// Certainty, as a published cross-identification gives.
    pub const CERTAIN: Self = Self(1.0);

    /// No confidence at all.
    pub const NONE: Self = Self(0.0);

    #[must_use]
    pub fn get(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for Confidence {
    type Error = ConfidenceError;
    fn try_from(value: f64) -> Result<Self, ConfidenceError> {
        Self::new(value)
    }
}

impl From<Confidence> for f64 {
    fn from(c: Confidence) -> Self {
        c.0
    }
}

/// How two entries were found to agree along the line of sight.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum MatchMethod {
    /// By sky position alone: the rule checks nothing else, or one entry lacks the value the
    /// rule compares along the line of sight.
    Position,
    /// By sky position and heliocentric recession velocity cz.
    PositionAndRedshift,
    /// By sky position and distance.
    PositionAndDistance,
    /// As a piece of a nearby galaxy: within the extent its source publishes, at a heliocentric
    /// cz agreeing with the galaxy's.
    WithinExtent,
}

/// The difference along the line of sight between two matched entries.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum LineOfSight {
    /// |Δcz| between the heliocentric velocities, km/s.
    Velocity { delta_km_s: f64 },
    /// |d1 − d2| over their mean, of the first distance of each entry.
    Distance { delta_fraction: f64 },
}

/// A positional match: the entry it was compared with and what the comparison found.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct MatchRecord {
    pub method: MatchMethod,
    /// The entry on the other side of the match.
    pub with: Designation,
    /// The angular separation of the two ICRS positions, arcseconds.
    pub separation_arcsec: f64,
    /// The difference along the line of sight, when both entries carry the value compared.
    pub difference: Option<LineOfSight>,
    pub confidence: Confidence,
}

/// How an entry came to belong to its object, tagged by `by`: a match keeps its own `method`
/// beside the tag, so the tag cannot be `method` too.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case", tag = "by")]
pub enum Resolution {
    /// The entry the object was resolved around: the first of its entries in the order read.
    Anchor,
    /// A published cross-identification: the entry shares the identifier `identifier` of
    /// `scheme`, such as a Gaia `source_id` or a HIP number, with `via`.
    CrossIdentification {
        scheme: String,
        identifier: String,
        via: Designation,
        confidence: Confidence,
    },
    /// A match by position, and by redshift or distance where the rule compares one.
    Match(MatchRecord),
}

/// A catalogue entry of an object, with how it was resolved into it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObjectEntry {
    pub entry: CatalogueEntry,
    pub resolved_by: Resolved,
}

/// A common name, such as M31 or the Large Magellanic Cloud, with the source that published it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CommonName {
    pub name: String,
    pub source: SourceId,
}

/// A candidate match to another object: both stay separate objects, and no measurement is
/// merged across the link.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CandidateLink {
    /// The entry of this object that the link starts from.
    pub entry: Designation,
    /// The entry of the other object.
    pub candidate: Designation,
    /// What made it a candidate: a match record, or a cross-identification that would have
    /// joined two entries of one source.
    pub reason: Resolution,
    pub confidence: Confidence,
}

/// One body resolved from its catalogue entries: a star, a galaxy or a cluster as its sources
/// list it, or a galaxy group where a source lists groups.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Object {
    /// What its sources list it as.
    #[serde(default)]
    pub kind: ObjectKind,
    /// Its catalogue entries, the anchor first, each with how it was resolved.
    ///
    /// This and the lists below are thin: 8 bytes each, as an object is held for each of tens
    /// of millions, and most have one entry and none of the rest.
    pub entries: ThinVec<ObjectEntry>,
    /// The designations its entries' sources give beside the entries' own, such as the HIP
    /// number of the Gaia–Hipparcos table; [`Object::designations`] gives every one. An entry's
    /// own is not copied here: an object is held for each of tens of millions.
    pub given: ThinVec<Designation>,
    /// Common names, each with the source that published it.
    pub names: ThinVec<CommonName>,
    /// Candidate matches to other objects.
    pub links: ThinVec<CandidateLink>,
    /// Typed relations to other objects, such as a group's dominant galaxy: neither object
    /// lends the other a measurement.
    #[serde(default, skip_serializing_if = "ThinVec::is_empty")]
    pub relations: ThinVec<Relation>,
    pub preferred_distance: Option<PreferredDistance>,
    pub preferred_position: Option<PreferredPosition>,
    pub placement: Placement,
}

impl Object {
    /// Each designation its entries carry, with the source that gave it: each entry's own, in
    /// the order of its entries, then those their sources give beside them.
    pub fn designations(&self) -> impl Iterator<Item = &Designation> {
        self.entries
            .iter()
            .map(|e| &e.entry.designation)
            .chain(&self.given)
    }

    /// An object of `entries`, unranked: no preferred measurement yet, and not placed for
    /// lack of a distance until a ranking gives one.
    #[must_use]
    pub fn new(entries: Vec<ObjectEntry>) -> Self {
        Self {
            kind: ObjectKind::default(),
            entries: ThinVec::from(entries),
            given: ThinVec::new(),
            names: ThinVec::new(),
            links: ThinVec::new(),
            relations: ThinVec::new(),
            preferred_distance: None,
            preferred_position: None,
            placement: Placement::NotPlaced {
                reason: NotPlacedReason::NoDistance,
            },
        }
    }

    /// The entry designated `designation`.
    #[must_use]
    pub fn entry(&self, designation: &Designation) -> Option<&ObjectEntry> {
        self.entries
            .iter()
            .find(|e| &e.entry.designation == designation)
    }

    /// The sources of its entries.
    pub fn sources(&self) -> impl Iterator<Item = &SourceId> {
        self.entries.iter().map(|e| &e.entry.designation.source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_confidence_lies_in_zero_to_one() {
        assert_eq!(Confidence::new(0.25).unwrap().get(), 0.25);
        assert!(Confidence::new(1.0001).is_err());
        assert!(Confidence::new(-0.1).is_err());
        assert!(Confidence::new(f64::NAN).is_err());
        assert!(Confidence::try_from(2.0).is_err(), "read through serde too");
    }
}
