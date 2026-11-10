//! The 3D dust map of Edenhofer et al. 2024 (A&A 685, A82): `mean_and_std_healpix.fits` of
//! Zenodo record 10.5281/zenodo.8187943 (v1.0, CC BY 4.0), resampled by [`resample`] onto a
//! Cartesian grid about the Sun.
//!
//! Layout, as the file's headers give it (read on 2026-11-10) and `sources/edenhofer-2024-dust.yaml`
//! records it:
//! - a primary HDU without data;
//! - IMAGE extensions `MEAN` and `STD.`, the posterior mean and standard deviation of the dust
//!   density, 32-bit floats with `NAXIS1` = 786,432 `HEALPix` pixels of `NSIDE` 256 in the NEST
//!   ordering (fastest) and `NAXIS2` = 516 distance bins, in "E of Zhang, Green, and Rix (2023)"
//!   per parsec;
//! - binary tables `RADIAL PIXEL CENTERS` (516 rows) and `RADIAL PIXEL BOUNDARIES` (517 rows), in
//!   pc, logarithmically spaced from 68.62 to 1,248.10 pc.
//!
//! The pixels are directions in galactic coordinates about the Sun, as the provider's readme
//! names its Cartesian interpolation "Galactic XYZ". Nothing is read whole: the layout is read
//! from the headers, and [`resample`] streams the images one distance bin at a time.

mod resample;

use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::Path;

use univ_model::grid::Axis;
use univ_model::position::Frame;
use univ_model::units::Unit;

use super::{GridReadError, Published, open};
use crate::fits::{BLOCK, Header, SampleKind, fill};

pub use resample::read_edenhofer;

/// The source definition this grid belongs to.
pub const SOURCE: &str = "edenhofer-2024-dust";
/// The fetched file the grid is read from.
pub const FILE: &str = "mean_and_std_healpix.fits";
/// Cells along X, Y and Z of the whole grid.
pub const CELLS: [usize; 3] = [250; 3];
/// The distances from the Sun, in pc, within which the map holds data (Edenhofer et al. 2024:
/// 69 pc to 1.25 kpc); a cell whose centre lies outside them holds no data.
pub const NEAREST_PC: f64 = 69.0;
pub const FARTHEST_PC: f64 = 1_250.0;

/// The published grid: a cube of 2,500 pc a side centred on the Sun, in 250 cells of 10 pc,
/// galactic Cartesian, the first cell centred half a cell in from −1,250 pc.
const PUBLISHED: Published = Published {
    frame: Frame::Galactic,
    unit: Unit::Parsec,
    axes: Axis::XYZ,
    cells: CELLS,
    cell: 10.0,
    origin: [-1_245.0; 3],
    assumed_cosmology: None,
};

/// The unit the images name in `CUNIT`.
const DUST_UNIT: &str = "E of Zhang, Green, and Rix (2023)";

/// One image of the map: where its samples begin in the file, and its scaling.
#[derive(Clone, Copy, Debug)]
struct Image {
    offset: u64,
    /// `BSCALE` and `BZERO`.
    scaling: (f64, f64),
}

/// The map's layout, read from its headers.
#[derive(Debug)]
struct Map {
    /// The `HEALPix` order, log2 of `NSIDE`.
    order: u8,
    pixels: usize,
    mean: Image,
    std: Image,
    /// The centre of each distance bin, in pc.
    centres: Vec<f64>,
    /// The `centres.len() + 1` boundaries of the bins, in pc, increasing.
    boundaries: Vec<f64>,
}

/// A reason the file does not have the layout the reader takes.
fn layout(reason: impl Into<String>) -> GridReadError {
    GridReadError::Layout {
        file: FILE.to_owned(),
        reason: reason.into(),
    }
}

/// The bytes a unit's data take with their padding to a whole block.
fn padded(bytes: usize) -> Result<i64, GridReadError> {
    i64::try_from(bytes.div_ceil(BLOCK) * BLOCK).map_err(|_| layout("an HDU too large to skip"))
}

/// Reads the map's layout: every header, the two images' places and the distance bins.
fn read_map(path: &Path) -> Result<Map, GridReadError> {
    let mut reader = open(path)?;
    let (mut mean, mut std, mut centres, mut boundaries) = (None, None, None, None);
    while let Some(header) = Header::read_from(&mut reader)? {
        let bytes = header.data_bytes()?;
        let name = header
            .get("EXTNAME")
            .unwrap_or_default()
            .trim_end()
            .to_owned();
        match name.as_str() {
            "MEAN" | "STD." => {
                let at = reader.stream_position()?;
                let image = image(&header, at).map_err(|e| layout(format!("{name}: {e}")))?;
                let slot = if name == "MEAN" { &mut mean } else { &mut std };
                *slot = Some(image);
                reader.seek_relative(padded(bytes)?)?;
            }
            "RADIAL PIXEL CENTERS" | "RADIAL PIXEL BOUNDARIES" => {
                let column =
                    column(&mut reader, &header).map_err(|e| layout(format!("{name}: {e}")))?;
                let rest = padded(bytes)? - i64::try_from(bytes).unwrap_or(i64::MAX);
                reader.seek_relative(rest)?;
                let slot = if name == "RADIAL PIXEL CENTERS" {
                    &mut centres
                } else {
                    &mut boundaries
                };
                *slot = Some(column);
            }
            _ => reader.seek_relative(padded(bytes)?)?,
        }
    }
    let missing = |what: &str| layout(format!("no {what} extension"));
    let ((mean, shape), (std, std_shape)) = (
        mean.ok_or_else(|| missing("MEAN"))?,
        std.ok_or_else(|| missing("STD."))?,
    );
    let centres = centres.ok_or_else(|| missing("RADIAL PIXEL CENTERS"))?;
    let boundaries = boundaries.ok_or_else(|| missing("RADIAL PIXEL BOUNDARIES"))?;
    if shape != std_shape {
        return Err(layout(format!("STD. is {std_shape:?}, MEAN {shape:?}")));
    }
    let (order, pixels, bins) = shape;
    if centres.len() != bins || boundaries.len() != bins + 1 {
        return Err(layout(format!(
            "{bins} distance bins, but {} centres and {} boundaries",
            centres.len(),
            boundaries.len()
        )));
    }
    let ordered = boundaries.windows(2).all(|b| b[0] < b[1])
        && centres
            .iter()
            .zip(boundaries.windows(2))
            .all(|(c, b)| b[0] <= *c && *c <= b[1])
        && boundaries.first().is_some_and(|b| *b > 0.0);
    if !ordered {
        return Err(layout(
            "the distance bins do not increase, each centre within its bounds",
        ));
    }
    Ok(Map {
        order,
        pixels,
        mean,
        std,
        centres,
        boundaries,
    })
}

/// An image's place and its (order, pixels, bins), checked against the layout the reader takes.
fn image(header: &Header, offset: u64) -> Result<(Image, (u8, usize, usize)), GridReadError> {
    let text = |key: &str| header.get(key).unwrap_or_default().trim_end().to_owned();
    if header.sample_kind()? != SampleKind::F32 {
        return Err(layout("BITPIX is not -32"));
    }
    if text("ORDERING") != "NEST" {
        return Err(layout(format!(
            "ORDERING is {:?}, not NEST",
            text("ORDERING")
        )));
    }
    if text("CUNIT") != DUST_UNIT {
        return Err(layout(format!(
            "CUNIT is {:?}, not {DUST_UNIT:?}",
            text("CUNIT")
        )));
    }
    let nside = header.integer("NSIDE")?.and_then(|n| u64::try_from(n).ok());
    let order = nside
        .filter(|n| n.is_power_of_two() && *n <= 1 << 13)
        .map(u64::trailing_zeros)
        .and_then(|o| u8::try_from(o).ok())
        .ok_or_else(|| layout(format!("NSIDE {nside:?} is not a power of two up to 2^13")))?;
    let [pixels, bins] = header.axes()?[..] else {
        return Err(layout("not a two-dimensional image"));
    };
    if pixels != 12 << (2 * order) {
        return Err(layout(format!(
            "{pixels} pixels, not 12 NSIDE² for NSIDE 2^{order}"
        )));
    }
    Ok((
        Image {
            offset,
            scaling: header.scaling()?,
        },
        (order, pixels, bins),
    ))
}

/// The one column of 32-bit floats of a binary table, read at the reader's place.
fn column(reader: &mut BufReader<File>, header: &Header) -> Result<Vec<f64>, GridReadError> {
    let form = header.get("TFORM1").unwrap_or_default().trim();
    let [width, rows] = header.axes()?[..] else {
        return Err(layout("not a table of rows"));
    };
    if header.count("TFIELDS", None)? != 1 || !matches!(form, "E" | "1E") || width != 4 {
        return Err(layout(format!(
            "not one column of 32-bit floats (TFORM1 {form:?})"
        )));
    }
    let mut bytes = vec![0_u8; rows * 4];
    if fill(reader, &mut bytes)? < bytes.len() {
        return Err(layout("the table ends early"));
    }
    Ok(floats(&bytes, (1.0, 0.0)).collect())
}

/// Big-endian 32-bit floats, scaled as `BZERO + BSCALE × stored`.
fn floats(bytes: &[u8], (scale, zero): (f64, f64)) -> impl Iterator<Item = f64> + '_ {
    bytes.as_chunks::<4>().0.iter().map(move |&b| {
        let stored = f32::from_be_bytes(b);
        zero + scale * f64::from(stored)
    })
}

/// Reads `buf.len()` bytes or fails.
fn read_exact(reader: &mut impl Read, buf: &mut [u8]) -> Result<(), GridReadError> {
    if fill(reader, buf)? < buf.len() {
        return Err(layout("an image ends before its last distance bin"));
    }
    Ok(())
}
