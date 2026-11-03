//! The record of a structure outlined from a volume grid (RQ-23): the grid, the threshold and the
//! test that must admit the outline. [`crate::outlines`] computes the outline and runs the test;
//! a structure whose outline fails is published as a point at its published position.

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};
use univ_model::ids::SourceId;
use univ_model::units::Unit;

use super::cited::Cited;
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
    Cited(Cited),
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

/// A threshold is chosen when it has `chosen`, and is otherwise read as a cited value, so that a
/// bare number is refused as citing nothing.
impl<'de> Deserialize<'de> for Threshold {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_norway::Value::deserialize(deserializer)?;
        let chosen = value.as_mapping().is_some_and(|m| m.contains_key("chosen"));
        let threshold = if chosen {
            Choice::deserialize(value)
                .map(Self::Chosen)
                .map_err(de::Error::custom)?
        } else {
            Cited::deserialize(value)
                .map(Self::Cited)
                .map_err(de::Error::custom)?
        };
        threshold.check().map_err(de::Error::custom)?;
        Ok(threshold)
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
