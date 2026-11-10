//! The matching rules of a release, read from `rules/matching.yaml`: the published
//! cross-identifications, the tolerances of each pair of sources and the confidence rule.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use univ_model::entry::CatalogueEntry;
use univ_model::ids::SourceId;
use univ_model::object::{LineOfSight, MatchMethod, ObjectKind};
use univ_model::position::separation_degrees;

use crate::yaml::{FileError, read_checked};

pub use super::extent::ExtentRule;
pub use super::fields::EntryField;
pub use super::sight::{NearbyTolerance, distance};

/// The matching rules, as the release records them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatchingRules {
    pub confidence: ConfidenceRule,
    pub cross_identifications: Vec<CrossIdentification>,
    pub designations: Vec<ExtraDesignation>,
    pub galaxy_groups: Vec<GalaxyGroupRule>,
    pub names: Vec<NameRule>,
    pub structure_members: MemberRule,
    pub pairs: Vec<PairRule>,
    /// Where given, the pieces of a nearby galaxy within its published extent join it.
    #[serde(default)]
    pub extents: Option<ExtentRule>,
    /// The kind of object each tracer source's rows are, as its definition names it: not part
    /// of the rules file, but given by the run from the source definitions
    /// ([`MatchingRules::with_object_kinds`]).
    #[serde(skip)]
    pub object_kinds: BTreeMap<SourceId, ObjectKind>,
}

impl MatchingRules {
    /// The rules with the kind of object of each source of `definitions` that names one.
    #[must_use]
    pub fn with_object_kinds(mut self, definitions: &[crate::source::SourceDefinition]) -> Self {
        self.object_kinds = definitions
            .iter()
            .filter_map(|d| Some((d.id.clone(), d.object_kind?)))
            .collect();
        self
    }
}

/// A galaxy source whose field names, by the group rule's scheme, the group each galaxy
/// belongs to: the identifier the group source gives in its `dominant_member` field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupMemberRule {
    pub source: SourceId,
    pub field: EntryField,
    pub basis: String,
}

/// A source whose rows are galaxy groups, and the field naming each group's dominant galaxy by
/// an identifier of `scheme`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GalaxyGroupRule {
    pub source: SourceId,
    pub basis: String,
    pub dominant_member: EntryField,
    pub scheme: String,
    pub member_basis: String,
    /// Galaxy sources that assign their galaxies to these groups.
    #[serde(default)]
    pub members: Vec<GroupMemberRule>,
}

/// How far a researched structure's member may lie from the object its name resolves to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemberRule {
    pub angular_tolerance_arcsec: f64,
    pub basis: String,
}

/// How a match's confidence is computed, in words and as parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfidenceRule {
    pub statement: String,
    pub exponent: f64,
    pub exponent_basis: String,
    pub unchecked_factor: f64,
    pub unchecked_basis: String,
    pub cross_identification_conflict: f64,
    pub conflict_basis: String,
}

/// A source and the field holding its identifier in a scheme.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemeMember {
    pub source: SourceId,
    pub field: EntryField,
}

/// A published cross-identification: the sources whose entries share identifiers of one scheme.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrossIdentification {
    pub scheme: String,
    pub basis: String,
    /// How two identifiers of the scheme are compared.
    #[serde(default)]
    pub compare: IdentifierForm,
    /// Where given, two entries that share an identifier join only when their sky positions lie
    /// within this angle, in arcminutes; a pair farther apart is left apart as a candidate link.
    /// A pair one of whose entries has no position joins unchecked.
    #[serde(default)]
    pub angular_tolerance_arcmin: Option<f64>,
    /// Why the tolerance is what it is; required with it.
    #[serde(default)]
    pub tolerance_basis: Option<String>,
    /// For a scheme compared as common names, catalogue prefixes written another way, by their
    /// lower-case letters, and how they are compared: `messier` as `m`.
    #[serde(default)]
    pub aliases: BTreeMap<String, String>,
    /// Where the sources write a catalogue so; required with aliases.
    #[serde(default)]
    pub aliases_basis: Option<String>,
    pub members: Vec<SchemeMember>,
}

/// How the identifiers of a scheme are compared.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentifierForm {
    /// Trimmed, and a number without its leading zeros.
    #[default]
    Identifier,
    /// As a common name: by its key in the names table (lower case, without white space), with
    /// a catalogue number's leading zeros dropped and `messier` written `m`, so that `NGC0055`
    /// and `NGC 55`, or `MESSIER032` and `M32`, agree.
    CommonName,
}

/// A designation a source gives beside the entry's own identifier.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtraDesignation {
    pub source: SourceId,
    pub field: EntryField,
    pub prefix: String,
    pub basis: String,
}

/// A source whose field holds common names.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NameRule {
    pub source: SourceId,
    pub field: EntryField,
    /// What separates the names of a field that holds more than one, such as `,` in `OpenNGC`'s
    /// `Great Orion Nebula,Orion Nebula`; none where the field holds one name.
    #[serde(default)]
    pub separator: Option<char>,
    pub basis: String,
}

impl NameRule {
    /// The names the rule reads from `entry`'s field, each trimmed, none blank.
    pub fn names<'e>(&self, entry: &'e CatalogueEntry) -> impl Iterator<Item = &'e str> {
        let separator = self.separator;
        self.field
            .read(entry)
            .unwrap_or_default()
            .split(move |c| Some(c) == separator)
            .map(str::trim)
            .filter(|n| !n.is_empty())
    }
}

/// What a pair of sources compares along the line of sight.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "compare", deny_unknown_fields)]
pub enum LineOfSightRule {
    /// Nothing: the sky position alone.
    None,
    /// The heliocentric cz, within `tolerance_km_s`.
    Velocity { tolerance_km_s: f64 },
    /// The distance, |d1 − d2| over their mean within `tolerance_fraction`.
    Distance { tolerance_fraction: f64 },
}

/// The tolerances of one pair of sources.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairRule {
    pub sources: [SourceId; 2],
    pub angular_tolerance_arcsec: f64,
    pub line_of_sight: LineOfSightRule,
    /// A wider tolerance for two nearby entries, where given.
    #[serde(default)]
    pub nearby: Option<NearbyTolerance>,
    pub basis: String,
}

impl PairRule {
    /// The widest angle the pair matches at, its nearby tolerance included, degrees.
    #[must_use]
    pub fn widest_degrees(&self) -> f64 {
        let nearby = self.nearby.as_ref().map(|n| n.angular_tolerance_arcsec);
        self.angular_tolerance_arcsec.max(nearby.unwrap_or(0.0)) / 3_600.0
    }
}

/// What comparing two entries under a pair rule found: within the tolerances, with this
/// separation, difference and confidence.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Comparison {
    pub method: MatchMethod,
    pub separation_arcsec: f64,
    pub difference: Option<LineOfSight>,
    pub confidence: f64,
}

impl MatchingRules {
    /// Reads and checks the rules in `file`.
    ///
    /// # Errors
    ///
    /// Fails naming the field when the file does not parse or a rule cannot stand.
    pub fn read(file: &Path) -> Result<Self, FileError> {
        read_checked(file, Self::check)
    }

    /// The cross-identification scheme named `name`.
    #[must_use]
    pub fn scheme(&self, name: &str) -> Option<&CrossIdentification> {
        self.cross_identifications.iter().find(|s| s.scheme == name)
    }

    /// The galaxy-group rule of `source`, if its rows are groups.
    #[must_use]
    pub fn galaxy_group(&self, source: &SourceId) -> Option<&GalaxyGroupRule> {
        self.galaxy_groups.iter().find(|g| &g.source == source)
    }

    /// The pair rule of each two of `sources`, in either order, at `a * n + b` for the sources
    /// numbered `a` and `b` of the `n`.
    #[must_use]
    pub fn pair_table(&self, sources: &[SourceId]) -> Vec<Option<&PairRule>> {
        let n = sources.len();
        let mut table = vec![None; n * n];
        for pair in &self.pairs {
            let at = |s: &SourceId| sources.iter().position(|x| x == s);
            if let (Some(a), Some(b)) = (at(&pair.sources[0]), at(&pair.sources[1])) {
                table[a * n + b] = Some(pair);
                table[b * n + a] = Some(pair);
            }
        }
        table
    }

    /// The line-of-sight factor of a difference `delta` against its tolerance `tolerance`:
    /// 1 when they agree exactly, falling to 0 at the tolerance, by the rule's exponent.
    #[must_use]
    pub fn line_of_sight_factor(&self, delta: f64, tolerance: f64) -> f64 {
        1.0 - (delta / tolerance).powf(self.confidence.exponent)
    }

    /// The confidence of a match at `separation` of tolerance `tolerance`, and the line-of-sight
    /// factor `line_of_sight`: the rule the file states.
    #[must_use]
    pub fn positional_confidence(
        &self,
        separation: f64,
        tolerance: f64,
        line_of_sight: f64,
    ) -> f64 {
        let p = self.confidence.exponent;
        ((1.0 - (separation / tolerance).powf(p)) * line_of_sight).clamp(0.0, 1.0)
    }

    /// Compares `a` and `b` under `pair`: none when either lacks a position or they lie beyond
    /// a tolerance.
    #[must_use]
    pub fn compare(
        &self,
        pair: &PairRule,
        a: &CatalogueEntry,
        b: &CatalogueEntry,
    ) -> Option<Comparison> {
        let (pa, pb) = (a.position.as_ref()?, b.position.as_ref()?);
        let separation = separation_degrees(pa.icrs.unit_vector(), pb.icrs.unit_vector());
        self.compare_at(pair, a, b, separation * 3_600.0)
    }

    /// Compares `a` and `b`, `separation` arcseconds apart, under `pair`: none when they lie
    /// beyond a tolerance.
    #[must_use]
    pub fn compare_at(
        &self,
        pair: &PairRule,
        a: &CatalogueEntry,
        b: &CatalogueEntry,
        separation: f64,
    ) -> Option<Comparison> {
        let theta = self.angular_tolerance(pair, a, b);
        if separation > theta {
            return None;
        }
        let (method, difference, factor) = match pair.line_of_sight {
            LineOfSightRule::None => (MatchMethod::Position, None, 1.0),
            LineOfSightRule::Velocity {
                tolerance_km_s: tau,
            } => match (self.velocity(a), self.velocity(b)) {
                (Some(va), Some(vb)) => {
                    let delta = (va - vb).abs();
                    if delta > tau {
                        return None;
                    }
                    let d = LineOfSight::Velocity { delta_km_s: delta };
                    (
                        MatchMethod::PositionAndRedshift,
                        Some(d),
                        self.line_of_sight_factor(delta, tau),
                    )
                }
                _ => (
                    MatchMethod::Position,
                    None,
                    self.confidence.unchecked_factor,
                ),
            },
            LineOfSightRule::Distance {
                tolerance_fraction: tau,
            } => match (distance(a), distance(b)) {
                (Some(da), Some(db)) => {
                    let delta = (da - db).abs() / da.midpoint(db);
                    if delta > tau {
                        return None;
                    }
                    let d = LineOfSight::Distance {
                        delta_fraction: delta,
                    };
                    (
                        MatchMethod::PositionAndDistance,
                        Some(d),
                        self.line_of_sight_factor(delta, tau),
                    )
                }
                _ => (
                    MatchMethod::Position,
                    None,
                    self.confidence.unchecked_factor,
                ),
            },
        };
        Some(Comparison {
            method,
            separation_arcsec: separation,
            difference,
            confidence: self.positional_confidence(separation, theta, factor),
        })
    }
}

/// The unordered key of a pair of sources.
#[must_use]
pub fn key(a: &SourceId, b: &SourceId) -> (SourceId, SourceId) {
    if a <= b {
        (a.clone(), b.clone())
    } else {
        (b.clone(), a.clone())
    }
}
