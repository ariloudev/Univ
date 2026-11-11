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

mod common;

use std::path::{Path, PathBuf};

use univ_ingest::fits::BLOCK;
use univ_ingest::grids::edenhofer::CELLS;
use univ_ingest::grids::{GridReadError, Window, read_edenhofer};
use univ_ingest::release::{grid_files, is_grid, read_grid};
use univ_ingest::tiles::sha256;
use univ_model::derive::Release;
use univ_model::grid::{CellUncertainty, Grid, Samples};
use univ_model::healpix;
use univ_model::position::{Frame, norm};
use univ_model::release::keys;
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
    out.resize(out.len().div_ceil(BLOCK) * BLOCK, b' ');
    let start = out.len();
    out.extend(data);
    out.resize(start + data.len().div_ceil(BLOCK) * BLOCK, 0);
    out
}

/// An extension of two axes, `naxis`, with its own cards after the required ones, and its data.
fn extension(kind: &str, bitpix: &str, naxis: [usize; 2], own: &[String], data: &[u8]) -> Vec<u8> {
    let mut cards = vec![
        card("XTENSION", &text(kind)),
        card("BITPIX", bitpix),
        card("NAXIS", "2"),
        card("NAXIS1", &naxis[0].to_string()),
        card("NAXIS2", &naxis[1].to_string()),
        card("PCOUNT", "0"),
        card("GCOUNT", "1"),
    ];
    cards.extend_from_slice(own);
    hdu(&cards, data)
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
            let own = [
                card("EXTNAME", &text(name)),
                card("NSIDE", &(1_u32 << ORDER).to_string()),
                card("ORDERING", &text(ordering)),
                card("CUNIT", &text(UNIT)),
            ];
            let data = be((0..BINS).flat_map(|b| (0..n).map(move |p| value(b, p))));
            extension("IMAGE", "-32", [n, BINS], &own, &data)
        };
        let table = |name: &str, values: &[f32]| {
            let own = [
                card("TFIELDS", "1"),
                card("TFORM1", &text("E")),
                card("EXTNAME", &text(name)),
            ];
            let data = be(values.iter().copied());
            extension("BINTABLE", "8", [4, values.len()], &own, &data)
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
        let (inner, outer) = (
            f64::from(self.boundaries[0]),
            f64::from(self.boundaries[BINS]),
        );
        let mut out = vec![(None, false); g.cells()];
        for x in 0..g.shape[0] {
            for y in 0..g.shape[1] {
                for z in 0..g.shape[2] {
                    let i = g.sample_index([x, y, z]).unwrap();
                    let c = g.centre([x, y, z]);
                    let r = norm(c);
                    let (mean, std, n) = sums[i];
                    out[i] = if !(inner..=outer).contains(&r) {
                        (None, n > 0)
                    } else if n > 0 {
                        (Some((mean / f64::from(n), std / f64::from(n))), true)
                    } else {
                        let bin = self.boundaries.partition_point(|&b| f64::from(b) <= r);
                        let b = bin.min(BINS) - 1;
                        let p = usize::try_from(healpix::hash(ORDER, c)).unwrap();
                        let at = (f64::from(Self::mean(b, p)), f64::from(Self::std(b, p)));
                        (Some(at), false)
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
        ("MEAN", "density")
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

#[test]
fn the_release_writes_the_mean_and_its_deviation_named_beside_it_the_same_from_the_same_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = Simulated::new().write(dir.path(), "NEST");
    let definition = common::definition_of("edenhofer-2024-dust");
    assert!(is_grid(&definition));
    // A grid, though its file is FITS: no table of rows to normalize.
    assert!(!univ_ingest::normalize::reads_rows(&definition));
    // The whole grid is read from the file the fetch gives, by its name.
    let missing = read_grid(&definition, &[]).unwrap_err();
    assert!(missing.contains("mean_and_std_healpix.fits"), "{missing}");
    let build = || {
        let window = Window::Cutout {
            start: [129, 120, 120],
        };
        let grid = read_edenhofer(&path, window, [12, 10, 10]).unwrap();
        grid_files(13, &grid, &definition, &Release::planck_2018()).unwrap()
    };
    let (first, second) = (build(), build());
    let digests = |files: &[(String, Vec<u8>)]| -> Vec<(String, String)> {
        files.iter().map(|(p, b)| (p.clone(), sha256(b))).collect()
    };
    assert_eq!(digests(&first.files), digests(&second.files));
    assert_eq!(first.row.header_key, second.row.header_key);
    let id = "edenhofer-2024-dust";
    let file = |path: String| &first.files.iter().find(|(p, _)| *p == path).unwrap().1;
    let header: serde_json::Value = serde_json::from_slice(file(keys::grid_header(id))).unwrap();
    assert_eq!(header["frame"], "galactic");
    assert_eq!(header["unit"], "parsec");
    assert_eq!(header["cell"], 10.0);
    assert!(
        header["resampling"]
            .as_str()
            .unwrap()
            .contains("mean of the source's values")
    );
    assert!(header["no_data"].as_str().unwrap().starts_with("NaN"));
    let field = &header["fields"][0];
    assert_eq!(field["unit"], "extinction_zgr23_per_parsec");
    assert_eq!(field["kind"], "reconstructed");
    // The deviation is written as its own grid of samples, named under the mean it belongs to.
    let sigma = file(keys::grid_samples(id, "field-0-sigma"));
    assert_eq!(field["uncertainty"]["form"], "standard_deviation");
    assert_eq!(field["uncertainty"]["column"], "STD.");
    assert_eq!(field["uncertainty"]["samples"], keys::file(&sha256(sigma)));
    let mean = file(keys::grid_samples(id, "field-0"));
    assert_eq!(field["samples"], keys::file(&sha256(mean)));
    // Both mark the same cells as of no data.
    let nan = |bytes: &[u8]| -> Vec<bool> {
        let floats = bytes.as_chunks::<4>().0.iter();
        floats.map(|b| f32::from_le_bytes(*b).is_nan()).collect()
    };
    assert_eq!(nan(mean), nan(sigma));
    assert!(nan(mean).contains(&true) && nan(mean).contains(&false));
}

/// Run by hand on the real map, `UNIV_EDENHOFER_FILE` naming `mean_and_std_healpix.fits`:
/// `cargo test --release -p univ-ingest --test grids_edenhofer -- --ignored --nocapture`.
#[test]
#[ignore = "reads the real 3.25 GB map"]
fn the_real_map_resamples_whole() {
    let path = PathBuf::from(std::env::var("UNIV_EDENHOFER_FILE").unwrap());
    let started = std::time::Instant::now();
    let grid = read_edenhofer(&path, Window::Whole, CELLS).unwrap();
    let seconds = started.elapsed().as_secs_f64();
    let (mean, std) = (reals(&grid.fields[0].samples), sigma(&grid));
    let g = &grid.geometry;
    let held: Vec<usize> = (0..mean.len()).filter(|&i| !mean[i].is_nan()).collect();
    // The cells within 69–1,250 pc of the Sun: a spherical shell's share of the cube.
    // The bins reach from 68.62 to 1,248.10 pc in the published file.
    let (inner, outer) = (68.618_73_f64, 1_248.100_1_f64);
    let shell = std::f64::consts::FRAC_PI_6 * (outer.powi(3) - inner.powi(3)) / 1_250_f64.powi(3);
    #[expect(clippy::cast_precision_loss)]
    let share = held.len() as f64 / mean.len() as f64;
    assert!((share - shell).abs() < 0.002, "{share} not {shell}");
    assert!(
        held.iter()
            .all(|&i| mean[i] >= 0.0 && std[i] >= 0.0 && !std[i].is_nan())
    );
    let mut sorted: Vec<f64> = held.iter().map(|&i| mean[i]).collect();
    sorted.sort_by(f64::total_cmp);
    let quantile = |q: f64| {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let at = ((sorted.len() - 1) as f64 * q) as usize;
        sorted[at]
    };
    let peak = held
        .iter()
        .copied()
        .max_by(|&a, &b| mean[a].total_cmp(&mean[b]))
        .unwrap();
    let at = |i: usize| {
        let (n, m) = (g.shape[1] * g.shape[2], g.shape[2]);
        g.centre([i / n, (i % n) / m, i % m])
    };
    let c = at(peak);
    let r = norm(c);
    let l = c[1].atan2(c[0]).to_degrees().rem_euclid(360.0);
    let b = (c[2] / r).asin().to_degrees();
    eprintln!(
        "resampled in {seconds:.1} s; {} of {} cells hold data ({share:.4}); density per pc \
         median {:.3e}, 99th percentile {:.3e}, max {:.3e} at l {l:.1}, b {b:.1}, {r:.0} pc",
        held.len(),
        mean.len(),
        quantile(0.5),
        quantile(0.99),
        mean[peak]
    );
}

#[test]
fn a_map_holding_a_value_that_is_not_finite_is_refused_so_nan_marks_only_no_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = Simulated::new().write(dir.path(), "NEST");
    // The first value of MEAN, after the primary HDU's block and MEAN's header block.
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[2 * BLOCK..2 * BLOCK + 4].copy_from_slice(&f32::NAN.to_be_bytes());
    std::fs::write(&path, bytes).unwrap();
    let refused = read_edenhofer(
        &path,
        Window::Cutout {
            start: [129, 120, 120],
        },
        [2; 3],
    );
    let refused = refused.unwrap_err().to_string();
    assert!(refused.contains("not finite"), "{refused}");
}
