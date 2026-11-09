//! A released volume grid's samples read at points, as its header lays them out.

use std::path::Path;

use serde::Deserialize;
use sha2::{Digest, Sha256};
use univ_model::encoding::hex;
use univ_model::release::keys;

/// What a check reads of a grid header (`grids/{id}/header.json`).
#[derive(Debug, Deserialize)]
pub struct Header {
    /// The axes in the order the samples are laid out, the last fastest: `x`, `y` or `z`.
    pub axes: Vec<String>,
    /// The number of cells along each axis, in the order of `axes`.
    pub shape: Vec<usize>,
    /// The centre of the cell whose indices are all 0, in frame order (x, y, z).
    pub origin: [f64; 3],
    pub cell: f64,
    pub unit: String,
    pub frame: String,
    pub fields: Vec<Field>,
}

/// A field of a grid.
#[derive(Debug, Deserialize)]
pub struct Field {
    /// Its samples' R2 key.
    pub samples: String,
    #[serde(default)]
    pub labels: Vec<Label>,
}

/// A label a field names.
#[derive(Debug, Deserialize)]
pub struct Label {
    pub label: i64,
    pub name: String,
}

/// A grid field loaded: its header and samples.
pub struct Loaded {
    pub header: Header,
    pub field: usize,
    pub samples: Vec<f32>,
}

/// Reads field `field` of grid `grid` of release `number` in `dir`.
///
/// # Errors
///
/// Fails on a header or samples file that cannot be read, or samples that do not fill the
/// shape.
pub fn load(dir: &Path, number: u32, grid: &str, field: usize) -> Result<Loaded, String> {
    let path = dir.join(keys::grid_header(grid));
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let header: Header =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let f = header
        .fields
        .get(field)
        .ok_or_else(|| format!("grid {grid} has no field {field}"))?;
    // A field's samples lie at its path in the release directory and, in R2, under the key of
    // what they hold, which the header names.
    let path = dir.join(keys::grid_samples(grid, &format!("field-{field}")));
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let key = keys::file(&hex(&Sha256::digest(&bytes)));
    if key != f.samples {
        return Err(format!(
            "grid {grid} of release {number}: the header names {} for field {field}, but its \
             samples are {key}",
            f.samples
        ));
    }
    let samples: Vec<f32> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect();
    let cells: usize = header.shape.iter().product();
    if header.axes.len() != 3 || header.shape.len() != 3 || samples.len() != cells {
        return Err(format!(
            "grid {grid}: {} samples for the shape {:?} over axes {:?}",
            samples.len(),
            header.shape,
            header.axes
        ));
    }
    Ok(Loaded {
        header,
        field,
        samples,
    })
}

impl Loaded {
    /// The sample of the cell holding `at`, in the grid's frame and unit (x, y, z), or `None`
    /// outside the grid.
    #[must_use]
    pub fn at(&self, at: [f64; 3]) -> Option<f32> {
        let mut flat = 0usize;
        for (axis, &n) in self.header.axes.iter().zip(&self.header.shape) {
            let k = match axis.as_str() {
                "x" => 0,
                "y" => 1,
                "z" => 2,
                _ => return None,
            };
            let index = ((at[k] - self.header.origin[k]) / self.header.cell).round();
            if !(0.0..f64::from(u32::try_from(n).ok()?)).contains(&index) {
                return None;
            }
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "an index checked to lie within the shape"
            )]
            let index = index as usize;
            flat = flat * n + index;
        }
        self.samples.get(flat).copied()
    }

    /// The name the field gives `label`, if it names it.
    #[must_use]
    pub fn label_name(&self, label: i64) -> Option<&str> {
        self.header.fields[self.field]
            .labels
            .iter()
            .find(|l| l.label == label)
            .map(|l| l.name.as_str())
    }
}
