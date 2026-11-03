//! A release's volume grids (fetch-a-volume-grid): each grid source's files read into its
//! univ-model grid, and written as a JSON header and one file of little-endian 32-bit floats
//! per field and per field's uncertainty, at the keys of `univ_model::release::keys`.
//!
//! The samples keep the grid's own layout: row-major over the header's `axes`, the last
//! fastest, and for a vector field the component slowest. A label grid's labels are written as
//! floats, exact up to 2^24, beyond which the build refuses them.

use std::path::{Path, PathBuf};

use serde_json::json;
use univ_model::cosmology::AssumedCosmology;
use univ_model::derive::Release;
use univ_model::grid::{CellUncertainty, Grid, Samples};
use univ_model::release::keys;
use univ_model::release::rows::GridRow;
use univ_model::units::Unit;
use univ_model::value::Quantity;

use super::objects::{json, snake};
use crate::grids::{Window, read_cf4_basins, read_cf4pp, read_twompp};
use crate::source::SourceDefinition;

/// The version of the grid header's layout.
pub const HEADER_VERSION: u32 = 1;
/// The largest label written exactly as a 32-bit float.
const MAX_LABEL: u32 = 1 << 24;

/// A grid's files in the release, by path, and its D1 row.
#[derive(Debug)]
pub struct GridFiles {
    pub files: Vec<(String, Vec<u8>)>,
    pub row: GridRow,
}

/// Whether `definition` is a volume grid this build reads.
#[must_use]
pub fn is_grid(definition: &SourceDefinition) -> bool {
    matches!(
        definition.id.as_str(),
        "cf4pp-density" | "cf4-basins" | "twompp-reconstruction"
    )
}

/// The file among `files` whose name is `name`.
fn named<'a>(files: &'a [PathBuf], name: &str) -> Result<&'a Path, String> {
    files
        .iter()
        .find(|f| f.file_name().is_some_and(|n| n == name))
        .map(PathBuf::as_path)
        .ok_or_else(|| format!("the fetch gave no file {name}"))
}

/// Reads the whole grid of `definition` from its fetched `files`.
///
/// # Errors
///
/// Fails, naming the source, when it is not a grid this build reads or its files cannot be read.
pub fn read(definition: &SourceDefinition, files: &[PathBuf]) -> Result<Grid, String> {
    let id = definition.id.as_str();
    let grid = match id {
        "cf4pp-density" => {
            read_cf4pp(named(files, "new_CF4pp_mean_std_grids.npz")?, Window::Whole).map(|g| g.grid)
        }
        "cf4-basins" => read_cf4_basins(
            named(files, "CF4_new_128-z008_watersheds-fits.zip")?,
            Window::Whole,
        ),
        "twompp-reconstruction" => read_twompp(
            named(files, "twompp_density.npy")?,
            named(files, "twompp_velocity.npy")?,
            Window::Whole,
        ),
        _ => return Err(format!("{id}: no volume grid reader")),
    };
    grid.map_err(|e| format!("{id}: {e}"))
}

/// Little-endian `f32` samples.
fn floats(values: impl Iterator<Item = f64>) -> Vec<u8> {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the samples are published as 32-bit floats; the format is f32"
    )]
    values.flat_map(|v| (v as f32).to_le_bytes()).collect()
}

/// The files and row of `grid`, from `definition`, in release `release` under `cosmology`.
///
/// # Errors
///
/// Fails when a label lies beyond 2^24, or the cell cannot be rescaled.
pub fn grid_files(
    release: u32,
    grid: &Grid,
    definition: &SourceDefinition,
    cosmology: &Release,
) -> Result<GridFiles, String> {
    let id = definition.id.as_str();
    let g = &grid.geometry;
    let mut files = Vec::new();
    let mut fields = Vec::new();
    for (i, field) in grid.fields.iter().enumerate() {
        let path = keys::grid_samples(id, &format!("field-{i}"));
        let bytes = match &field.samples {
            Samples::Real(v) => floats(v.iter().copied()),
            Samples::Label(v) => {
                if let Some(big) = v.iter().find(|&&l| l > MAX_LABEL) {
                    return Err(format!(
                        "{id}: label {big} is beyond 2^24, not exact in f32"
                    ));
                }
                floats(v.iter().map(|&l| f64::from(l)))
            }
        };
        files.push((path.clone(), bytes));
        let uncertainty = match &field.uncertainty {
            CellUncertainty::NoneReported => json!({ "form": "none_reported" }),
            CellUncertainty::StandardDeviation { column, sigma } => {
                let sigma_path = keys::grid_samples(id, &format!("field-{i}-sigma"));
                files.push((sigma_path.clone(), floats(sigma.iter().copied())));
                json!({
                    "form": "standard_deviation",
                    "column": column,
                    "samples": keys::key(release, &sigma_path),
                })
            }
        };
        fields.push(json!({
            "column": field.column,
            "field": field.field,
            "unit": field.unit,
            "kind": field.kind,
            "components": field.components,
            "samples": keys::key(release, &path),
            "uncertainty": uncertainty,
            "labels": field.labels,
        }));
    }
    // The cell in h^-1 Mpc, rescaled by the release's Hubble constant, as the spec asks.
    let cell_rescaled = if g.unit == Unit::MegaparsecOverH {
        let cell = Quantity::new(g.cell, g.unit, univ_model::value::Uncertainty::NoneReported)
            .map_err(|e| e.to_string())?;
        let rescaled = cosmology
            .hubble_rescaling(AssumedCosmology::HUBBLE_SCALED)
            .and_then(|r| r.apply(&cell))
            .map_err(|e| format!("{id}: the cell cannot be rescaled: {e}"))?;
        Some(rescaled)
    } else {
        None
    };
    let header = json!({
        "format": "univ-volume-grid",
        "header_version": HEADER_VERSION,
        "id": id,
        "release": release,
        "source": { "id": id, "title": definition.title, "citation": definition.citation,
                    "credit": definition.terms.credit },
        "frame": g.frame,
        "epoch": g.epoch,
        "unit": g.unit,
        "axes": g.axes,
        "shape": g.shape,
        "origin": g.origin,
        "origin_is": "the centre of the cell whose indices are all 0, in frame order (x, y, z)",
        "cell": g.cell,
        "cell_rescaled": cell_rescaled,
        "release_cosmology": cosmology.cosmology(),
        "assumed_cosmology": g.assumed_cosmology,
        "samples": "little-endian 32-bit floats, row-major over `axes` (the last fastest); a \
                    vector field's component is the slowest index",
        "fields": fields,
    });
    let header_path = keys::grid_header(id);
    let header_bytes = serde_json::to_vec_pretty(&header).map_err(|e| e.to_string())?;
    files.push((header_path.clone(), header_bytes));
    let quantities: Vec<_> = grid
        .fields
        .iter()
        .map(|f| json!({ "field": f.field, "unit": f.unit, "kind": f.kind }))
        .collect();
    let row = GridRow {
        release,
        id: id.to_owned(),
        source: id.to_owned(),
        unit: snake(&g.unit),
        shape: json(&g.shape)?,
        fields: json(&quantities)?,
        header_key: keys::key(release, &header_path),
    };
    Ok(GridFiles { files, row })
}
