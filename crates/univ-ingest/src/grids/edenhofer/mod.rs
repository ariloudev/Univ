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

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use univ_model::grid::Axis;
use univ_model::position::Frame;
use univ_model::units::Unit;

use super::{GridReadError, Published, open, read_numbers};
use crate::fits::{BLOCK, Header, Sample, SampleKind, sample};

pub use resample::read_edenhofer;

/// The source definition this grid belongs to.
pub const SOURCE: &str = "edenhofer-2024-dust";
/// The fetched file the grid is read from.
pub const FILE: &str = "mean_and_std_healpix.fits";
/// Cells along X, Y and Z of the whole grid.
pub const CELLS: [usize; 3] = [250; 3];
/// The distances the paper gives the map, 69 pc to 1.25 kpc: the file's outermost distance
/// boundaries must round to them, within 1%.
const PAPER_REACH_PC: [f64; 2] = [69.0, 1_250.0];

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

/// One image of the map: where its samples begin in the file, its scaling and its shape.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Image {
    offset: u64,
    /// `BSCALE` and `BZERO`.
    scaling: (f64, f64),
    /// The `HEALPix` order, log2 of `NSIDE`.
    order: u8,
    bins: usize,
}

impl Image {
    fn pixels(self) -> usize {
        12 << (2 * self.order)
    }

    /// The values of `bins` from the reader's place, one per pixel and bin, as stored scaled.
    /// A value that is not finite is refused: NaN in the grid marks only the cells outside the
    /// map's distance bins.
    fn values(self, reader: &mut impl Read, bins: usize) -> Result<Vec<f64>, GridReadError> {
        let (scale, zero) = self.scaling;
        read_numbers(reader, self.pixels() * bins, 4, |b| {
            let value = zero + scale * single(b)?;
            if value.is_finite() {
                Ok(value)
            } else {
                Err(layout(format!("a value that is not finite, {value}")))
            }
        })
    }
}

/// The map's layout, read from its headers.
#[derive(Debug)]
struct Map {
    mean: Image,
    std: Image,
    /// The centre of each distance bin, in pc.
    centres: Vec<f64>,
    /// The `centres.len() + 1` boundaries of the bins, in pc, increasing.
    boundaries: Vec<f64>,
}

impl Map {
    /// The innermost and outermost distance boundaries, in pc: where the map holds data.
    fn reach(&self) -> (f64, f64) {
        (
            self.boundaries[0],
            self.boundaries[self.boundaries.len() - 1],
        )
    }
}

/// A reason the file does not have the layout the reader takes.
fn layout(reason: impl Into<String>) -> GridReadError {
    GridReadError::Layout {
        file: FILE.to_owned(),
        reason: reason.into(),
    }
}

/// Reads the map's layout: every header, the two images' places and the distance bins.
fn read_map(path: &Path) -> Result<Map, GridReadError> {
    let mut reader = open(path)?;
    let (mut mean, mut std, mut centres, mut boundaries) = (None, None, None, None);
    while let Some(header) = Header::read_from(&mut reader)? {
        let start = reader.stream_position()?;
        let name = header.get("EXTNAME").unwrap_or_default().trim_end();
        let named = |e: GridReadError| layout(format!("{name}: {e}"));
        match name {
            "MEAN" => mean = Some(image(&header, start).map_err(named)?),
            "STD." => std = Some(image(&header, start).map_err(named)?),
            "RADIAL PIXEL CENTERS" => centres = Some(column(&mut reader, &header).map_err(named)?),
            "RADIAL PIXEL BOUNDARIES" => {
                boundaries = Some(column(&mut reader, &header).map_err(named)?);
            }
            _ => {}
        }
        let bytes = u64::try_from(header.data_bytes()?.div_ceil(BLOCK) * BLOCK)
            .map_err(|_| layout("an HDU too large to skip"))?;
        reader.seek(SeekFrom::Start(start + bytes))?;
    }
    let missing = |what: &str| layout(format!("no {what} extension"));
    let mean = mean.ok_or_else(|| missing("MEAN"))?;
    let std = std.ok_or_else(|| missing("STD."))?;
    let centres = centres.ok_or_else(|| missing("RADIAL PIXEL CENTERS"))?;
    let boundaries = boundaries.ok_or_else(|| missing("RADIAL PIXEL BOUNDARIES"))?;
    if (mean.order, mean.bins) != (std.order, std.bins) {
        return Err(layout("STD. differs in shape from MEAN"));
    }
    if centres.len() != mean.bins || boundaries.len() != mean.bins + 1 {
        return Err(layout(format!(
            "{} distance bins, but {} centres and {} boundaries",
            mean.bins,
            centres.len(),
            boundaries.len()
        )));
    }
    let ordered = boundaries.windows(2).all(|b| b[0] < b[1])
        && centres
            .iter()
            .zip(boundaries.windows(2))
            .all(|(c, b)| b[0] <= *c && *c <= b[1]);
    if !ordered {
        return Err(layout(
            "the distance bins do not increase, each centre within its bounds",
        ));
    }
    let map = Map {
        mean,
        std,
        centres,
        boundaries,
    };
    let (near, far) = map.reach();
    let [paper_near, paper_far] = PAPER_REACH_PC;
    if (near / paper_near - 1.0).abs() > 0.01 || (far / paper_far - 1.0).abs() > 0.01 {
        return Err(layout(format!(
            "the bins reach {near} to {far} pc, not the paper's {paper_near} to {paper_far}"
        )));
    }
    Ok(map)
}

/// An image's place and shape, checked against the layout the reader takes.
fn image(header: &Header, offset: u64) -> Result<Image, GridReadError> {
    plain(header, "IMAGE")?;
    if header.sample_kind()? != SampleKind::F32 {
        return Err(layout("BITPIX is not -32"));
    }
    for (key, want) in [("ORDERING", "NEST"), ("CUNIT", DUST_UNIT)] {
        let got = header.get(key).unwrap_or_default().trim_end();
        if got != want {
            return Err(layout(format!("{key} is {got:?}, not {want}")));
        }
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
    let image = Image {
        offset,
        scaling: header.scaling()?,
        order,
        bins,
    };
    if pixels != image.pixels() {
        return Err(layout(format!(
            "{pixels} pixels, not 12 NSIDE² for NSIDE 2^{order}"
        )));
    }
    Ok(image)
}

/// The one column of 32-bit floats of a binary table, read at the reader's place.
fn column(reader: &mut impl Read, header: &Header) -> Result<Vec<f64>, GridReadError> {
    plain(header, "BINTABLE")?;
    let form = header.get("TFORM1").unwrap_or_default().trim();
    let [width, rows] = header.axes()?[..] else {
        return Err(layout("not a table of rows"));
    };
    if header.count("TFIELDS", None)? != 1 || !matches!(form, "E" | "1E") || width != 4 {
        return Err(layout(format!(
            "not one column of 32-bit floats (TFORM1 {form:?})"
        )));
    }
    read_numbers(reader, rows, 4, single)
}

/// One big-endian 32-bit float.
fn single(bytes: &[u8]) -> Result<f64, GridReadError> {
    sample(SampleKind::F32, bytes)
        .map(Sample::to_f64)
        .ok_or_else(|| layout("a sample of the wrong width"))
}

/// Refuses an extension that is not a `kind` (IMAGE or BINTABLE) of one group without a heap,
/// whose data are then exactly its axes' product.
fn plain(header: &Header, kind: &str) -> Result<(), GridReadError> {
    let xtension = header.get("XTENSION").unwrap_or_default().trim_end();
    if xtension != kind {
        return Err(layout(format!("XTENSION is {xtension:?}, not {kind}")));
    }
    if header.count("PCOUNT", Some(0))? != 0 || header.count("GCOUNT", Some(1))? != 1 {
        return Err(layout("PCOUNT or GCOUNT other than 0 and 1"));
    }
    Ok(())
}
