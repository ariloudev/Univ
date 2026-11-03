//! The 2M++ reconstruction (Carrick et al. 2015, MNRAS 450, 317): the luminosity-weighted galaxy
//! density contrast and the predicted peculiar velocities, `twompp_density.npy` and
//! `twompp_velocity.npy`.
//!
//! Layout, from <https://cosmicflows.iap.fr/download/>: "257^3 cubes", "Galactic Cartesian comoving
//! coordinates X,Y,Z in Mpc/h, with k running fastest", "X = (i-128)*400./256." (likewise Y with
//! j, Z with k), "the origin (Local Group) is in the centre of cell [128,128,128]", velocities in
//! the CMB frame in km/s. The README's "i running fastest" is wrong for the `.npy` files
//! (`sources/twompp-reconstruction.yaml`): their headers give C order, so the density is
//! `[i_X][i_Y][i_Z]` and the velocity `[c][i_X][i_Y][i_Z]` with c = X, Y, Z. Galactic axes follow
//! the Hipparcos definition (ESA 1997): X toward the centre, Y toward l = 90°, Z toward the north
//! pole. The distances are in units of cz/100, so no Hubble constant is assumed. The provider
//! publishes no uncertainty per cell, so none is reported.

use std::path::Path;

use univ_model::grid::{Axis, CellUncertainty, Components, Field, Grid, Samples};
use univ_model::position::Frame;
use univ_model::units::Unit;
use univ_model::value::KindOfValue;

use super::npy::NpyArray;
use super::{GridReadError, Published, Window, name, open};

/// The source definition this grid belongs to.
pub const SOURCE: &str = "twompp-reconstruction";

/// The published grid: 257³ cells of 400/256 = 1.5625 h⁻¹ Mpc, centres from −200 to +200.
const PUBLISHED: Published = Published {
    frame: Frame::Galactic,
    unit: Unit::MegaparsecOverH,
    axes: Axis::XYZ,
    cells: [257; 3],
    cell: 400.0 / 256.0,
    origin: [-200.0; 3],
    assumed_cosmology: None,
};

/// Reads the density and velocity files into one grid, both reconstructed, with no uncertainty
/// reported.
///
/// # Errors
///
/// Fails when a file cannot be read or does not hold the grid (or `window` of it): a cube for the
/// density, three cubes for the velocity.
pub fn read_twompp(density: &Path, velocity: &Path, window: Window) -> Result<Grid, GridReadError> {
    let d = NpyArray::read(open(density)?)?;
    let geometry = PUBLISHED.geometry(&name(density), &d.shape, window)?;
    let v = NpyArray::read(open(velocity)?)?;
    let same = v.shape.split_first() == Some((&3, &d.shape[..]));
    if !same {
        return Err(GridReadError::Layout {
            file: name(velocity),
            reason: format!("shape {:?} is not 3 × the density's {:?}", v.shape, d.shape),
        });
    }
    let field = |column: &str, field: &str, unit, components, values| Field {
        column: column.to_string(),
        field: field.to_string(),
        unit,
        kind: KindOfValue::Reconstructed,
        components,
        samples: Samples::Real(values),
        uncertainty: CellUncertainty::NoneReported,
        labels: vec![],
    };
    let fields = vec![
        field(
            "twompp_density.npy",
            "density",
            Unit::Dimensionless,
            Components::Scalar,
            d.values,
        ),
        field(
            "twompp_velocity.npy",
            "velocity_vector",
            Unit::KilometrePerSecond,
            Components::Vector { axes: Axis::XYZ },
            v.values,
        ),
    ];
    Ok(Grid::new(SOURCE.to_string(), geometry, fields)?)
}
