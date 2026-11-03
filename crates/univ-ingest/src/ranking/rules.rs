//! The ranking rule of a release, read from `rules/ranking.yaml` and recorded with its
//! parameters in the release manifest.

use std::path::Path;

use serde::{Deserialize, Serialize};
use univ_model::ids::SourceId;
use univ_model::object::{MethodRank, PositionClass};

use crate::yaml::{FileError, read_checked};

/// The ranking rule and its parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RankingRules {
    pub rule: String,
    pub statement: String,
    pub peculiar_velocity_km_s: f64,
    pub peculiar_velocity_basis: String,
    pub method_rank: Vec<MethodRank>,
    pub method_rank_basis: String,
    pub tie_tolerance: f64,
    pub tie_basis: String,
    /// Where a geometric or indicator distance is a candidate, every redshift distance whose
    /// fractional uncertainty exceeds this fraction is set aside.
    pub redshift_threshold: f64,
    pub redshift_threshold_basis: String,
    /// The order of sources that decides between candidates published without an uncertainty
    /// that share the best method rank; a source not listed comes after every listed one.
    pub source_order: Vec<SourceId>,
    pub source_order_basis: String,
    /// An object whose preferred distance has a fractional uncertainty above this is not placed.
    pub too_uncertain_above: f64,
    pub too_uncertain_basis: String,
    pub parallax_max_fractional_error: f64,
    pub parallax_basis: String,
    pub parallax_zero_point: ZeroPointRule,
    pub position: PositionRule,
}

/// The parallax zero point taken off the parallaxes of some sources before they are inverted.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZeroPointRule {
    pub sources: Vec<SourceId>,
    /// The zero point Z in mas: the corrected parallax is ϖ − Z.
    pub zero_point_mas: f64,
    pub basis: String,
}

/// Which sources give Gaia and Hipparcos positions; every other source gives a survey position.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PositionRule {
    pub statement: String,
    pub gaia: Vec<SourceId>,
    pub hipparcos: Vec<SourceId>,
    pub basis: String,
}

const ALL_RANKS: [MethodRank; 5] = [
    MethodRank::Geometric,
    MethodRank::PrimaryIndicator,
    MethodRank::SecondaryIndicator,
    MethodRank::SpectroscopicRedshift,
    MethodRank::PhotometricRedshift,
];

impl RankingRules {
    /// Reads and checks the rule in `file`.
    ///
    /// # Errors
    ///
    /// Fails naming the field when the file does not parse or a parameter cannot stand.
    pub fn read(file: &Path) -> Result<Self, FileError> {
        read_checked(file, Self::check)
    }

    /// The checks serde cannot make.
    ///
    /// # Errors
    ///
    /// Gives the field and the problem.
    pub fn check(&self) -> Result<(), (String, String)> {
        let bad = |f: &str, p: &str| Err((f.to_owned(), p.to_owned()));
        let v = self.peculiar_velocity_km_s;
        if !(v.is_finite() && v >= 0.0) {
            return bad("peculiar_velocity_km_s", "is negative or not a number");
        }
        if !(self.tie_tolerance.is_finite() && self.tie_tolerance >= 0.0) {
            return bad("tie_tolerance", "is negative or not a number");
        }
        let t = self.redshift_threshold;
        if !(t.is_finite() && t > 0.0) {
            return bad("redshift_threshold", "is not a positive number");
        }
        let u = self.too_uncertain_above;
        if !(u.is_finite() && u > 0.0) {
            return bad("too_uncertain_above", "is not a positive number");
        }
        let mut seen = std::collections::HashSet::new();
        if let Some(s) = self.source_order.iter().find(|s| !seen.insert(*s)) {
            return bad("source_order", &format!("names {s} twice"));
        }
        let p = self.parallax_max_fractional_error;
        if !(p > 0.0 && p <= 1.0) {
            return bad("parallax_max_fractional_error", "lies outside (0, 1]");
        }
        if !self.parallax_zero_point.zero_point_mas.is_finite() {
            return bad("parallax_zero_point.zero_point_mas", "is not a number");
        }
        if self.method_rank.len() != ALL_RANKS.len()
            || ALL_RANKS.iter().any(|r| !self.method_rank.contains(r))
        {
            return bad("method_rank", "does not name each of the five methods once");
        }
        if let Some(s) = self
            .position
            .gaia
            .iter()
            .find(|s| self.position.hipparcos.contains(s))
        {
            return bad("position", &format!("names {s} both Gaia and Hipparcos"));
        }
        Ok(())
    }

    /// The place of `rank` in the rule's order, 0 best.
    #[must_use]
    pub fn rank_of(&self, rank: MethodRank) -> usize {
        self.method_rank
            .iter()
            .position(|r| *r == rank)
            .unwrap_or(self.method_rank.len())
    }

    /// The place of `source` in the rule's order of sources, 0 first; a source not listed after
    /// every listed one.
    #[must_use]
    pub fn source_place(&self, source: &SourceId) -> usize {
        self.source_order
            .iter()
            .position(|s| s == source)
            .unwrap_or(self.source_order.len())
    }

    /// The parallax zero point of `source`, mas, where the rule gives it one.
    #[must_use]
    pub fn zero_point_of(&self, source: &SourceId) -> Option<f64> {
        let z = &self.parallax_zero_point;
        z.sources.contains(source).then_some(z.zero_point_mas)
    }

    /// The position class of `source`.
    #[must_use]
    pub fn class_of(&self, source: &SourceId) -> PositionClass {
        if self.position.gaia.contains(source) {
            PositionClass::Gaia
        } else if self.position.hipparcos.contains(source) {
            PositionClass::Hipparcos
        } else {
            PositionClass::Survey
        }
    }

    /// The rule and its parameters as the release manifest records them, in YAML.
    ///
    /// # Errors
    ///
    /// Fails only if the rule cannot be written, which a checked rule always can.
    pub fn record(&self) -> Result<String, serde_norway::Error> {
        serde_norway::to_string(self)
    }
}
