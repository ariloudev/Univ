//! Structures outlined from a volume grid (RQ-23, AC-58, AC-59): the connected region of grid
//! cells on the stated side of the threshold that holds the structure's published position (or
//! the named seed its record states), tested against the galaxy catalogues that cover it. An
//! outline the test admits is a univ-model grid region in the grid's frame, reconstructed; any
//! other outcome publishes the structure as a point at its published position, with the reason.

mod compare;
mod dense;
mod region;

use serde::Serialize;
use thiserror::Error;
use univ_model::entry::CatalogueEntry;
use univ_model::geometry::Geometry;
use univ_model::grid::{Axis, Grid, GridGeometry};
use univ_model::ids::SourceId;
use univ_model::position::{Frame, Vec3};
use univ_model::units::Unit;
use univ_model::value::KindOfValue;

pub use compare::{COVERAGE, CatalogueCheck, Coverage, CoverageRule, GridSky, compare};
pub use dense::{DenseAt, DenseCheck, DensePlace, dense_places};
pub use region::{Region, on_grid, on_side, region, scalar};

use crate::structures::{Coordinates, GridOutline, OutlineTest, Side, Structure, Threshold};

/// What became of a structure outlined from a grid, recorded with it: the grid, threshold and
/// test, the seed, the region, each catalogue's numbers and the verdict.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OutlineResult {
    pub structure: String,
    pub grid: SourceId,
    pub field: String,
    pub threshold: Threshold,
    pub side: Side,
    pub test: OutlineTest,
    pub coverage: CoverageRule,
    /// Where the region was started, when the seed could be placed on the grid.
    pub seed: Option<SeedCell>,
    /// Cells in the region; 0 when the seed is not on the stated side.
    pub cells: usize,
    /// Whether the region reaches a face of the grid, which may cut it.
    pub touches_grid_edge: bool,
    pub catalogues: Vec<CatalogueCheck>,
    /// What the check of the dense places found, when the record states them.
    pub dense: Option<DenseCheck>,
    pub placed: Placed,
}

/// The seed of a region on the grid.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SeedCell {
    /// The published position, or the named seed of the record.
    pub name: String,
    /// The seed in the grid's frame and unit.
    pub at: Vec3,
    /// Its cell, by indices along X, Y and Z of the frame.
    pub cell: [usize; 3],
    /// The field's value in that cell.
    pub value: f64,
}

/// How the structure is published.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "as")]
pub enum Placed {
    /// The outline, admitted by its test.
    Outline {
        frame: Frame,
        unit: Unit,
        kind: KindOfValue,
        geometry: Geometry,
    },
    /// The published position, because the outline could not be drawn or failed its test.
    Point {
        position: Box<Coordinates>,
        reason: String,
    },
}

/// A structure and grid that cannot be outlined at all: a fault of the record or the call, not
/// an outline that failed its test.
#[derive(Debug, Error, PartialEq)]
pub enum OutlineError {
    #[error("{0} has no grid outline record or no published position")]
    NoRecord(String),
    #[error("{structure} is outlined on {expected}, not on {given}")]
    WrongGrid {
        structure: String,
        expected: SourceId,
        given: String,
    },
    #[error("{0}")]
    Field(String),
}

/// Outlines `structure` on `grid` and tests the outline against `catalogues`, the normalized
/// entries of each catalogue by source; a catalogue of the test not among them does not cover
/// the outline. No dense place is given: see [`place_on`].
///
/// # Errors
///
/// As [`place_on`].
pub fn place(
    structure: &Structure,
    grid: &Grid,
    catalogues: &[(SourceId, &[CatalogueEntry])],
) -> Result<OutlineResult, OutlineError> {
    place_on(
        structure,
        grid,
        &GridSky::new(&grid.geometry),
        catalogues,
        &[],
    )
}

/// [`place`] with the grid's cells described once in `sky`, for every structure outlined on it,
/// and `dense`, the dense places the record's rule picked (see [`dense_places`]): a void whose
/// region holds one is shown as a point, with the reason.
///
/// # Errors
///
/// Fails when the structure has no outline record, `grid` is not the record's grid, or the grid
/// lacks the record's field as one real number per cell.
pub fn place_on(
    structure: &Structure,
    grid: &Grid,
    sky: &GridSky<'_>,
    catalogues: &[(SourceId, &[CatalogueEntry])],
    dense: &[DensePlace],
) -> Result<OutlineResult, OutlineError> {
    let (Some(record), Some(published)) = (&structure.outline, &structure.position) else {
        return Err(OutlineError::NoRecord(structure.name.clone()));
    };
    if record.grid.as_str() != grid.source {
        return Err(OutlineError::WrongGrid {
            structure: structure.name.clone(),
            expected: record.grid.clone(),
            given: grid.source.clone(),
        });
    }
    let values = scalar(grid, &record.field).map_err(OutlineError::Field)?;
    let geometry = &grid.geometry;
    let mut result = OutlineResult::untested(&structure.name, record, published);
    let point = |mut result: OutlineResult, reason: String| {
        result.placed = Placed::Point {
            position: Box::new(published.clone()),
            reason,
        };
        Ok(result)
    };
    let (name, position) = record.seed.as_ref().map_or_else(
        || ("the published position".to_owned(), published),
        |s| (s.name.clone(), &s.position),
    );
    let seed = match seed_cell(grid, values, name, position) {
        Ok(seed) => seed,
        Err(why) => return point(result, why),
    };
    let (cell, value) = (seed.cell, seed.value);
    result.seed = Some(seed);
    let threshold = record.threshold.value();
    if !on_side(value, threshold, record.side) {
        let side = match record.side {
            Side::Below => "below",
            Side::Above => "above",
        };
        let name = result
            .seed
            .as_ref()
            .map(|s| s.name.as_str())
            .unwrap_or_default();
        let why = format!(
            "{name} lies in cell {cell:?} of {}, where {} is {value:.3}, not {side} the threshold \
             {threshold}",
            grid.source, record.field
        );
        return point(result, why);
    }
    let outlined = region(geometry, values, cell, threshold, record.side);
    (result.cells, result.touches_grid_edge) = (outlined.cells.len(), outlined.touches_edge);
    if let Some(why) = holds_dense(&mut result, record, dense, geometry, &outlined) {
        return point(result, why);
    }
    result.catalogues = record
        .test
        .catalogues
        .iter()
        .map(|source| {
            let entries = catalogues
                .iter()
                .find_map(|(id, entries)| (id == source).then_some(*entries));
            compare(sky, &outlined, source, entries, &record.test)
        })
        .collect();
    if let Some(why) = refusal(&result.catalogues, &record.test) {
        return point(result, why);
    }
    let geometry = Geometry::GridRegion {
        grid: grid.source.clone(),
        origin: geometry.corner(),
        cell: geometry.cell,
        shape: Axis::XYZ.map(|a| index_u32(geometry.cells_along(a))),
        cells: outlined.cells.iter().map(|c| c.map(index_u32)).collect(),
    };
    geometry
        .check()
        .map_err(|e| OutlineError::Field(e.to_string()))?;
    result.placed = Placed::Outline {
        frame: grid.geometry.frame,
        unit: grid.geometry.unit,
        kind: KindOfValue::Reconstructed,
        geometry,
    };
    Ok(result)
}

impl OutlineResult {
    /// The record of `structure` outlined by `record`, before any test: shown at `published`.
    fn untested(structure: &str, record: &GridOutline, published: &Coordinates) -> Self {
        Self {
            structure: structure.to_owned(),
            grid: record.grid.clone(),
            field: record.field.clone(),
            threshold: record.threshold.clone(),
            side: record.side,
            test: record.test.clone(),
            coverage: COVERAGE,
            seed: None,
            cells: 0,
            touches_grid_edge: false,
            catalogues: vec![],
            dense: None,
            placed: Placed::Point {
                position: Box::new(published.clone()),
                reason: String::new(),
            },
        }
    }
}

/// Records the check of the dense places the record names, and gives why the region may not be
/// a void when it holds one.
fn holds_dense(
    result: &mut OutlineResult,
    record: &GridOutline,
    dense: &[DensePlace],
    geometry: &GridGeometry,
    outlined: &Region,
) -> Option<String> {
    let rule = record.test.dense_places.as_ref()?;
    let found = dense::check(rule, dense, geometry, outlined);
    let why = (!found.held.is_empty()).then(|| {
        format!(
            "the region holds {} of the dense places a void may not: {}",
            found.held.len(),
            found.held.join(", ")
        )
    });
    result.dense = Some(found);
    why
}

/// The seed on the grid, or why it cannot be placed there.
fn seed_cell(
    grid: &Grid,
    values: &[f64],
    name: String,
    position: &Coordinates,
) -> Result<SeedCell, String> {
    let geometry = &grid.geometry;
    let at = on_grid(position, geometry)
        .map_err(|why| format!("{name} cannot be placed on the grid: {why}"))?;
    let Some((cell, index)) = geometry
        .cell_at(at)
        .and_then(|c| Some((c, geometry.sample_index(c)?)))
    else {
        return Err(format!("{name} lies outside the grid {}", grid.source));
    };
    Ok(SeedCell {
        name,
        at,
        cell,
        value: values[index],
    })
}

/// Why the catalogue checks refuse the outline under `test`, or `None` when they admit it.
fn refusal(checks: &[CatalogueCheck], test: &OutlineTest) -> Option<String> {
    if !checks.iter().any(|c| c.coverage == Coverage::Covers) {
        return Some("no catalogue of the test covers the outline".to_owned());
    }
    let failed: Vec<String> = checks
        .iter()
        .filter(|c| c.passes == Some(false))
        .map(|c| {
            format!(
                "{} holds {} galaxies against {:.1} expected, a ratio of {:.2}",
                c.source,
                c.held,
                c.expected,
                c.ratio.unwrap_or_default()
            )
        })
        .collect();
    if failed.is_empty() {
        return None;
    }
    let rule = serde_norway::to_string(&test.rule).unwrap_or_default();
    Some(format!(
        "the outline fails its test ({}, ratio {}): {}",
        rule.trim(),
        test.ratio,
        failed.join("; ")
    ))
}

/// A cell index or count, which a grid of real memory holds far below 2^32.
fn index_u32(i: usize) -> u32 {
    u32::try_from(i).unwrap_or(u32::MAX)
}
