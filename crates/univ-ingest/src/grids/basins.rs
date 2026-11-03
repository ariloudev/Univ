//! The Cosmicflows-4 basins of attraction and repulsion (Dupuy & Courtois 2023, A&A 678, A176):
//! four FITS label grids in `CF4_new_128-z008_watersheds-fits.zip`.
//!
//! Layout: "Both reconstructions are computed in a grid of size 128^3 and side length of 1,000
//! Mpc h^-1" (Dupuy & Courtois 2023, §2.1), in supergalactic Cartesian coordinates (Lahav et al.
//! 2000), for H0 = 74.6 km s⁻¹ Mpc⁻¹ (§2.1). The FITS axes carry no names; NAXIS1 is SGX, so in C
//! order the samples are `[i_SGZ][i_SGY][i_SGX]` (`sources/cf4-basins.yaml`, checked at the nine
//! attractors of Table 1). Every attractor and repeller position in Table 1 is a whole multiple
//! of 1000/128 h⁻¹ Mpc, rounded, so the authors place voxel i at `(i − 64) × 7.8125` h⁻¹ Mpc, the
//! observer at the centre of voxel 64; that is the origin taken here (inferred from Table 1; the
//! CF4++ grid of the same group is offset from it by half a cell). Label 0 is no basin; label k
//! is the k-th basin of its section of Table 1, blank lines skipped.

use std::path::Path;

use univ_model::cosmology::AssumedCosmology;
use univ_model::grid::{Axis, CellUncertainty, Components, Field, Grid, LabelName, Samples};
use univ_model::position::Frame;
use univ_model::units::Unit;
use univ_model::value::KindOfValue;

use super::fits::FitsImage;
use super::{GridReadError, Published, Window, archive, member};

/// The source definition these grids belong to.
pub const SOURCE: &str = "cf4-basins";

/// The archive's four members: basins of attraction (`BoA`) and repulsion (`BoR`) of the CF4
/// galaxies (`CF4_new`) and groups (`CF4gp_new`) reconstructions.
pub const MEMBERS: [&str; 4] = [
    "CF4_new_128-z008_BoA.fits",
    "CF4_new_128-z008_BoR.fits",
    "CF4gp_new_128-z008_BoA.fits",
    "CF4gp_new_128-z008_BoR.fits",
];

/// The basins of attraction of the CF4 galaxies map in the order of Dupuy & Courtois 2023
/// Table 1, blank lines skipped, as `sources/cf4-basins.yaml` observed them. The other three
/// maps follow the same rule but were not checked, so their labels are left unnamed.
const CF4_GALAXY_ATTRACTORS: [&str; 9] = [
    "Laniakea",
    "Apus",
    "Hercules",
    "Lepus",
    "Perseus-Pisces",
    "Shapley",
    "SDSS-1a",
    "SDSS-2a",
    "SDSS-2b",
];

/// The published grid: 128³ voxels of 1000/128 h⁻¹ Mpc, voxel 0 at −500, NAXIS1 = SGX fastest.
const PUBLISHED: Published = Published {
    frame: Frame::Supergalactic,
    unit: Unit::MegaparsecOverH,
    axes: [Axis::Z, Axis::Y, Axis::X],
    cells: [128; 3],
    cell: 1000.0 / 128.0,
    origin: [-500.0; 3],
    assumed_cosmology: Some(AssumedCosmology::hubble(74.6)),
};

/// Reads the four label grids of the archive into one grid with a field per member. The labels
/// are stored as floats (BITPIX = −64) and become integers; a value that is not a whole,
/// non-negative number is refused.
///
/// # Errors
///
/// Fails when the archive cannot be read, misses a member, or a member is not a three-axis image
/// of the grid's shape (or `window` of it) holding whole labels.
pub fn read_cf4_basins(path: &Path, window: Window) -> Result<Grid, GridReadError> {
    let mut zip = archive(path)?;
    let mut geometry = None;
    let mut fields = Vec::new();
    for name in MEMBERS {
        let layout = |reason: String| GridReadError::Layout {
            file: name.to_string(),
            reason,
        };
        let image = member(&mut zip, name, |r| FitsImage::read_primary(r))?;
        if image.bitpix != -64 || image.axes.len() != 3 {
            return Err(layout(format!(
                "BITPIX {} with {} axes, not a cube of float64 labels",
                image.bitpix,
                image.axes.len()
            )));
        }
        // NAXIS1 runs fastest, so the C-order shape is the FITS axes reversed.
        let shape: Vec<usize> = image.axes.iter().rev().copied().collect();
        let this = PUBLISHED.geometry(name, &shape, window)?;
        if geometry.as_ref().is_some_and(|g| *g != this) {
            return Err(layout(format!("its grid differs from {}'s", MEMBERS[0])));
        }
        geometry = Some(this);
        let labels = image
            .values
            .iter()
            .map(|&x| whole(x).ok_or_else(|| layout(format!("{x} is not a whole label"))))
            .collect::<Result<Vec<_>, _>>()?;
        let names = if name == MEMBERS[0] {
            (1..)
                .zip(CF4_GALAXY_ATTRACTORS)
                .map(|(label, name)| LabelName {
                    label,
                    name: name.to_string(),
                })
                .collect()
        } else {
            vec![]
        };
        fields.push(Field {
            column: name.to_string(),
            field: "basin_label".to_string(),
            unit: Unit::Dimensionless,
            kind: KindOfValue::Reconstructed,
            components: Components::Scalar,
            samples: Samples::Label(labels),
            uncertainty: CellUncertainty::NoneReported,
            labels: names,
        });
    }
    let geometry = geometry.ok_or_else(|| GridReadError::Layout {
        file: MEMBERS[0].to_string(),
        reason: "no members".into(),
    })?;
    Ok(Grid::new(SOURCE.to_string(), geometry, fields)?)
}

/// A whole number in [0, 2^32) as a label.
fn whole(x: f64) -> Option<u32> {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "checked to be a whole number in [0, 2^32)"
    )]
    let label = x as u32;
    (x.fract() == 0.0 && (0.0..4_294_967_296.0).contains(&x)).then_some(label)
}
