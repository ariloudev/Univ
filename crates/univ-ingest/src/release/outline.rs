//! The test of each structure outlined from a volume grid (RQ-23), run while the build holds the
//! grid ([`crate::outlines::place_on`]), against the entries of the test's catalogues and the
//! dense places the release holds, and recorded in `structure_details.outline`.
//!
//! An outline its test admits is the structure's placed shape: the grid region, kept in the
//! grid's frame (supergalactic for the Cosmicflows grids), its origin and cell edge turned from
//! h⁻¹ Mpc into Mpc of the release cosmology by the factor 100 / H0 of
//! `univ_model::derive::Release::hubble_rescaling`, as the grid's header rescales its cell. Its
//! box is its cells' corners moved to the ICRS, and the structure's centre is that box's centre.
//! A structure whose outline its test refused, whose outline could not be drawn, or whose grid is
//! not among the release's grids keeps the geometry its file gives, its published position.

use std::collections::HashMap;

use univ_model::api::structures::{
    DistanceDerivation, GridPlacement, OutlineRecord, OutlineVerdict,
};
use univ_model::cosmology::AssumedCosmology;
use univ_model::derive::Release;
use univ_model::entry::CatalogueEntry;
use univ_model::geometry::{Aabb, Geometry, Shape};
use univ_model::grid::Grid;
use univ_model::ids::SourceId;
use univ_model::position::Frame;
use univ_model::units::{Dimension, Unit};

use super::objects::snake;
use super::shapes::{PlacedShape, derivation, frame_chain, in_domain};
use crate::crossmatch::ReleaseObject;
use crate::outlines::{DensePlace, GridSky, OutlineResult, Placed, dense_places, place_on};
use crate::structures::{DensePlaces, GridOutline, Structure};

/// What the build made of a structure outlined on a grid: its `outline` column and, when its
/// test admitted the outline, the outline placed.
#[derive(Clone, Debug, PartialEq)]
pub struct Outlined {
    pub record: OutlineRecord,
    pub admitted: Option<Admitted>,
}

/// An admitted outline placed: the structure's shape and derivation, and its box in the ICRS
/// about the barycentre in Mpc, which bounds and centres the structure.
#[derive(Clone, Debug, PartialEq)]
pub struct Admitted {
    pub placed: PlacedShape,
    pub bounds: Aabb,
}

impl Admitted {
    /// The centre of the box, in Mpc.
    #[must_use]
    pub fn centre(&self) -> [f64; 3] {
        std::array::from_fn(|k| f64::midpoint(self.bounds.min[k], self.bounds.max[k]))
    }
}

/// The record of an outline on `record`'s grid, before its test's numbers: refused for
/// `reason` unless `reason` is none.
fn outline_record(record: &GridOutline, reason: Option<String>) -> Result<OutlineRecord, String> {
    let value = |v: Result<serde_json::Value, serde_json::Error>| {
        v.map_err(|e| format!("the outline record cannot be written as JSON: {e}"))
    };
    Ok(OutlineRecord {
        grid: record.grid.to_string(),
        field: record.field.clone(),
        threshold: value(serde_json::to_value(&record.threshold))?,
        test: value(serde_json::to_value(&record.test))?,
        verdict: if reason.is_some() {
            OutlineVerdict::Refused
        } else {
            OutlineVerdict::Admitted
        },
        reason,
        cells: 0,
        touches_edge: false,
        checks: Vec::new(),
        dense: None,
    })
}

/// The `outline` column of a structure whose outline was drawn, its record `record`: the grid,
/// field, threshold and test as recorded, the verdict and its reason, the region's size and the
/// checks.
///
/// # Errors
///
/// Fails when a part cannot be written as JSON.
pub fn result_record(record: &GridOutline, r: &OutlineResult) -> Result<OutlineRecord, String> {
    let reason = match &r.placed {
        Placed::Outline { .. } => None,
        Placed::Point { reason, .. } => Some(reason.clone()),
    };
    let json = |e: serde_json::Error| format!("the outline's checks cannot be written: {e}");
    Ok(OutlineRecord {
        cells: r.cells as u64,
        touches_edge: r.touches_grid_edge,
        checks: r
            .catalogues
            .iter()
            .map(serde_json::to_value)
            .collect::<Result<_, _>>()
            .map_err(json)?,
        dense: r
            .dense
            .as_ref()
            .map(serde_json::to_value)
            .transpose()
            .map_err(json)?,
        ..outline_record(record, reason)?
    })
}

/// The `outline` column of a structure whose outline could not be drawn, with why.
///
/// # Errors
///
/// Fails when the record cannot be written as JSON.
pub fn not_drawn(record: &GridOutline, reason: &str) -> Result<OutlineRecord, String> {
    outline_record(
        record,
        Some(format!(
            "the outline was not drawn: {reason}; the structure is shown at its published position"
        )),
    )
}

/// Why the outline of a structure on `grid` is not drawn when the release holds no such grid.
#[must_use]
pub fn grid_missing(record: &GridOutline) -> String {
    format!(
        "its volume grid {} is not among this release's grids",
        record.grid
    )
}

/// The factor that turns a grid length in `unit` into Mpc of `release`'s cosmology: 100 / H0
/// for h⁻¹ Mpc, a conversion for any other length.
fn to_mpc(unit: Unit, release: &Release) -> Result<f64, String> {
    match unit.dimension() {
        Dimension::HubbleScaledLength => release
            .hubble_rescaling(AssumedCosmology::HUBBLE_SCALED)
            .map(|r| r.factor)
            .map_err(|e| e.to_string()),
        _ => unit.factor_to(Unit::Megaparsec).map_err(|e| e.to_string()),
    }
}

/// The admitted outline of `s`, a grid region in `frame` and `unit`, placed under `release`.
///
/// # Errors
///
/// Fails when the grid's unit is not a length or the region cannot stand as a shape.
pub fn admitted(
    s: &Structure,
    frame: Frame,
    unit: Unit,
    region: &Geometry,
    release: &Release,
) -> Result<Admitted, String> {
    let Geometry::GridRegion {
        grid,
        origin,
        cell,
        shape,
        cells,
    } = region
    else {
        return Err("an admitted outline is a grid region".to_owned());
    };
    let factor = to_mpc(unit, release)?;
    let in_mpc = Geometry::GridRegion {
        grid: grid.clone(),
        origin: origin.map(|c| c * factor),
        cell: cell * factor,
        shape: *shape,
        cells: cells.clone(),
    };
    let bounds = Shape::new(frame, Unit::Megaparsec, in_mpc.clone())
        .map_err(|e| format!("its outline cannot stand: {e}"))?
        .bounds_in(Frame::Icrs)
        .ok_or_else(|| format!("its outline's frame {frame:?} has no box in the ICRS"))?;
    let shape = in_domain(frame, in_mpc, s.scale_domain)?;
    let mut derived = derivation(
        vec!["/outline".to_owned()],
        frame_chain(Some(frame)),
        DistanceDerivation {
            from: format!(
                "the {} cells of the volume grid {grid} its outline test admitted",
                cells.len()
            ),
            method: format!(
                "each cell's corners at the grid's corner plus its indices times the cell edge, \
                 the corner and the edge in {} multiplied by {factor} into Mpc of the release \
                 cosmology",
                snake(&unit)
            ),
        },
    );
    derived.grid = Some(GridPlacement {
        grid: grid.clone(),
        frame,
        native_unit: unit,
        unit: shape.unit,
        factor,
        centre: "the centre of the box of the region's cell corners moved to the ICRS".to_owned(),
    });
    Ok(Admitted {
        placed: PlacedShape {
            shape,
            derivation: derived,
        },
        bounds,
    })
}

/// What became of the outline of `s` on `record`'s grid, its test's result `result`.
fn outlined(
    s: &Structure,
    record: &GridOutline,
    result: Result<OutlineResult, String>,
    release: &Release,
) -> Result<Outlined, String> {
    let r = match result {
        Ok(r) => r,
        Err(e) => {
            return Ok(Outlined {
                record: not_drawn(record, &e)?,
                admitted: None,
            });
        }
    };
    let admitted = match &r.placed {
        Placed::Outline {
            frame,
            unit,
            geometry,
            ..
        } => match admitted(s, *frame, *unit, geometry, release) {
            Ok(a) => Some(a),
            Err(e) => {
                let why = format!("its admitted outline cannot be placed: {e}");
                return Ok(Outlined {
                    record: not_drawn(record, &why)?,
                    admitted: None,
                });
            }
        },
        Placed::Point { .. } => None,
    };
    Ok(Outlined {
        record: result_record(record, &r)?,
        admitted,
    })
}

/// Outlines each of `structures` recorded on `grid` and tests it against the entries of
/// `objects`, giving each structure's id and what became of its outline, an admitted one placed
/// under `release`.
///
/// # Errors
///
/// Fails when a record cannot be written as JSON.
pub fn outlines_on(
    grid: &Grid,
    structures: &[(String, Structure)],
    objects: &[ReleaseObject],
    release: &Release,
) -> Result<Vec<(String, Outlined)>, String> {
    let outlined_here: Vec<(&String, &Structure, &GridOutline)> = structures
        .iter()
        .filter_map(|(id, s)| {
            let record = s.outline.as_ref()?;
            (record.grid.as_str() == grid.source).then_some((id, s, record))
        })
        .collect();
    if outlined_here.is_empty() {
        return Ok(Vec::new());
    }
    let sky = GridSky::new(&grid.geometry);
    let wanted: Vec<&SourceId> = outlined_here
        .iter()
        .flat_map(|(_, _, r)| &r.test.catalogues)
        .collect();
    let mut entries: HashMap<&SourceId, Vec<&CatalogueEntry>> = HashMap::new();
    for e in objects.iter().flat_map(|o| &o.object.entries) {
        let source = &e.entry.designation.source;
        if wanted.contains(&source) {
            entries.entry(source).or_default().push(&e.entry);
        }
    }
    // The dense places of each distinct rule, picked once for the grid.
    let mut dense: Vec<(&DensePlaces, Vec<DensePlace>)> = Vec::new();
    for rule in outlined_here
        .iter()
        .filter_map(|(_, _, r)| r.test.dense_places.as_ref())
    {
        if !dense.iter().any(|(r, _)| *r == rule) {
            let places = dense_places(
                rule,
                objects.iter().map(|o| &o.object),
                structures.iter().map(|(_, s)| s),
            );
            dense.push((rule, places));
        }
    }
    outlined_here
        .into_iter()
        .map(|(id, s, record)| {
            // A catalogue the release holds no entry of is not given to the test.
            let catalogues: Vec<(SourceId, &[&CatalogueEntry])> = record
                .test
                .catalogues
                .iter()
                .filter_map(|c| Some((c.clone(), entries.get(c)?.as_slice())))
                .collect();
            let places = record
                .test
                .dense_places
                .as_ref()
                .and_then(|rule| dense.iter().find(|(r, _)| *r == rule))
                .map_or(&[][..], |(_, p)| p.as_slice());
            let result = place_on(s, grid, &sky, &catalogues, places).map_err(|e| e.to_string());
            Ok((id.clone(), outlined(s, record, result, release)?))
        })
        .collect()
}
