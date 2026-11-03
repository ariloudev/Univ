//! The matching rules of a release, read from `rules/matching.yaml`: the published
//! cross-identifications, the tolerances of each pair of sources and the confidence rule.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use serde::{Deserialize, Serialize};
use univ_model::entry::CatalogueEntry;
use univ_model::ids::SourceId;
use univ_model::object::{LineOfSight, MatchMethod};
use univ_model::position::separation_degrees;

use crate::yaml::{FileError, read_checked};

/// The matching rules, as the release records them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatchingRules {
    pub confidence: ConfidenceRule,
    pub cross_identifications: Vec<CrossIdentification>,
    pub designations: Vec<ExtraDesignation>,
    pub names: Vec<NameRule>,
    pub pairs: Vec<PairRule>,
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

/// The field of an entry an identifier is read from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryField {
    Identifier,
    Name,
}

impl EntryField {
    /// The field's text in `entry`, trimmed; none when blank or absent.
    #[must_use]
    pub fn read(self, entry: &CatalogueEntry) -> Option<&str> {
        let text = match self {
            Self::Identifier => Some(entry.designation.identifier.as_str()),
            Self::Name => entry.name.as_deref(),
        }?;
        Some(text.trim()).filter(|t| !t.is_empty())
    }
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
    pub members: Vec<SchemeMember>,
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
    pub basis: String,
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
    pub basis: String,
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

    /// The checks serde cannot make: positive tolerances, factors within [0, 1], each pair and
    /// each scheme member given once.
    ///
    /// # Errors
    ///
    /// Gives the field and the problem.
    pub fn check(&self) -> Result<(), (String, String)> {
        let c = &self.confidence;
        if !(c.exponent.is_finite() && c.exponent > 0.0) {
            return Err(field("confidence.exponent", "is not positive"));
        }
        for (name, v) in [
            ("unchecked_factor", c.unchecked_factor),
            (
                "cross_identification_conflict",
                c.cross_identification_conflict,
            ),
        ] {
            if !(0.0..=1.0).contains(&v) {
                return Err(field(&format!("confidence.{name}"), "lies outside [0, 1]"));
            }
        }
        let mut seen = BTreeSet::new();
        for (i, pair) in self.pairs.iter().enumerate() {
            let at = |what: &str| format!("pairs[{i}].{what}");
            let [a, b] = &pair.sources;
            if a == b {
                return Err(field(&at("sources"), "names one source twice"));
            }
            if !seen.insert(key(a, b)) {
                return Err(field(&at("sources"), "is a pair given before"));
            }
            if !positive(pair.angular_tolerance_arcsec) {
                return Err(field(&at("angular_tolerance_arcsec"), "is not positive"));
            }
            match pair.line_of_sight {
                LineOfSightRule::Velocity { tolerance_km_s: t }
                | LineOfSightRule::Distance {
                    tolerance_fraction: t,
                } if !positive(t) => {
                    return Err(field(&at("line_of_sight"), "has a tolerance not positive"));
                }
                _ => {}
            }
        }
        let mut members = BTreeSet::new();
        for (i, scheme) in self.cross_identifications.iter().enumerate() {
            for m in &scheme.members {
                if !members.insert((scheme.scheme.as_str(), m.source.as_str())) {
                    return Err(field(
                        &format!("cross_identifications[{i}].members"),
                        &format!("names {} twice", m.source),
                    ));
                }
            }
        }
        Ok(())
    }

    /// The pair rules by their sources, in either order.
    #[must_use]
    pub fn pair_index(&self) -> HashMap<(SourceId, SourceId), &PairRule> {
        self.pairs
            .iter()
            .map(|p| (key(&p.sources[0], &p.sources[1]), p))
            .collect()
    }

    /// The largest angular tolerance of any pair, degrees: the radius the sky index searches.
    #[must_use]
    pub fn search_radius_degrees(&self) -> f64 {
        self.pairs
            .iter()
            .map(|p| p.angular_tolerance_arcsec / 3_600.0)
            .fold(0.0, f64::max)
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
        let separation = separation_degrees(pa.icrs.unit_vector(), pb.icrs.unit_vector()) * 3_600.0;
        let theta = pair.angular_tolerance_arcsec;
        if separation > theta {
            return None;
        }
        let p = self.confidence.exponent;
        let (method, difference, factor) = match pair.line_of_sight {
            LineOfSightRule::None => (MatchMethod::Position, None, 1.0),
            LineOfSightRule::Velocity {
                tolerance_km_s: tau,
            } => match (velocity(a), velocity(b)) {
                (Some(va), Some(vb)) => {
                    let delta = (va - vb).abs();
                    if delta > tau {
                        return None;
                    }
                    let d = LineOfSight::Velocity { delta_km_s: delta };
                    (
                        MatchMethod::PositionAndRedshift,
                        Some(d),
                        1.0 - (delta / tau).powf(p),
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
                        1.0 - (delta / tau).powf(p),
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

fn positive(x: f64) -> bool {
    x.is_finite() && x > 0.0
}

fn field(name: &str, problem: &str) -> (String, String) {
    (name.to_owned(), problem.to_owned())
}

/// The entry's heliocentric cz in km/s: its first spectroscopic redshift, else its first
/// redshift, else its published velocity.
#[must_use]
pub fn velocity(entry: &CatalogueEntry) -> Option<f64> {
    use univ_model::entry::RedshiftKind;
    let c = univ_model::cosmology::SPEED_OF_LIGHT;
    entry
        .redshifts
        .iter()
        .find(|r| r.kind == RedshiftKind::Spectroscopic)
        .or_else(|| entry.redshifts.first())
        .map(|r| r.cmb.heliocentric.value * c)
        .or_else(|| entry.velocity.map(|v| v.normalized.value))
}

/// The entry's first distance in Mpc.
#[must_use]
pub fn distance(entry: &CatalogueEntry) -> Option<f64> {
    let d = entry.distances.first()?;
    d.normalized
        .to(univ_model::units::Unit::Megaparsec)
        .ok()
        .map(|q| q.value)
        .filter(|v| *v > 0.0)
}
