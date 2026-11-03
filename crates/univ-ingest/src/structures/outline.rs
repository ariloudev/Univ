//! The record of a structure outlined from a volume grid (RQ-23): the grid, the threshold and the
//! test that must admit the outline. [`crate::outlines`] computes the outline and runs the test;
//! a structure whose outline fails is published as a point at its published position.

use std::fmt;

use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::{Deserialize, Serialize};
use univ_model::ids::SourceId;
use univ_model::units::Unit;
use univ_model::value::{KindOfValue, Uncertainty};

use super::citation::Citation;
use super::cited::{Assumes, Cited, RawCited, uncited};
use super::geometry::Coordinates;

/// How a structure is outlined from a volume grid.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawOutline")]
pub struct GridOutline {
    /// The source of the volume grid, such as `cf4pp-density`.
    pub grid: SourceId,
    /// The grid field the threshold applies to, by its field in the source definition, such as
    /// `density`.
    pub field: String,
    /// The grid value that bounds the outline.
    pub threshold: Threshold,
    /// Whether the outline holds the cells below the threshold (a void) or above it.
    pub side: Side,
    /// A named point the region must hold instead of the published position, such as a density
    /// minimum the paper names; the region holds the published position when none is given.
    pub seed: Option<Seed>,
    pub test: OutlineTest,
}

/// The threshold of an outline: a level a paper gives, cited, or a choice of this pipeline, said
/// to be one and why, never dressed as a citation.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Threshold {
    Cited(Box<Cited>),
    Chosen(Choice),
}

/// A number this pipeline chose, not one a paper published, with the reason for it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Choice {
    /// The number chosen, as a grid value without a unit.
    pub chosen: f64,
    pub why: String,
}

impl Threshold {
    /// The level, in the grid's own units.
    #[must_use]
    pub fn value(&self) -> f64 {
        match self {
            Self::Cited(c) => c.value(),
            Self::Chosen(c) => c.chosen,
        }
    }

    fn check(&self) -> Result<(), String> {
        match self {
            Self::Cited(c) if c.unit() != Unit::Dimensionless => {
                Err("the threshold is a grid value, without a unit".into())
            }
            Self::Chosen(c) if !c.chosen.is_finite() => {
                Err("the chosen threshold is finite".into())
            }
            Self::Chosen(c) if c.why.trim().is_empty() => {
                Err("a chosen threshold says why it was chosen".into())
            }
            _ => Ok(()),
        }
    }
}

/// A threshold as written: a choice (`chosen`, `why`) or a cited value, read in one pass so
/// that a refusal points at the threshold itself.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawThreshold {
    #[serde(default)]
    chosen: Option<f64>,
    #[serde(default)]
    why: Option<String>,
    #[serde(default)]
    value: Option<f64>,
    #[serde(default)]
    unit: Option<Unit>,
    #[serde(default)]
    kind: Option<KindOfValue>,
    #[serde(default)]
    uncertainty: Option<Uncertainty>,
    #[serde(default)]
    cite: Option<Citation>,
    #[serde(default)]
    as_published: Option<String>,
    #[serde(default)]
    uncertainty_cite: Option<Citation>,
    #[serde(default)]
    assumes: Option<Assumes>,
    #[serde(default)]
    note: Option<String>,
}

impl TryFrom<RawThreshold> for Threshold {
    type Error = String;
    fn try_from(raw: RawThreshold) -> Result<Self, String> {
        let threshold = if let Some(chosen) = raw.chosen {
            let cited = raw.value.is_some() || raw.unit.is_some() || raw.cite.is_some();
            if cited {
                return Err("a chosen threshold cites nothing: give chosen and why only".into());
            }
            Self::Chosen(Choice {
                chosen,
                why: raw.why.ok_or("missing field `why`")?,
            })
        } else {
            if raw.why.is_some() {
                return Err("why is given for a chosen threshold, which needs `chosen`".into());
            }
            let missing = |field: &str| format!("missing field `{field}`");
            let cited = RawCited {
                value: raw.value.ok_or_else(|| missing("value"))?,
                unit: raw.unit.ok_or_else(|| missing("unit"))?,
                kind: raw.kind.ok_or_else(|| missing("kind"))?,
                uncertainty: raw.uncertainty.ok_or_else(|| missing("uncertainty"))?,
                cite: raw.cite,
                as_published: raw.as_published,
                uncertainty_cite: raw.uncertainty_cite,
                assumes: raw.assumes,
                note: raw.note,
            };
            Self::Cited(Box::new(Cited::try_from(cited)?))
        };
        threshold.check()?;
        Ok(threshold)
    }
}

/// Reads a threshold, and names a bare number as uncited rather than as a type mismatch.
struct ThresholdVisitor;

impl<'de> Visitor<'de> for ThresholdVisitor {
    type Value = Threshold;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a cited threshold, or a chosen one with why")
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "only echoed in the refusal message"
    )]
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Threshold, E> {
        Err(E::custom(uncited(v as f64)))
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "only echoed in the refusal message"
    )]
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Threshold, E> {
        Err(E::custom(uncited(v as f64)))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Threshold, E> {
        Err(E::custom(uncited(v)))
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Threshold, A::Error> {
        let raw = RawThreshold::deserialize(de::value::MapAccessDeserializer::new(map))?;
        Threshold::try_from(raw).map_err(de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for Threshold {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(ThresholdVisitor)
    }
}

/// A named point, such as a density minimum, at cited coordinates.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Seed {
    pub name: String,
    pub position: Coordinates,
}

/// The side of the threshold the outlined cells lie on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Below,
    Above,
}

/// What the comparison with the catalogues must find.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rule {
    /// Every covering catalogue finds fewer galaxies inside than `ratio` times the expectation.
    UnderdenseInEveryCoveringCatalogue,
    /// Every covering catalogue finds more galaxies inside than `ratio` times the expectation.
    OverdenseInEveryCoveringCatalogue,
}

/// The test that admits an outline: a comparison with the galaxy catalogues that cover it. Its
/// numbers are choices of this pipeline, given with the reason for them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutlineTest {
    /// The catalogues the outline is compared with, by source id.
    pub catalogues: Vec<SourceId>,
    pub rule: Rule,
    /// The bound on galaxies held over galaxies expected that the rule compares with.
    pub ratio: f64,
    /// The fewest galaxies a catalogue must expect inside the outline to judge it; a sparser one
    /// does not cover it.
    pub min_expected: f64,
    /// Why these numbers: they are this pipeline's choices, not published values.
    pub why: String,
    /// The dense places a void's region may not hold, where the record states them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dense_places: Option<DensePlaces>,
}

/// The dense places an outline below its threshold, a void, may not hold: a choice of this
/// pipeline, said to be one and why. A region that holds one is refused, and the structure is
/// shown at its published position.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DensePlaces {
    /// Always `pipeline`: the rule is a choice of this pipeline, not a published one.
    pub chosen_by: ChosenBy,
    /// Sources whose matched objects are dense places, such as a catalogue of galaxy clusters:
    /// each object holding one of their entries, at that entry's CMB-frame redshift.
    #[serde(default)]
    pub sources: Vec<SourceId>,
    /// Kinds of researched structure that are dense places, each at its published position.
    #[serde(default)]
    pub structure_kinds: Vec<super::StructureKind>,
    pub why: String,
}

/// Who chose a rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChosenBy {
    Pipeline,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawOutline {
    grid: SourceId,
    #[serde(default = "density")]
    field: String,
    threshold: Threshold,
    side: Side,
    #[serde(default)]
    seed: Option<Seed>,
    test: OutlineTest,
}

fn density() -> String {
    "density".to_owned()
}

impl TryFrom<RawOutline> for GridOutline {
    type Error = String;
    fn try_from(raw: RawOutline) -> Result<Self, String> {
        let test = &raw.test;
        if test.catalogues.is_empty() {
            return Err("the outline test names the catalogues it compares with".into());
        }
        let fits = match test.rule {
            Rule::UnderdenseInEveryCoveringCatalogue => raw.side == Side::Below,
            Rule::OverdenseInEveryCoveringCatalogue => raw.side == Side::Above,
        };
        if !fits {
            return Err(
                "an outline below its threshold is tested for underdensity, one above it for \
                 overdensity"
                    .into(),
            );
        }
        let positive = |x: f64| x.is_finite() && x > 0.0;
        if !positive(test.ratio) || !positive(test.min_expected) {
            return Err("the test's ratio and min_expected are positive".into());
        }
        if test.why.trim().is_empty() {
            return Err("the outline test says why it chose its numbers".into());
        }
        if let Some(dense) = &test.dense_places {
            if raw.side != Side::Below {
                return Err(
                    "dense places are a test of a void, an outline below its threshold".into(),
                );
            }
            if dense.sources.is_empty() && dense.structure_kinds.is_empty() {
                return Err("the dense places name sources or structure kinds".into());
            }
            if dense.why.trim().is_empty() {
                return Err("the dense places say why they were chosen".into());
            }
        }
        if raw.field.trim().is_empty() || raw.seed.as_ref().is_some_and(|s| s.name.is_empty()) {
            return Err("the field and the seed's name are not empty".into());
        }
        Ok(Self {
            grid: raw.grid,
            field: raw.field,
            threshold: raw.threshold,
            side: raw.side,
            seed: raw.seed,
            test: raw.test,
        })
    }
}
