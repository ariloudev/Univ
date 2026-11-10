//! The Edenhofer 2024 dust map resampled onto a Cartesian grid about the Sun
//! (resample-the-dust-map). The map read here is SIMULATED: a small file in the published layout
//! (`HEALPix` NEST pixels × logarithmic distance bins, `MEAN` and `STD.` images, the bins' centres
//! and boundaries), written by the test with values chosen per bin and pixel, so that every cell
//! can be checked against a brute-force reading of the same map.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    clippy::many_single_char_names,
    reason = "the test's helpers fail the test by unwrapping or panicking, and name a map's \
              bins, pixels and coordinates as the paper does"
)]

use std::path::{Path, PathBuf};

use univ_ingest::grids::edenhofer::{CELLS, FARTHEST_PC, NEAREST_PC};
use univ_ingest::grids::{GridReadError, Window, read_edenhofer};
use univ_model::grid::{CellUncertainty, Grid, Samples};
use univ_model::healpix;
use univ_model::position::Frame;
use univ_model::units::Unit;
use univ_model::value::KindOfValue;

const ORDER: u8 = 3;
const BINS: usize = 64;
const UNIT: &str = "E of Zhang, Green, and Rix (2023)";

/// One header card of 80 characters.
fn card(key: &str, value: &str) -> String {
    format!("{key:<8}= {value:<70}")
}

fn text(s: &str) -> String {
    format!("'{s:<8}'")
}

/// A header and its data, each padded to whole blocks of 2,880 bytes.
fn hdu(cards: &[String], data: &[u8]) -> Vec<u8> {
    let mut out: Vec<u8> = cards.concat().into_bytes();
    out.extend(format!("{:<80}", "END").bytes());
    out.resize(out.len().div_ceil(2_880) * 2_880, b' ');
    let start = out.len();
    out.extend(data);
    out.resize(start + data.len().div_ceil(2_880) * 2_880, 0);
    out
}

fn be(values: impl Iterator<Item = f32>) -> Vec<u8> {
    values.flat_map(f32::to_be_bytes).collect()
}

/// The simulated map: its bins' boundaries and centres (as stored, 32-bit), and its mean and
/// standard deviation at bin `b` and pixel `p`.
struct Simulated {
    boundaries: Vec<f32>,
    centres: Vec<f32>,
}

impl Simulated {
    fn new() -> Self {
        let (first, last) = (68.618_73_f64, 1_248.100_1_f64);
        #[expect(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
        let boundaries: Vec<f32> = (0..=BINS)
            .map(|i| (first * (last / first).powf(i as f64 / BINS as f64)) as f32)
            .collect();
        #[expect(clippy::cast_possible_truncation)]
        let centres = boundaries
            .windows(2)
            .map(|b| (f64::from(b[0]) * f64::from(b[1])).sqrt() as f32)
            .collect();
        Self {
            boundaries,
            centres,
        }
    }

    #[expect(clippy::cast_precision_loss)]
    fn mean(b: usize, p: usize) -> f32 {
        b as f32 + p as f32 / 1_000.0
    }

    fn std(b: usize, p: usize) -> f32 {
        0.5 * Self::mean(b, p) + 0.25
    }

    fn pixels() -> usize {
        12 << (2 * ORDER)
    }

    /// The file, with `ordering` in place of NEST where given.
    fn write(&self, dir: &Path, ordering: &str) -> PathBuf {
        let n = Self::pixels();
        let image = |name: &str, value: fn(usize, usize) -> f32| {
            let cards = [
                card("XTENSION", &text("IMAGE")),
                card("BITPIX", "-32"),
                card("NAXIS", "2"),
                card("NAXIS1", &n.to_string()),
                card("NAXIS2", &BINS.to_string()),
                card("PCOUNT", "0"),
                card("GCOUNT", "1"),
                card("EXTNAME", &text(name)),
                card("NSIDE", &(1_u32 << ORDER).to_string()),
                card("ORDERING", &text(ordering)),
                card("CUNIT", &text(UNIT)),
            ];
            let data = be((0..BINS).flat_map(|b| (0..n).map(move |p| value(b, p))));
            hdu(&cards, &data)
        };
        let table = |name: &str, values: &[f32]| {
            let cards = [
                card("XTENSION", &text("BINTABLE")),
                card("BITPIX", "8"),
                card("NAXIS", "2"),
                card("NAXIS1", "4"),
                card("NAXIS2", &values.len().to_string()),
                card("PCOUNT", "0"),
                card("GCOUNT", "1"),
                card("TFIELDS", "1"),
                card("TFORM1", &text("E")),
                card("EXTNAME", &text(name)),
            ];
            hdu(&cards, &be(values.iter().copied()))
        };
        let primary = [
            card("SIMPLE", "T"),
            card("BITPIX", "8"),
            card("NAXIS", "0"),
            card("EXTEND", "T"),
        ];
        let mut file = hdu(&primary, &[]);
        file.extend(image("MEAN", Self::mean));
        file.extend(image("STD.", Self::std));
        file.extend(table("RADIAL PIXEL CENTERS", &self.centres));
        file.extend(table("RADIAL PIXEL BOUNDARIES", &self.boundaries));
        let path = dir.join("mean_and_std_healpix.fits");
        std::fs::write(&path, file).unwrap();
        path
    }

    /// What each cell of `grid` should hold, read from the map by brute force: (mean, std), or
    /// none for no data, and whether a voxel centre fell in it.
    fn expected(&self, grid: &Grid) -> Vec<(Option<(f64, f64)>, bool)> {
        let g = &grid.geometry;
        let mut sums = vec![(0.0, 0.0, 0_u32); g.cells()];
        for (b, &d) in self.centres.iter().enumerate() {
            for p in 0..Self::pixels() {
                let u = healpix::centre(ORDER, p as u64);
                let Some(cell) = g.cell_at(u.map(|c| c * f64::from(d))) else {
                    continue;
                };
                let s = &mut sums[g.sample_index(cell).unwrap()];
                s.0 += f64::from(Self::mean(b, p));
                s.1 += f64::from(Self::std(b, p));
                s.2 += 1;
            }
        }
        let mut out = vec![(None, false); g.cells()];
        for x in 0..g.shape[0] {
            for y in 0..g.shape[1] {
                for z in 0..g.shape[2] {
                    let i = g.sample_index([x, y, z]).unwrap();
                    let c = g.centre([x, y, z]);
                    let r = c.iter().map(|v| v * v).sum::<f64>().sqrt();
                    let (mean, std, n) = sums[i];
                    out[i] = if !(NEAREST_PC..=FARTHEST_PC).contains(&r) {
                        (None, n > 0)
                    } else if n > 0 {
                        (Some((mean / f64::from(n), std / f64::from(n))), true)
                    } else {
                        let bin = self.boundaries.partition_point(|&b| f64::from(b) <= r);
                        let at = (1..=BINS).contains(&bin).then(|| {
                            let p = usize::try_from(healpix::hash(ORDER, c)).unwrap();
                            let b = bin - 1;
                            (f64::from(Self::mean(b, p)), f64::from(Self::std(b, p)))
                        });
                        (at, false)
                    };
                }
            }
        }
        out
    }
}

fn reals(samples: &Samples) -> &[f64] {
    match samples {
        Samples::Real(v) => v,
        Samples::Label(_) => panic!("labels"),
    }
}

fn sigma(grid: &Grid) -> &[f64] {
    match &grid.fields[0].uncertainty {
        CellUncertainty::StandardDeviation { sigma, .. } => sigma,
        CellUncertainty::NoneReported => panic!("no standard deviation"),
    }
}

#[test]
fn each_cell_holds_the_mean_of_the_voxels_in_it_or_the_voxel_at_its_centre_or_no_data() {
    let dir = tempfile::tempdir().unwrap();
    let map = Simulated::new();
    let path = map.write(dir.path(), "NEST");
    // Cells 129–140 along X (45 to 155 pc) and 120–129 along Y and Z (−45 to 45 pc): some
    // within 69 pc of the Sun, the rest holding voxel centres or none.
    let window = Window::Cutout {
        start: [129, 120, 120],
    };
    let grid = read_edenhofer(&path, window, [12, 10, 10]).unwrap();
    let (mean, std) = (reals(&grid.fields[0].samples), sigma(&grid));
    let mut kinds = [0; 3];
    for (i, (expected, voxels)) in map.expected(&grid).into_iter().enumerate() {
        match expected {
            None => {
                assert!(mean[i].is_nan() && std[i].is_nan(), "cell {i}: {}", mean[i]);
                kinds[0] += 1;
            }
            Some((m, s)) => {
                assert!((mean[i] - m).abs() < 1e-9, "cell {i}: {} not {m}", mean[i]);
                assert!((std[i] - s).abs() < 1e-9, "cell {i}: {} not {s}", std[i]);
                kinds[if voxels { 1 } else { 2 }] += 1;
            }
        }
    }
    assert!(
        kinds.iter().all(|&n| n > 0),
        "no data, voxel means, centre values: {kinds:?}"
    );
    // No value is zero where the map holds none.
    assert!(!mean.contains(&0.0));
}

#[test]
fn the_cells_beyond_the_map_s_reach_hold_no_data_never_zero() {
    let dir = tempfile::tempdir().unwrap();
    let path = Simulated::new().write(dir.path(), "NEST");
    // The grid's corner, 2,157 pc from the Sun, and the cells about the Sun, within 69 pc.
    for (start, shape) in [([0; 3], [3; 3]), ([122; 3], [6; 3])] {
        let grid = read_edenhofer(&path, Window::Cutout { start }, shape).unwrap();
        assert!(reals(&grid.fields[0].samples).iter().all(|v| v.is_nan()));
        assert!(sigma(&grid).iter().all(|v| v.is_nan()));
    }
}

#[test]
fn the_grid_is_a_galactic_cube_of_2500_pc_about_the_sun_in_10_pc_cells() {
    let dir = tempfile::tempdir().unwrap();
    let path = Simulated::new().write(dir.path(), "NEST");
    assert_eq!(CELLS, [250; 3]);
    let first = read_edenhofer(&path, Window::Cutout { start: [0; 3] }, [1; 3]).unwrap();
    let last = read_edenhofer(&path, Window::Cutout { start: [249; 3] }, [1; 3]).unwrap();
    let g = &first.geometry;
    assert_eq!(
        (g.frame, g.unit, g.cell),
        (Frame::Galactic, Unit::Parsec, 10.0)
    );
    assert_eq!(g.corner(), [-1_250.0; 3]);
    assert_eq!(last.geometry.origin.map(|c| c + 5.0), [1_250.0; 3]);
    let beyond = read_edenhofer(&path, Window::Cutout { start: [249; 3] }, [2; 3]);
    assert!(
        matches!(beyond, Err(GridReadError::Layout { .. })),
        "{beyond:?}"
    );
}

#[test]
fn the_mean_and_its_standard_deviation_are_reconstructed_in_the_source_s_unit_and_say_how() {
    let dir = tempfile::tempdir().unwrap();
    let path = Simulated::new().write(dir.path(), "NEST");
    let read = || {
        read_edenhofer(
            &path,
            Window::Cutout {
                start: [129, 120, 120],
            },
            [4; 3],
        )
        .unwrap()
    };
    let grid = read();
    let field = &grid.fields[0];
    assert_eq!(field.unit, Unit::ExtinctionZgr23PerParsec);
    assert_eq!(field.kind, KindOfValue::Reconstructed);
    assert_eq!(
        (field.column.as_str(), field.field.as_str()),
        ("MEAN", "dust_density")
    );
    let CellUncertainty::StandardDeviation { column, .. } = &field.uncertainty else {
        panic!("no standard deviation")
    };
    assert_eq!(column, "STD.");
    let resampling = grid.resampling.as_deref().unwrap();
    assert!(
        resampling.contains("mean of the source's values"),
        "{resampling}"
    );
    assert!(resampling.contains("NSIDE 8, NEST"), "{resampling}");
    assert!(grid.no_data.as_deref().unwrap().starts_with("NaN"));
    // Read again, the same bits.
    let again = read();
    let bits = |g: &Grid| -> Vec<u64> {
        let m = reals(&g.fields[0].samples).iter();
        m.chain(sigma(g)).map(|v| v.to_bits()).collect()
    };
    assert_eq!(bits(&grid), bits(&again));
}

#[test]
fn a_map_in_another_ordering_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = Simulated::new().write(dir.path(), "RING");
    let refused = read_edenhofer(&path, Window::Cutout { start: [0; 3] }, [1; 3]).unwrap_err();
    assert!(refused.to_string().contains("not NEST"), "{refused}");
}
