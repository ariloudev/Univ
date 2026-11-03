//! The record of a structure outlined from a volume grid (RQ-23): the grid, the threshold and the
//! test that must admit the outline. Computing the outline and running the test come later; a
//! structure whose outline fails is published as a point at its published position.

use serde::{Deserialize, Serialize};
use univ_model::ids::SourceId;
use univ_model::units::Unit;

use super::cited::Cited;

/// How a structure is outlined from a volume grid.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawOutline")]
pub struct GridOutline {
    /// The source of the volume grid, such as `cf4pp-density`.
    pub grid: SourceId,
    /// The grid value that bounds the outline, cited from the paper whose level it follows.
    pub threshold: Cited,
    /// Whether the outline holds the cells below the threshold (a void) or above it.
    pub side: Side,
    pub test: OutlineTest,
}

/// The side of the threshold the outlined cells lie on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Below,
    Above,
}

/// The test that admits an outline: a comparison with the galaxy catalogues that cover it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutlineTest {
    /// The catalogues the outline is compared with, by source id.
    pub catalogues: Vec<SourceId>,
    /// What the comparison requires, in words, such as fewer galaxies than other cells at the
    /// same distance in every catalogue that covers the outline.
    pub rule: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawOutline {
    grid: SourceId,
    threshold: Cited,
    side: Side,
    test: OutlineTest,
}

impl TryFrom<RawOutline> for GridOutline {
    type Error = String;
    fn try_from(raw: RawOutline) -> Result<Self, String> {
        if raw.threshold.unit() != Unit::Dimensionless {
            return Err("the threshold is a grid value, without a unit".into());
        }
        if raw.test.catalogues.is_empty() {
            return Err("the outline test names the catalogues it compares with".into());
        }
        if raw.test.rule.trim().is_empty() {
            return Err("the outline test states its rule".into());
        }
        Ok(Self {
            grid: raw.grid,
            threshold: raw.threshold,
            side: raw.side,
            test: raw.test,
        })
    }
}
