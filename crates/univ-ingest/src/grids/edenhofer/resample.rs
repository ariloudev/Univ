//! The dust map resampled onto the published grid (resample-the-dust-map): each cell holds the
//! mean of the source's values at the centres of the source's voxels that fall within it, or,
//! where none does, the source's value at the cell's centre; a cell whose centre lies outside
//! the map's distance bins holds NaN, which marks no data.
//!
//! One pass streams the two images a distance bin at a time, counting and summing the voxels of
//! each cell; the cells it leaves without a voxel then read the few bins that hold their centres.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, Seek, SeekFrom};
use std::path::Path;

use univ_model::grid::{Axis, CellUncertainty, Components, Field, Grid, GridGeometry, Samples};
use univ_model::healpix;
use univ_model::position::{Vec3, norm};
use univ_model::units::Unit;
use univ_model::value::KindOfValue;

use super::{FILE, Image, Map, PUBLISHED, SOURCE, layout, read_map};
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
    let directions: Vec<Vec3> = (0..map.mean.pixels() as u64)
        .map(|p| healpix::centre(map.mean.order, p))
        .collect();
    // The bins whose centres can fall in the grid: the bins increase, so they run together.
    let (near, far) = reach(&geometry);
    let shells =
        map.centres.partition_point(|&c| c < near)..map.centres.partition_point(|&c| c <= far);
    let cells = geometry.cells();
    let (mut counts, mut mean, mut sigma) =
        (vec![0_u32; cells], vec![0.0; cells], vec![0.0; cells]);
    let mut rows = Rows::open(path, &map)?;
    for b in shells {
        let (m, s) = rows.bin(b)?;
        let d = map.centres[b];
        for (p, u) in directions.iter().enumerate() {
            let at = geometry.cell_at(u.map(|c| c * d));
            if let Some(i) = at.and_then(|cell| geometry.sample_index(cell)) {
                counts[i] += 1;
                mean[i] += m[p];
                sigma[i] += s[p];
            }
        }
    }
    let at_centre = finish(&geometry, &map, &counts, &mut mean, &mut sigma);
    for (&b, held) in &at_centre {
        let (m, s) = rows.bin(b)?;
        for &(p, i) in held {
            (mean[i], sigma[i]) = (m[p], s[p]);
        }
    }
    let field = Field {
        column: "MEAN".to_owned(),
        field: "density".to_owned(),
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
    let mut grid = Grid::new(SOURCE.to_owned(), geometry, vec![field])?;
    grid.resampling = Some(note(&map));
    let (inner, outer) = map.reach();
    grid.no_data = Some(format!(
        "NaN: a cell whose centre lies nearer than {inner:.2} pc or farther than {outer:.2} pc \
         from the Sun, outside the map's distance bins"
    ));
    Ok(grid)
}

/// How the grid was resampled.
fn note(map: &Map) -> String {
    let (inner, outer) = map.reach();
    format!(
        "Each cell holds the mean of the source's values at the centres of the source's voxels \
         that fall within it, or, where none does, the source's value in the voxel that holds the \
         cell's centre. A voxel is a HEALPix pixel (NSIDE {}, NEST, galactic) of one of {} \
         distance bins from {inner:.2} to {outer:.2} pc, centred at the bin's RADIAL PIXEL CENTERS \
         distance along the pixel's centre. A cell's standard deviation is likewise the mean of \
         its voxels' standard deviations: the deviation of the cell's mean where the voxels vary \
         together, an upper bound on it where they do not.",
        1_u64 << map.mean.order,
        map.mean.bins
    )
}

/// The nearest and farthest distances from the Sun of any point of the grid, in pc.
fn reach(geometry: &GridGeometry) -> (f64, f64) {
    let corner = geometry.corner();
    let (mut near, mut far) = ([0.0; 3], [0.0; 3]);
    for axis in Axis::XYZ {
        let k = axis.index();
        #[expect(clippy::cast_precision_loss, reason = "cell counts are far below 2^52")]
        let (lo, hi) = (
            corner[k],
            corner[k] + geometry.cells_along(axis) as f64 * geometry.cell,
        );
        near[k] = lo.max(-hi).max(0.0);
        far[k] = lo.abs().max(hi.abs());
    }
    (norm(near), norm(far))
}

/// By distance bin, the (pixel, cell) of each cell that holds data but no voxel centre: the
/// voxel at its centre.
type AtCentre = BTreeMap<usize, Vec<(usize, usize)>>;

/// Turns the sums into means, marks the cells outside the map's bins as of no data, and gives
/// the voxel at the centre of each cell within them that no voxel centre fell in.
fn finish(
    geometry: &GridGeometry,
    map: &Map,
    counts: &[u32],
    mean: &mut [f64],
    sigma: &mut [f64],
) -> AtCentre {
    let (inner, outer) = map.reach();
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
                let r = norm(c);
                if !(inner..=outer).contains(&r) {
                    (mean[i], sigma[i]) = (f64::NAN, f64::NAN);
                } else if counts[i] > 0 {
                    let n = f64::from(counts[i]);
                    (mean[i], sigma[i]) = (mean[i] / n, sigma[i] / n);
                } else {
                    // The bin whose bounds hold r; r lies within the outermost bounds.
                    let bin = map
                        .boundaries
                        .partition_point(|&b| b <= r)
                        .min(map.mean.bins)
                        - 1;
                    #[expect(clippy::cast_possible_truncation, reason = "a pixel of order ≤ 13")]
                    let pixel = healpix::hash(map.mean.order, c) as usize;
                    at_centre.entry(bin).or_default().push((pixel, i));
                }
            }
        }
    }
    at_centre
}

/// The two images, each read a distance bin at a time.
struct Rows {
    images: [(Image, BufReader<File>); 2],
}

impl Rows {
    fn open(path: &Path, map: &Map) -> Result<Self, GridReadError> {
        Ok(Self {
            images: [(map.mean, open(path)?), (map.std, open(path)?)],
        })
    }

    /// The mean and the standard deviation of bin `b`, one per pixel. Reading the bins in
    /// order seeks to where each reader already is.
    fn bin(&mut self, b: usize) -> Result<(Vec<f64>, Vec<f64>), GridReadError> {
        let row = |(image, reader): &mut (Image, BufReader<File>)| {
            let skip =
                u64::try_from(b * image.pixels() * 4).map_err(|_| layout("a bin beyond reach"))?;
            reader.seek(SeekFrom::Start(image.offset + skip))?;
            image.values(reader, 1)
        };
        let [m, s] = &mut self.images;
        Ok((row(m)?, row(s)?))
    }
}
