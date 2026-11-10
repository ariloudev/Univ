//! The dust map resampled onto the published grid (resample-the-dust-map): each cell holds the
//! mean of the source's values at the centres of the source's voxels that fall within it, or,
//! where none does, the source's value at the cell's centre; a cell whose centre lies outside
//! the distances the map covers holds NaN, which marks no data.
//!
//! Two passes over the voxels keep the reading sequential: the first, from the geometry alone,
//! counts the voxel centres in each cell and finds the voxel at the centre of each cell that has
//! none; the second streams the two images one distance bin at a time and sums.

use std::collections::BTreeMap;
use std::io::{Seek, SeekFrom};
use std::path::Path;

use univ_model::grid::{Axis, CellUncertainty, Components, Field, Grid, GridGeometry, Samples};
use univ_model::healpix;
use univ_model::position::Vec3;
use univ_model::units::Unit;
use univ_model::value::KindOfValue;

use super::{
    FARTHEST_PC, FILE, Image, Map, NEAREST_PC, PUBLISHED, SOURCE, floats, read_exact, read_map,
};
use crate::grids::{GridReadError, Window, open};

/// Reads the dust map at `path` and resamples it onto `window` of the published grid, `shape`
/// cells along X, Y and Z: [`super::CELLS`] for the whole grid. The posterior mean is the field,
/// its posterior standard deviation the field's uncertainty, both reconstructed.
///
/// # Errors
///
/// Fails when the file does not have the map's layout, ends early, or `shape` does not fit the
/// published grid from `window`.
pub fn read_edenhofer(
    path: &Path,
    window: Window,
    shape: [usize; 3],
) -> Result<Grid, GridReadError> {
    let map = read_map(path)?;
    let geometry = PUBLISHED.geometry(FILE, &shape, window)?;
    let directions: Vec<Vec3> = (0..map.pixels as u64)
        .map(|p| healpix::centre(map.order, p))
        .collect();
    let (near, far) = reach(&geometry);
    let shells: Vec<usize> = (0..map.centres.len())
        .filter(|&b| (near..=far).contains(&map.centres[b]))
        .collect();
    let (grid, dirs) = (&geometry, &directions);
    let voxels = |b: usize| {
        let d = map.centres[b];
        dirs.iter()
            .enumerate()
            .filter_map(move |(p, u)| Some((p, index(grid, u.map(|c| c * d))?)))
    };
    let mut counts = vec![0_u32; geometry.cells()];
    for &b in &shells {
        for (_, i) in voxels(b) {
            counts[i] += 1;
        }
    }
    let (empty, at_centre) = classify(&geometry, &map, &counts);
    let mut mean = vec![0.0; counts.len()];
    let mut sigma = vec![0.0; counts.len()];
    let needed = shells.iter().chain(at_centre.keys()).copied();
    if let (Some(first), Some(last)) = (needed.clone().min(), needed.max()) {
        let row = map.pixels * 4;
        let mut mean_rows = Rows::open(path, map.mean, first, row)?;
        let mut std_rows = Rows::open(path, map.std, first, row)?;
        let (mut m, mut s) = (Vec::new(), Vec::new());
        for b in first..=last {
            m.clear();
            m.extend(mean_rows.next()?);
            s.clear();
            s.extend(std_rows.next()?);
            if shells.binary_search(&b).is_ok() {
                for (p, i) in voxels(b) {
                    mean[i] += m[p];
                    sigma[i] += s[p];
                }
            }
            for &(p, i) in at_centre.get(&b).into_iter().flatten() {
                (mean[i], sigma[i]) = (m[p], s[p]);
            }
        }
    }
    for i in 0..counts.len() {
        if empty[i] {
            (mean[i], sigma[i]) = (f64::NAN, f64::NAN);
        } else if counts[i] > 0 {
            let n = f64::from(counts[i]);
            (mean[i], sigma[i]) = (mean[i] / n, sigma[i] / n);
        }
    }
    let field = Field {
        column: "MEAN".to_owned(),
        field: "dust_density".to_owned(),
        unit: Unit::ExtinctionZgr23PerParsec,
        kind: KindOfValue::Reconstructed,
        components: Components::Scalar,
        samples: Samples::Real(mean),
        uncertainty: CellUncertainty::StandardDeviation {
            column: "STD.".to_owned(),
            sigma,
        },
        labels: vec![],
    };
    Ok(Grid::new(SOURCE.to_owned(), geometry, vec![field])?.resampled(note(&map), NO_DATA))
}

/// What marks a cell of no data.
const NO_DATA: &str = "NaN: a cell whose centre lies nearer than 69 pc or farther than 1,250 pc \
                       from the Sun, outside the distances the map covers, or outside its \
                       outermost distance bins";

/// How the grid was resampled.
fn note(map: &Map) -> String {
    let (first, last) = (map.boundaries[0], map.boundaries[map.boundaries.len() - 1]);
    format!(
        "Each cell holds the mean of the source's values at the centres of the source's voxels \
         that fall within it, or, where none does, the source's value in the voxel that holds the \
         cell's centre. A voxel is a HEALPix pixel (NSIDE {}, NEST, galactic) of one of {} \
         distance bins from {first:.2} to {last:.2} pc, centred at the bin's RADIAL PIXEL CENTERS \
         distance along the pixel's centre. The standard deviation is resampled as the mean is.",
        1_u64 << map.order,
        map.centres.len()
    )
}

/// The nearest and farthest distances from the Sun of any point of the grid, in pc.
fn reach(geometry: &GridGeometry) -> (f64, f64) {
    let corner = geometry.corner();
    let (mut near, mut far) = (0.0, 0.0);
    for axis in Axis::XYZ {
        let k = axis.index();
        #[expect(clippy::cast_precision_loss, reason = "cell counts are far below 2^52")]
        let (lo, hi) = (
            corner[k],
            corner[k] + geometry.cells_along(axis) as f64 * geometry.cell,
        );
        let gap = lo.max(-hi).max(0.0);
        near += gap * gap;
        let out = lo.abs().max(hi.abs());
        far += out * out;
    }
    (f64::sqrt(near), f64::sqrt(far))
}

/// The sample index of the cell holding `position`, none outside the grid.
fn index(geometry: &GridGeometry, position: Vec3) -> Option<usize> {
    geometry.sample_index(geometry.cell_at(position)?)
}

/// By distance bin, the (pixel, cell) of each cell that holds data but no voxel centre.
type AtCentre = BTreeMap<usize, Vec<(usize, usize)>>;

/// Which cells hold no data, and, by distance bin, the (pixel, cell) of each cell that holds
/// data but no voxel centre: the voxel at its centre.
fn classify(geometry: &GridGeometry, map: &Map, counts: &[u32]) -> (Vec<bool>, AtCentre) {
    let mut empty = vec![false; counts.len()];
    let mut at_centre = AtCentre::new();
    let along = Axis::XYZ.map(|a| geometry.cells_along(a));
    for x in 0..along[0] {
        for y in 0..along[1] {
            for z in 0..along[2] {
                let cell = [x, y, z];
                let Some(i) = geometry.sample_index(cell) else {
                    continue;
                };
                let c = geometry.centre(cell);
                let r = c.iter().map(|v| v * v).sum::<f64>().sqrt();
                // The bin whose bounds hold r, none outside the outermost bounds.
                let bin = map.boundaries.partition_point(|&b| b <= r);
                let held = (1..map.boundaries.len()).contains(&bin);
                if !(NEAREST_PC..=FARTHEST_PC).contains(&r) || (counts[i] == 0 && !held) {
                    empty[i] = true;
                } else if counts[i] == 0 {
                    #[expect(clippy::cast_possible_truncation, reason = "a pixel of order ≤ 13")]
                    let pixel = healpix::hash(map.order, c) as usize;
                    at_centre.entry(bin - 1).or_default().push((pixel, i));
                }
            }
        }
    }
    (empty, at_centre)
}

/// One image read a distance bin at a time from a bin on.
struct Rows {
    reader: std::io::BufReader<std::fs::File>,
    scaling: (f64, f64),
    bytes: Vec<u8>,
}

impl Rows {
    fn open(path: &Path, image: Image, first: usize, row: usize) -> Result<Self, GridReadError> {
        let mut reader = open(path)?;
        let skip = u64::try_from(first * row).map_err(|_| super::layout("a bin beyond reach"))?;
        reader.seek(SeekFrom::Start(image.offset + skip))?;
        Ok(Self {
            reader,
            scaling: image.scaling,
            bytes: vec![0; row],
        })
    }

    /// The next bin's values, one per pixel.
    fn next(&mut self) -> Result<impl Iterator<Item = f64> + '_, GridReadError> {
        read_exact(&mut self.reader, &mut self.bytes)?;
        Ok(floats(&self.bytes, self.scaling))
    }
}
