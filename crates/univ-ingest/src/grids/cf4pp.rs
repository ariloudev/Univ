//! The CF4++ reconstruction (Courtois et al. 2025, A&A 701, A187): the mean and standard
//! deviation over its Hamiltonian Monte Carlo realizations of the density contrast, the radial
//! velocity and the velocity vector, in `new_CF4pp_mean_std_grids.npz`.
//!
//! Layout, as `sources/cf4pp-density.yaml` records it and the provider's
//! `retrieve_CF4pp_grid_values.py` reads it (`d_data[ix,iy,iz]`, `vxyz_data[:,ix,iy,iz]`, the
//! index of a coordinate `floor((x + 500) / (1000/128))`): 128³ cells of 1000/128 = 7.8125 h⁻¹ Mpc
//! in supergalactic Cartesian coordinates (Lahav et al. 2000), the observer at the centre, the
//! arrays indexed `[i_SGX][i_SGY][i_SGZ]` in C order and the vectors `[c][i_SGX][i_SGY][i_SGZ]`
//! with c = SGX, SGY, SGZ. Cell i is centred on `(i + 0.5) × 7.8125 − 500` h⁻¹ Mpc. No Hubble
//! constant is stated with the file, so none is recorded.

use std::path::Path;

use univ_model::grid::{Axis, CellUncertainty, Components, Field, Grid, Samples};
use univ_model::position::Frame;
use univ_model::units::Unit;
use univ_model::value::KindOfValue;

use super::npy::NpyArray;
use super::{GridReadError, Published, Window, archive, member};

/// The source definition this grid belongs to.
pub const SOURCE: &str = "cf4pp-density";

/// The published grid: 128³ cells of 1000/128 h⁻¹ Mpc, the first centred half a cell in from −500.
const PUBLISHED: Published = Published {
    frame: Frame::Supergalactic,
    unit: Unit::MegaparsecOverH,
    axes: Axis::XYZ,
    cells: [128; 3],
    cell: 1000.0 / 128.0,
    origin: [-500.0 + 500.0 / 128.0; 3],
    assumed_hubble_constant: None,
};

/// Each field: its mean member, its standard-deviation member, its field in the source
/// definition, its unit, and whether it is a vector.
const FIELDS: [(&str, &str, &str, Unit, bool); 3] = [
    ("d_mean", "d_std", "density", Unit::Dimensionless, false),
    (
        "vr_mean",
        "vr_std",
        "radial_velocity",
        Unit::KilometrePerSecond,
        false,
    ),
    (
        "v_mean",
        "v_std",
        "velocity_vector",
        Unit::KilometrePerSecond,
        true,
    ),
];

/// The CF4++ grid and the number of realizations its means are taken over.
#[derive(Clone, Debug, PartialEq)]
pub struct Cf4pp {
    pub grid: Grid,
    /// The archive's member `N`: read as the number of HMC realizations (inferred; the paper and
    /// the provider's page say nearly 10,000).
    pub realizations: u64,
}

/// Reads the CF4++ archive: the density contrast, radial velocity and velocity vector with their
/// standard deviations as one-sigma uncertainties, all of them reconstructed.
///
/// # Errors
///
/// Fails when the archive cannot be read, misses a member, or holds arrays of another shape than
/// the grid's (or `window` of it).
pub fn read_cf4pp(path: &Path, window: Window) -> Result<Cf4pp, GridReadError> {
    let mut zip = archive(path)?;
    let mut array = |name: &str| member(&mut zip, &format!("{name}.npy"), |r| NpyArray::read(r));
    let layout = |file: &str, reason: String| GridReadError::Layout {
        file: file.to_string(),
        reason,
    };
    let mut geometry = None;
    let mut fields = Vec::new();
    for (mean, std, field, unit, vector) in FIELDS {
        let (m, s) = (array(mean)?, array(std)?);
        if m.shape != s.shape {
            return Err(layout(
                std,
                format!("shape {:?} differs from {mean}'s {:?}", s.shape, m.shape),
            ));
        }
        let cube = match (vector, m.shape.split_first()) {
            (true, Some((3, cube))) => cube,
            (true, _) => return Err(layout(mean, format!("shape {:?} is not (3, …)", m.shape))),
            (false, _) => &m.shape[..],
        };
        let this = PUBLISHED.geometry(mean, cube, window)?;
        if geometry.as_ref().is_some_and(|g| *g != this) {
            return Err(layout(mean, "its grid differs from d_mean's".into()));
        }
        geometry = Some(this);
        let components = if vector {
            Components::Vector { axes: Axis::XYZ }
        } else {
            Components::Scalar
        };
        fields.push(Field {
            column: mean.to_string(),
            field: field.to_string(),
            unit,
            kind: KindOfValue::Reconstructed,
            components,
            samples: Samples::Real(m.values),
            uncertainty: CellUncertainty::StandardDeviation {
                column: std.to_string(),
                sigma: s.values,
            },
            labels: vec![],
        });
    }
    let n = array("N")?;
    let realizations = n
        .scalar()
        .filter(|x| x.fract() == 0.0 && *x >= 1.0)
        .ok_or_else(|| layout("N", format!("{:?} is not a count", n.values)))?;
    let geometry = geometry.ok_or_else(|| layout("d_mean", "no fields".into()))?;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a whole number ≥ 1 below 2^53"
    )]
    let realizations = realizations as u64;
    Ok(Cf4pp {
        grid: Grid::new(SOURCE.to_string(), geometry, fields)?,
        realizations,
    })
}
