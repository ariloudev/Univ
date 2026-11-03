//! Readers of the reconstructed volume grids: the file formats ([`npy`], [`fits`], members of a
//! ZIP or NPZ archive) and, per source, the typed [`univ_model::grid::Grid`] each file becomes,
//! with its frame, axis order, spacing, origin, units and kind of value made explicit.
//!
//! The sources are defined in `sources/cf4pp-density.yaml`, `sources/cf4-basins.yaml` and
//! `sources/twompp-reconstruction.yaml`; each reader's documentation cites what fixes its layout.

pub mod basins;
pub mod cf4pp;
pub mod fits;
pub mod npy;
pub mod twompp;

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use thiserror::Error;
use univ_model::cosmology::AssumedCosmology;
use univ_model::grid::{Axis, GridError, GridGeometry};

use crate::fits::{FitsError, fill};
use univ_model::position::{Epoch, Frame, Vec3};
use univ_model::units::Unit;

pub use basins::read_cf4_basins;
pub use cf4pp::{Cf4pp, read_cf4pp};
pub use twompp::read_twompp;

/// A grid file that cannot be read.
#[derive(Debug, Error)]
pub enum GridReadError {
    #[error("reading the grid failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("the archive cannot be read: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("not a NumPy array this reader takes: {0}")]
    Npy(String),
    #[error(transparent)]
    Fits(#[from] FitsError),
    #[error("the data end after {read} of the {declared} values the header declares")]
    Truncated { declared: usize, read: usize },
    #[error("{file}: {reason}")]
    Layout { file: String, reason: String },
    #[error(transparent)]
    Grid(#[from] GridError),
}

/// Which part of a published grid a file holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Window {
    /// The whole grid, as its source publishes it: the file must have the published shape.
    Whole,
    /// A cut-out whose first cell is the published grid's cell `start`, indices along X, Y and Z
    /// of the frame: the test fixtures, cropped from the published files.
    Cutout { start: [usize; 3] },
}

/// How a source lays out its whole grid.
struct Published {
    frame: Frame,
    unit: Unit,
    /// The frame axis of each array dimension, slowest first.
    axes: [Axis; 3],
    /// Cells along X, Y and Z of the whole grid.
    cells: [usize; 3],
    cell: f64,
    /// The centre of the whole grid's first cell, X, Y, Z.
    origin: Vec3,
    assumed_cosmology: Option<AssumedCosmology>,
}

impl Published {
    /// The geometry of a file of `shape` (array order) that holds `window` of this grid.
    fn geometry(
        &self,
        file: &str,
        shape: &[usize],
        window: Window,
    ) -> Result<GridGeometry, GridReadError> {
        let layout = |reason: String| GridReadError::Layout {
            file: file.to_string(),
            reason,
        };
        let shape: [usize; 3] = shape
            .try_into()
            .map_err(|_| layout(format!("shape {shape:?} is not three-dimensional")))?;
        let start = match window {
            Window::Whole => [0; 3],
            Window::Cutout { start } => start,
        };
        for (axis, n) in self.axes.iter().zip(shape) {
            let k = axis.index();
            let fits = match window {
                Window::Whole => n == self.cells[k],
                Window::Cutout { .. } => start[k] + n <= self.cells[k],
            };
            if !fits {
                return Err(layout(format!(
                    "{n} cells along {axis:?} from cell {} do not fit the published {} cells",
                    start[k], self.cells[k]
                )));
            }
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "cell indices are far below 2^52"
        )]
        let origin = std::array::from_fn(|k| self.origin[k] + start[k] as f64 * self.cell);
        let geometry = GridGeometry {
            frame: self.frame,
            epoch: Epoch::J2000,
            unit: self.unit,
            axes: self.axes,
            shape,
            cell: self.cell,
            origin,
            assumed_cosmology: self.assumed_cosmology,
        };
        geometry.check()?;
        Ok(geometry)
    }
}

/// Reads `count` numbers of `width` bytes each (1 to 8), decoding each with `decode`, in chunks.
/// `count` comes from a file's header, so no more is reserved than one chunk holds: the values
/// grow as the data arrive, and data shorter than `count` are an error.
fn read_numbers(
    reader: &mut impl Read,
    count: usize,
    width: usize,
    decode: impl Fn(&[u8]) -> Result<f64, GridReadError>,
) -> Result<Vec<f64>, GridReadError> {
    const CHUNK: usize = 1 << 20;
    let per_chunk = CHUNK.checked_div(width).unwrap_or(0);
    let mut values = Vec::with_capacity(count.min(per_chunk));
    let mut buf = vec![0_u8; per_chunk * width];
    while values.len() < count && per_chunk > 0 {
        let n = (count - values.len()).min(per_chunk);
        let bytes = &mut buf[..n * width];
        let filled = fill(reader, bytes)?;
        for element in bytes[..filled].chunks_exact(width) {
            values.push(decode(element)?);
        }
        if filled < bytes.len() {
            break;
        }
    }
    if values.len() < count {
        return Err(GridReadError::Truncated {
            declared: count,
            read: values.len(),
        });
    }
    Ok(values)
}

/// Reads one byte more: none should follow the data a header declares, and reaching the end
/// is what makes a ZIP member check its CRC-32.
fn at_end(reader: &mut (impl Read + ?Sized)) -> std::io::Result<bool> {
    Ok(fill(reader, &mut [0_u8; 1])? == 0)
}

/// A ZIP archive (an NPZ is one), open on its file.
type Archive = zip::ZipArchive<BufReader<File>>;

/// Opens a ZIP archive once, for all the members read from it.
fn archive(path: &Path) -> Result<Archive, GridReadError> {
    Ok(zip::ZipArchive::new(open(path)?)?)
}

/// Runs `read` on one member of an archive, streamed, then reads the member to its end, so
/// that bytes after the data or a member whose CRC-32 does not match are refused. A reading
/// error names the member.
fn member<T>(
    zip: &mut Archive,
    name: &str,
    read: impl FnOnce(&mut dyn Read) -> Result<T, GridReadError>,
) -> Result<T, GridReadError> {
    let in_member = |reason: String| GridReadError::Layout {
        file: name.to_string(),
        reason,
    };
    let mut entry = zip.by_name(name)?;
    let value = read(&mut entry).map_err(|e| match e {
        GridReadError::Io(e) => in_member(e.to_string()),
        e => e,
    })?;
    match at_end(&mut entry) {
        Ok(true) => Ok(value),
        Ok(false) => Err(in_member(
            "bytes follow the data its header declares".into(),
        )),
        Err(e) => Err(in_member(e.to_string())),
    }
}

/// Opens a plain file for buffered reading.
fn open(path: &Path) -> Result<BufReader<File>, GridReadError> {
    Ok(BufReader::new(File::open(path)?))
}

/// A file's name for messages.
fn name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}
