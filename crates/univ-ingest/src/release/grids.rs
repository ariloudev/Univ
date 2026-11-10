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
use univ_model::derive::{Release, Rescaled};
use univ_model::grid::{CellUncertainty, Grid, Samples};
use univ_model::release::keys;
use univ_model::release::rows::GridRow;
use univ_model::units::Unit;
use univ_model::value::Quantity;

use super::objects::{json, snake};
use crate::grids::{
    Window, basins, cf4pp, edenhofer, read_cf4_basins, read_cf4pp, read_edenhofer, read_twompp,
    twompp,
};
use crate::source::SourceDefinition;
use crate::tiles::sha256;

/// The version of the grid header's layout.
pub const HEADER_VERSION: u32 = 1;
/// The largest label written exactly as a 32-bit float.
const MAX_LABEL: u32 = 1 << 24;

/// A grid's files in the release, by path, and its D1 row, whose `source` the build numbers.
#[derive(Debug)]
pub struct GridFiles {
    pub files: Vec<(String, Vec<u8>)>,
    pub row: GridRow,
}

/// How a grid source's whole grid is read from the files its fetch gives.
type Reader = fn(&[PathBuf]) -> Result<Grid, String>;

/// Each volume grid this build reads, by its source's id.
const READERS: [(&str, Reader); 4] = [
    (cf4pp::SOURCE, |files| {
        let path = named(files, "new_CF4pp_mean_std_grids.npz")?;
        read_cf4pp(path, Window::Whole)
            .map(|g| g.grid)
            .map_err(|e| e.to_string())
    }),
    (basins::SOURCE, |files| {
        let path = named(files, "CF4_new_128-z008_watersheds-fits.zip")?;
        read_cf4_basins(path, Window::Whole).map_err(|e| e.to_string())
    }),
    (twompp::SOURCE, |files| {
        let (density, velocity) = (
            named(files, "twompp_density.npy")?,
            named(files, "twompp_velocity.npy")?,
        );
        read_twompp(density, velocity, Window::Whole).map_err(|e| e.to_string())
    }),
    (edenhofer::SOURCE, |files| {
        let path = named(files, edenhofer::FILE)?;
        read_edenhofer(path, Window::Whole, edenhofer::CELLS).map_err(|e| e.to_string())
    }),
];

/// The reader of `definition`'s grid, none for a source that is no volume grid.
fn reader(definition: &SourceDefinition) -> Option<Reader> {
    READERS
        .iter()
        .find(|(id, _)| *id == definition.id.as_str())
        .map(|&(_, read)| read)
}

/// Whether `definition` is a volume grid this build reads.
#[must_use]
pub fn is_grid(definition: &SourceDefinition) -> bool {
    reader(definition).is_some()
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
    let read = reader(definition).ok_or_else(|| format!("{id}: no volume grid reader"))?;
    read(files).map_err(|e| format!("{id}: {e}"))
}

/// Little-endian `f32` samples.
fn floats(values: impl Iterator<Item = f64>) -> Vec<u8> {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the samples are published as 32-bit floats; the format is f32"
    )]
    values.flat_map(|v| (v as f32).to_le_bytes()).collect()
}

/// The cell of `grid` in h^-1 Mpc, rescaled by the release's Hubble constant, as the spec asks;
/// none for a grid in another unit.
fn cell_rescaled(grid: &Grid, cosmology: &Release) -> Result<Option<Rescaled>, String> {
    let g = &grid.geometry;
    if g.unit != Unit::MegaparsecOverH {
        return Ok(None);
    }
    let cell = Quantity::new(g.cell, g.unit, univ_model::value::Uncertainty::NoneReported)
        .map_err(|e| e.to_string())?;
    cosmology
        .hubble_rescaling(AssumedCosmology::HUBBLE_SCALED)
        .and_then(|r| r.apply(&cell))
        .map(Some)
        .map_err(|e| format!("{}: the cell cannot be rescaled: {e}", grid.source))
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
        let samples_key = keys::file(&sha256(&bytes));
        files.push((path, bytes));
        let uncertainty = match &field.uncertainty {
            CellUncertainty::NoneReported => json!({ "form": "none_reported" }),
            CellUncertainty::StandardDeviation { column, sigma } => {
                let sigma_path = keys::grid_samples(id, &format!("field-{i}-sigma"));
                let sigma_bytes = floats(sigma.iter().copied());
                let sigma_key = keys::file(&sha256(&sigma_bytes));
                files.push((sigma_path, sigma_bytes));
                json!({
                    "form": "standard_deviation",
                    "column": column,
                    "samples": sigma_key,
                })
            }
        };
        fields.push(json!({
            "column": field.column,
            "field": field.field,
            "unit": field.unit,
            "kind": field.kind,
            "components": field.components,
            "samples": samples_key,
            "uncertainty": uncertainty,
            "labels": field.labels,
        }));
    }
    let cell_rescaled = cell_rescaled(grid, cosmology)?;
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
        "resampling": grid.resampling,
        "no_data": grid.no_data,
    });
    let header_path = keys::grid_header(id);
    let header_bytes = serde_json::to_vec_pretty(&header).map_err(|e| e.to_string())?;
    let header_key = keys::file(&sha256(&header_bytes));
    files.push((header_path, header_bytes));
    let quantities: Vec<_> = grid
        .fields
        .iter()
        .map(|f| json!({ "field": f.field, "unit": f.unit, "kind": f.kind }))
        .collect();
    let row = GridRow {
        release,
        id: id.to_owned(),
        source: 0,
        unit: snake(&g.unit),
        shape: json(&g.shape)?,
        fields: json(&quantities)?,
        header_key,
    };
    Ok(GridFiles { files, row })
}
