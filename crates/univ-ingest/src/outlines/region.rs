//! The region of an outline: the cells of one grid field on the stated side of the threshold,
//! connected across faces to the cell that holds the seed; and a published position placed on
//! the grid.

use univ_model::cosmology::SPEED_OF_LIGHT;
use univ_model::grid::{Axis, Components, Grid, GridGeometry, Samples};
use univ_model::position::{Vec3, between, unit_vector};
use univ_model::units::{Dimension, Unit};

use crate::structures::{Cited, Coordinates, Radial, Side};

/// The Hubble constant of velocity space, 100 h km s⁻¹ Mpc⁻¹: a recession velocity v stands for
/// the distance v / 100 h⁻¹ Mpc, with no cosmology.
const HUNDRED_H: f64 = 100.0;

/// The distance in velocity space of a redshift `z`: cz / 100 h⁻¹ Mpc.
#[must_use]
pub fn velocity_distance(z: f64) -> f64 {
    z * SPEED_OF_LIGHT / HUNDRED_H
}

/// The cells of a region, by their indices along X, Y and Z of the grid's frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Region {
    pub cells: Vec<[usize; 3]>,
    /// Whether a cell of the region lies on a face of the grid, so the grid may cut it.
    pub touches_edge: bool,
}

/// The samples of the scalar real field filling `field` in the source definition.
///
/// # Errors
///
/// Fails, saying why, when the grid has no such field or it is not one real number per cell.
pub fn scalar<'a>(grid: &'a Grid, field: &str) -> Result<&'a [f64], String> {
    let found = grid
        .fields
        .iter()
        .find(|f| f.field == field)
        .ok_or_else(|| format!("the grid {} has no field {field}", grid.source))?;
    match (&found.samples, found.components) {
        (Samples::Real(values), Components::Scalar) => Ok(values),
        _ => Err(format!(
            "the field {field} of {} is not one real number per cell",
            grid.source
        )),
    }
}

/// Whether `value` lies on `side` of `threshold`; a value on the threshold lies on neither.
#[must_use]
pub fn on_side(value: f64, threshold: f64, side: Side) -> bool {
    match side {
        Side::Below => value < threshold,
        Side::Above => value > threshold,
    }
}

/// The cells connected across faces to `start` whose value lies on `side` of `threshold`,
/// `start` first; empty when `start` itself does not.
#[must_use]
pub fn region(
    geometry: &GridGeometry,
    values: &[f64],
    start: [usize; 3],
    threshold: f64,
    side: Side,
) -> Region {
    let value = |cell: [usize; 3]| geometry.sample_index(cell).map(|i| values[i]);
    let mut seen = vec![false; geometry.cells()];
    let mut cells = Vec::new();
    let mut touches_edge = false;
    let take = |cell: [usize; 3], seen: &mut [bool]| match geometry.sample_index(cell) {
        Some(i) if !seen[i] && value(cell).is_some_and(|v| on_side(v, threshold, side)) => {
            seen[i] = true;
            true
        }
        _ => false,
    };
    if take(start, &mut seen) {
        cells.push(start);
    }
    let mut next = 0;
    while let Some(&cell) = cells.get(next) {
        next += 1;
        for axis in Axis::XYZ {
            let k = axis.index();
            let last = geometry.cells_along(axis) - 1;
            touches_edge |= cell[k] == 0 || cell[k] == last;
            for step in [cell[k].checked_sub(1), Some(cell[k] + 1)] {
                let Some(i) = step.filter(|&i| i <= last) else {
                    continue;
                };
                let mut neighbour = cell;
                neighbour[k] = i;
                if take(neighbour, &mut seen) {
                    cells.push(neighbour);
                }
            }
        }
    }
    Region {
        cells,
        touches_edge,
    }
}

/// A published position on a grid laid out in velocity space (h⁻¹ Mpc), in the grid's frame
/// and unit: velocities and redshifts become distances cz / 100 h⁻¹ Mpc, and the frame is
/// rotated into the grid's.
///
/// # Errors
///
/// Fails, saying why, for a grid in another unit, a frame univ-model does not rotate, a
/// distance in a unit that needs a Hubble constant, or a direction with no distance.
pub fn on_grid(position: &Coordinates, geometry: &GridGeometry) -> Result<Vec3, String> {
    if geometry.unit != Unit::MegaparsecOverH {
        return Err(format!(
            "the grid is laid out in {:?}, not h^-1 Mpc of velocity space, and placing a \
             published position on it needs a cosmology",
            geometry.unit
        ));
    }
    let (frame, v) = match position {
        Coordinates::Observer => return Ok([0.0; 3]),
        Coordinates::Cartesian { frame, x, y, z } => (
            frame,
            [velocity_space(x)?, velocity_space(y)?, velocity_space(z)?],
        ),
        Coordinates::Spherical {
            frame,
            lon,
            lat,
            radial,
        } => {
            let degrees = |c: &Cited| {
                c.quantity
                    .to(Unit::Degree)
                    .map(|q| q.value)
                    .map_err(|e| e.to_string())
            };
            let lon = lon.as_ref().map_or(Ok(0.0), degrees)?;
            let r = match radial {
                Some(Radial::Redshift(z)) => velocity_distance(z.value()),
                Some(Radial::Velocity(c) | Radial::Distance(c)) => velocity_space(c)?,
                None => return Err("the published position gives no distance".into()),
            };
            (frame, unit_vector(lon, degrees(lat)?).map(|u| u * r))
        }
    };
    if let Some(error) = frame.systematic_uncertainty() {
        return Err(format!(
            "the published position is in {frame:?}, uncertain by {error:.2}°: too coarse to check \
             an outline against"
        ));
    }
    Ok(between(*frame, geometry.frame).apply(v))
}

/// A published coordinate as a distance in velocity space, h⁻¹ Mpc.
fn velocity_space(c: &Cited) -> Result<f64, String> {
    let to = |unit| {
        c.quantity
            .to(unit)
            .map(|q| q.value)
            .map_err(|e| e.to_string())
    };
    match c.unit().dimension() {
        Dimension::Velocity => Ok(to(Unit::KilometrePerSecond)? / HUNDRED_H),
        Dimension::HubbleScaledLength => to(Unit::MegaparsecOverH),
        _ => Err(format!(
            "a distance in {:?} needs a Hubble constant to be placed in velocity space",
            c.unit()
        )),
    }
}
