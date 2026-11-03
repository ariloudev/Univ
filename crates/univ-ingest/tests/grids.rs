//! The volume-grid readers on crops of the real files (`tests/fixtures/grids/`, each with a YAML
//! sidecar naming the source file's SHA-256 and the crop), and the axis-order checks: the CF4
//! grids read in their published axis order put the density peak and the Laniakea basin at the
//! Laniakea attractor, and read with any other order do not.
//!
//! `full_cached_files_parse` reads the whole cached files and is run by hand:
//! `cargo test -p univ-ingest --test grids -- --ignored`, with `UNIV_INGEST_CACHE` naming the
//! cache directory when it is not the workspace's `.cache/univ-ingest`.

#![expect(
    clippy::unwrap_used,
    reason = "a failed unwrap in a test helper is the test failing"
)]

use std::path::{Path, PathBuf};

use serde::Deserialize;
use univ_ingest::grids::{Window, read_cf4_basins, read_cf4pp, read_twompp};
use univ_model::grid::{Axis, CellUncertainty, Components, Grid, Samples};
use univ_model::position::{Frame, Vec3, lon_lat};
use univ_model::units::Unit;
use univ_model::value::{KindOfValue, Uncertainty};

/// The central attractor of Laniakea in the CF4 reconstruction, supergalactic h⁻¹ Mpc: "The
/// Laniakea central attractor is located at [−62,−8,39] Mpc h^-1 in supergalactic cartesian
/// coordinates ... This is in full accordance with the direction of the Great Attractor (Dressler
/// et al., 1987a)." Dupuy & Courtois 2023, A&A 678, A176, §3 (and Table 1).
const LANIAKEA_ATTRACTOR: Vec3 = [-62.0, -8.0, 39.0];
const OBSERVER: Vec3 = [0.0, 0.0, 0.0];

/// The part of a fixture's sidecar the tests use.
#[derive(Deserialize)]
struct Sidecar {
    crop: Crop,
}

#[derive(Deserialize)]
struct Crop {
    start: [usize; 3],
}

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/grids")
}

/// The window a fixture holds, from its sidecar.
fn window(sidecar: &str) -> Window {
    let text = std::fs::read_to_string(fixtures().join(sidecar)).unwrap();
    let sidecar: Sidecar = serde_norway::from_str(&text).unwrap();
    Window::Cutout {
        start: sidecar.crop.start,
    }
}

fn cf4pp() -> Grid {
    read_cf4pp(
        &fixtures().join("cf4pp-crop.npz"),
        window("cf4pp-crop.yaml"),
    )
    .unwrap()
    .grid
}

fn basins() -> Grid {
    let zip = fixtures().join("cf4-basins-crop.zip");
    read_cf4_basins(&zip, window("cf4-basins-crop.yaml")).unwrap()
}

/// The grid read with its array axes in another order: every other permutation of its own.
fn permuted(grid: &Grid) -> Vec<(Grid, [Axis; 3])> {
    let [a, b, c] = grid.geometry.axes;
    [[a, c, b], [b, a, c], [b, c, a], [c, a, b], [c, b, a]]
        .into_iter()
        .map(|axes| {
            let mut wrong = grid.clone();
            wrong.geometry.axes = axes;
            (wrong, axes)
        })
        .collect()
}

/// The CF4++ density contrast at a position.
fn density_at(grid: &Grid, position: Vec3) -> Option<f64> {
    let cell = grid.geometry.cell_at(position)?;
    let d = grid.field("d_mean")?;
    Some(grid.value(d, cell, 0).unwrap()?.0.value)
}

/// Whether the CF4++ density marks the Laniakea attractor: overdense (δ > 0.5) in its own cell,
/// and a peak above δ = 2 within two cells (16 h⁻¹ Mpc) of it.
fn cf4pp_marks_laniakea(grid: &Grid) -> Result<(), String> {
    let at = density_at(grid, LANIAKEA_ATTRACTOR).ok_or("the attractor is off the grid")?;
    let near = (-2..=2_i32)
        .flat_map(|i| (-2..=2_i32).flat_map(move |j| (-2..=2_i32).map(move |k| [i, j, k])))
        .filter_map(|step| {
            let p = std::array::from_fn(|n| LANIAKEA_ATTRACTOR[n] + f64::from(step[n]) * 7.8125);
            density_at(grid, p)
        })
        .fold(f64::NEG_INFINITY, f64::max);
    if at > 0.5 && near > 2.0 {
        Ok(())
    } else {
        Err(format!(
            "δ = {at:.3} at the attractor, peak {near:.3} near it"
        ))
    }
}

/// Whether the basins map puts the observer and the Laniakea attractor in the basin it names
/// Laniakea.
fn basins_mark_laniakea(grid: &Grid) -> Result<(), String> {
    let boa = grid
        .field("CF4_new_128-z008_BoA.fits")
        .ok_or("no BoA map")?;
    let laniakea = boa
        .labels
        .iter()
        .find(|l| l.name == "Laniakea")
        .ok_or("no label named Laniakea")?
        .label;
    let label = |p: Vec3| grid.geometry.cell_at(p).and_then(|c| grid.label(boa, c));
    match (label(OBSERVER), label(LANIAKEA_ATTRACTOR)) {
        (Some(us), Some(there)) if us == laniakea && there == laniakea => Ok(()),
        other => Err(format!("labels {other:?} at the observer and attractor")),
    }
}

#[test]
fn cf4pp_reads_as_a_reconstructed_supergalactic_grid_with_standard_deviations() {
    let cf4pp = read_cf4pp(
        &fixtures().join("cf4pp-crop.npz"),
        window("cf4pp-crop.yaml"),
    )
    .unwrap();
    assert_eq!(cf4pp.realizations, 9739);
    let g = &cf4pp.grid.geometry;
    assert_eq!(g.frame, Frame::Supergalactic);
    assert_eq!(g.unit, Unit::MegaparsecOverH);
    assert_eq!(g.axes, Axis::XYZ);
    assert_eq!(g.shape, [17; 3]);
    assert_eq!(g.cell, 7.8125);
    // Cell 54 of the published grid is centred on (54 + 0.5) × 7.8125 − 500.
    assert_eq!(g.origin, [-74.21875; 3]);
    assert_eq!(g.cell_at(OBSERVER), Some([10; 3]));
    let fields: Vec<_> = cf4pp.grid.fields.iter().map(|f| f.field.as_str()).collect();
    assert_eq!(fields, ["density", "radial_velocity", "velocity_vector"]);
    for field in &cf4pp.grid.fields {
        assert_eq!(field.kind, KindOfValue::Reconstructed);
        let CellUncertainty::StandardDeviation { column, .. } = &field.uncertainty else {
            panic!("{} has no standard deviation", field.column);
        };
        assert_eq!(column, &field.column.replace("mean", "std"));
    }
    let d = cf4pp.grid.field("d_mean").unwrap();
    let (q, kind) = cf4pp.grid.value(d, [10; 3], 0).unwrap().unwrap();
    assert_eq!(kind, KindOfValue::Reconstructed);
    assert!(
        (q.value + 0.506).abs() < 1e-3,
        "δ at the observer {}",
        q.value
    );
    assert!(matches!(q.uncertainty, Uncertainty::Symmetric { sigma } if sigma > 0.0));
    let v = cf4pp.grid.field("v_mean").unwrap();
    assert_eq!(v.components, Components::Vector { axes: Axis::XYZ });
    assert_eq!(v.unit, Unit::KilometrePerSecond);
}

#[test]
fn cf4pp_density_peaks_at_the_laniakea_attractor_only_in_the_published_axis_order() {
    let grid = cf4pp();
    assert_eq!(cf4pp_marks_laniakea(&grid), Ok(()));
    for (wrong, axes) in permuted(&grid) {
        assert!(
            cf4pp_marks_laniakea(&wrong).is_err(),
            "the density read as {axes:?} still marks Laniakea"
        );
    }
}

#[test]
fn cf4pp_radial_velocity_is_the_velocity_along_the_line_of_sight_from_the_cell_centres() {
    // vr = v · r̂ holds only with the components in SGX, SGY, SGZ order and the cell centres at
    // (i + 0.5) × 7.8125 − 500 (sources/cf4pp-density.yaml).
    let grid = cf4pp();
    let (vr, v) = (
        grid.field("vr_mean").unwrap(),
        grid.field("v_mean").unwrap(),
    );
    let residual = |shift: f64, order: [usize; 3]| {
        let mut worst = 0.0_f64;
        for cell in (0..17).flat_map(|i| (0..17).flat_map(move |j| (0..17).map(move |k| [i, j, k])))
        {
            let r = grid.geometry.centre(cell).map(|x| x + shift);
            let norm = r.iter().map(|x| x * x).sum::<f64>().sqrt();
            let along: f64 = (0..3)
                .map(|c| grid.value(v, cell, order[c]).unwrap().unwrap().0.value * r[c] / norm)
                .sum();
            let radial = grid.value(vr, cell, 0).unwrap().unwrap().0.value;
            worst = worst.max((radial - along).abs());
        }
        worst
    };
    let published = residual(0.0, [0, 1, 2]);
    assert!(published < 1.0, "vr differs from v · r̂ by {published} km/s");
    assert!(residual(-3.906_25, [0, 1, 2]) > 10.0 * published.max(0.1));
    assert!(residual(0.0, [2, 1, 0]) > 10.0 * published.max(0.1));
}

#[test]
fn cf4_basins_read_as_labelled_supergalactic_grids_in_fits_axis_order() {
    let grid = basins();
    let g = &grid.geometry;
    assert_eq!(g.frame, Frame::Supergalactic);
    assert_eq!(g.axes, [Axis::Z, Axis::Y, Axis::X]);
    assert_eq!(g.shape, [17; 3]);
    assert_eq!(g.unit, Unit::MegaparsecOverH);
    assert_eq!(g.assumed_hubble_constant, Some(74.6));
    // Voxel 54 lies at (54 − 64) × 7.8125, the convention of Dupuy & Courtois 2023 Table 1.
    assert_eq!(g.origin, [-78.125; 3]);
    assert_eq!(grid.fields.len(), 4);
    for field in &grid.fields {
        assert_eq!(field.field, "basin_label");
        assert_eq!(field.kind, KindOfValue::Reconstructed);
        assert_eq!(field.uncertainty, CellUncertainty::NoneReported);
        assert!(matches!(field.samples, Samples::Label(_)));
    }
    let names: Vec<_> = grid.fields[0]
        .labels
        .iter()
        .map(|l| (l.label, l.name.as_str()))
        .collect();
    assert_eq!(names[0], (1, "Laniakea"));
    assert_eq!(names[8], (9, "SDSS-2b"));
    assert_eq!(grid.fields[1].labels, Vec::new());
}

#[test]
fn the_laniakea_basin_holds_its_attractor_only_in_the_published_axis_order() {
    let grid = basins();
    assert_eq!(basins_mark_laniakea(&grid), Ok(()));
    for (wrong, axes) in permuted(&grid) {
        assert!(
            basins_mark_laniakea(&wrong).is_err(),
            "the basins read as {axes:?} still put the attractor in Laniakea"
        );
    }
}

#[test]
fn twompp_reads_as_a_galactic_grid_with_no_uncertainty_reported() {
    let dir = fixtures();
    let grid = read_twompp(
        &dir.join("twompp_density-crop.npy"),
        &dir.join("twompp_velocity-crop.npy"),
        window("twompp-crop.yaml"),
    )
    .unwrap();
    let geometry = &grid.geometry;
    assert_eq!(
        (geometry.frame, geometry.unit, geometry.axes),
        (Frame::Galactic, Unit::MegaparsecOverH, Axis::XYZ)
    );
    assert_eq!((geometry.shape, geometry.cell), ([9; 3], 1.5625));
    // X = (i − 128) × 400 / 256: cell 124 is at −6.25.
    assert_eq!(geometry.origin, [-6.25; 3]);
    let local_group = geometry.cell_at(OBSERVER).unwrap();
    assert_eq!(local_group, [4; 3]);
    for field in &grid.fields {
        assert_eq!(field.kind, KindOfValue::Reconstructed);
        assert_eq!(field.uncertainty, CellUncertainty::NoneReported);
    }
    let density = grid.field("twompp_density.npy").unwrap();
    let (delta, _) = grid.value(density, local_group, 0).unwrap().unwrap();
    assert!((delta.value - 0.291_86).abs() < 1e-4);
    assert_eq!(delta.uncertainty, Uncertainty::NoneReported);
    // The Local Group's predicted velocity, observed in sources/twompp-reconstruction.yaml as
    // (66.2, −576.3, 402.5) km/s toward l = 276.5°, b = 34.8°.
    let velocity = grid.field("twompp_velocity.npy").unwrap();
    let motion: Vec3 = std::array::from_fn(|c| {
        let value = grid.value(velocity, local_group, c).unwrap();
        value.unwrap().0.value
    });
    let (lon, lat) = lon_lat(motion);
    assert!(
        (lon - 276.5).abs() < 0.1 && (lat - 34.8).abs() < 0.1,
        "l = {lon}, b = {lat}"
    );
}

#[test]
fn a_file_that_is_not_the_window_it_claims_is_refused() {
    let npz = fixtures().join("cf4pp-crop.npz");
    let whole = read_cf4pp(&npz, Window::Whole).unwrap_err();
    assert!(whole.to_string().contains("128"), "{whole}");
    assert!(read_cf4pp(&npz, Window::Cutout { start: [120, 0, 0] }).is_err());
    let missing = read_cf4pp(&fixtures().join("cf4-basins-crop.zip"), Window::Whole);
    assert!(missing.is_err());
}

fn cache() -> PathBuf {
    std::env::var_os("UNIV_INGEST_CACHE").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.cache/univ-ingest"),
        PathBuf::from,
    )
}

/// The range of a field's real samples; `None` for a label field.
fn range(grid: &Grid, column: &str) -> Option<(f64, f64)> {
    let Samples::Real(values) = &grid.field(column)?.samples else {
        return None;
    };
    Some(
        values
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &x| {
                (lo.min(x), hi.max(x))
            }),
    )
}

#[test]
#[ignore = "reads the whole cached files, about 800 MB; run by hand"]
fn full_cached_files_parse() {
    let cache = cache();
    let at = |p: &str| -> PathBuf { Path::new(&cache).join(p) };
    let cf4pp = read_cf4pp(
        &at("cf4pp-density/new_CF4pp_mean_std_grids.npz"),
        Window::Whole,
    )
    .unwrap();
    assert_eq!(cf4pp.grid.geometry.shape, [128; 3]);
    assert_eq!(cf4pp_marks_laniakea(&cf4pp.grid), Ok(()));
    for column in ["d_mean", "vr_mean", "v_mean"] {
        eprintln!("cf4pp {column}: {:?}", range(&cf4pp.grid, column).unwrap());
    }

    let basins = read_cf4_basins(
        &at("cf4-basins/CF4_new_128-z008_watersheds-fits.zip"),
        Window::Whole,
    )
    .unwrap();
    assert_eq!(basins_mark_laniakea(&basins), Ok(()));
    let boa = &basins.fields[0];
    let Samples::Label(labels) = &boa.samples else {
        panic!("not labels")
    };
    // Dupuy & Courtois 2023 §3: "The segmented Laniakea basin has a volume of 1.9×10^6 (Mpc h^-1)^3."
    let volume = f64::from(u32::try_from(labels.iter().filter(|&&l| l == 1).count()).unwrap())
        * 7.8125_f64.powi(3);
    assert!(
        (volume / 1.9e6 - 1.0).abs() < 0.05,
        "Laniakea volume {volume}"
    );
    // The nine CF4 galaxy attractors of Table 1 (§3 for the first six) lie in their own basins.
    let attractors: [Vec3; 9] = [
        [-62.0, -8.0, 39.0],
        [-125.0, -23.0, -31.0],
        [-39.0, 70.0, 78.0],
        [-62.0, -39.0, -109.0],
        [47.0, -23.0, -31.0],
        [-141.0, 62.0, -16.0],
        [117.0, 227.0, -227.0],
        [-102.0, 297.0, 102.0],
        [31.0, 289.0, 156.0],
    ];
    for (label, p) in (1..).zip(attractors) {
        let cell = basins.geometry.cell_at(p).unwrap();
        assert_eq!(
            basins.label(boa, cell),
            Some(label),
            "attractor {label} at {p:?}"
        );
    }
    for field in &basins.fields {
        let Samples::Label(l) = &field.samples else {
            panic!("not labels")
        };
        eprintln!("{}: labels 0..={}", field.column, l.iter().max().unwrap());
    }

    let twompp = read_twompp(
        &at("twompp-reconstruction/twompp_density.npy"),
        &at("twompp-reconstruction/twompp_velocity.npy"),
        Window::Whole,
    )
    .unwrap();
    assert_eq!(twompp.geometry.shape, [257; 3]);
    for column in ["twompp_density.npy", "twompp_velocity.npy"] {
        eprintln!("2M++ {column}: {:?}", range(&twompp, column).unwrap());
    }
}
