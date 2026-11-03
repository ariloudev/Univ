//! The SIMULATED grid, catalogues and void structure the outline tests share: nothing here is
//! real data. The grid is 16 × 14 × 12 cells of 10 h⁻¹ Mpc in supergalactic coordinates, stored
//! Z slowest, then X, then Y. Its density contrast is 0 except for a simulated void of 19 cells
//! at −1.5 around (35, 5, 5) h⁻¹ Mpc.
#![allow(
    dead_code,
    clippy::unwrap_used,
    reason = "helpers shared by several test crates, each using a subset; a failed unwrap is the \
              test failing"
)]

use std::collections::BTreeMap;
use std::fs;

use univ_ingest::outlines::{OutlineResult, Placed};
use univ_ingest::structures::{Structure, StructureError};
use univ_model::cosmology::SPEED_OF_LIGHT;
use univ_model::entry::{
    CatalogueEntry, NativePosition, Position, PositionUncertainty, Redshift, RedshiftKind,
};
use univ_model::grid::{Axis, CellUncertainty, Components, Field, Grid, GridGeometry, Samples};
use univ_model::ids::{Designation, SourceId};
use univ_model::position::{Epoch, Frame, Vec3, between, dot, lon_lat};
use univ_model::redshift::{CmbRedshift, DipoleModel};
use univ_model::units::Unit;
use univ_model::value::{KindOfValue, Quantity, Uncertainty};

/// The simulated void's centre, h⁻¹ Mpc, supergalactic.
pub const VOID: Vec3 = [35.0, 5.0, 5.0];
/// The radius, h⁻¹ Mpc, within which a cell centre lies in the simulated void.
pub const VOID_RADIUS: f64 = 15.0;

pub fn geometry() -> GridGeometry {
    GridGeometry {
        frame: Frame::Supergalactic,
        epoch: Epoch::J2000,
        unit: Unit::MegaparsecOverH,
        axes: [Axis::Z, Axis::X, Axis::Y],
        shape: [12, 16, 14],
        cell: 10.0,
        origin: [-75.0, -65.0, -55.0],
        assumed_cosmology: None,
    }
}

pub fn cells(g: &GridGeometry) -> impl Iterator<Item = [usize; 3]> + '_ {
    let n = Axis::XYZ.map(|a| g.cells_along(a));
    (0..n[0]).flat_map(move |x| (0..n[1]).flat_map(move |y| (0..n[2]).map(move |z| [x, y, z])))
}

pub fn in_void(p: Vec3) -> bool {
    let d = [p[0] - VOID[0], p[1] - VOID[1], p[2] - VOID[2]];
    dot(d, d).sqrt() <= VOID_RADIUS
}

/// The simulated density grid.
pub fn grid() -> Grid {
    let g = geometry();
    let mut samples = vec![0.0; g.cells()];
    for cell in cells(&g) {
        if in_void(g.centre(cell)) {
            samples[g.sample_index(cell).unwrap()] = -1.5;
        }
    }
    let density = Field {
        column: "d_mean".into(),
        field: "density".into(),
        unit: Unit::Dimensionless,
        kind: KindOfValue::Reconstructed,
        components: Components::Scalar,
        samples: Samples::Real(samples),
        uncertainty: CellUncertainty::NoneReported,
        labels: vec![],
    };
    Grid::new("simulated-density".into(), g, vec![density]).unwrap()
}

/// A simulated entry at the ICRS direction of the supergalactic `p`, with a CMB-frame redshift
/// putting it at |p| h⁻¹ Mpc when `redshift` is set.
pub fn entry(source: &str, n: usize, p: Vec3, redshift: bool) -> CatalogueEntry {
    let (ra, dec) = lon_lat(between(Frame::Supergalactic, Frame::Icrs).apply(p));
    let native = NativePosition {
        frame: Frame::Icrs,
        lon: ra,
        lat: dec,
        unit: Unit::Degree,
        epoch: Epoch::J2000,
    };
    let z = |v: f64| Quantity::new(v, Unit::Dimensionless, Uncertainty::NoneReported).unwrap();
    let r = dot(p, p).sqrt() * 100.0 / SPEED_OF_LIGHT;
    let redshifts = if redshift {
        vec![Redshift {
            kind: RedshiftKind::Spectroscopic,
            kind_of_value: KindOfValue::Measured,
            velocity: None,
            cmb: CmbRedshift {
                value: z(r),
                heliocentric: z(r),
                dipole: DipoleModel::Planck2018,
            },
        }]
    } else {
        vec![]
    };
    CatalogueEntry {
        designation: Designation::new(SourceId::new(source).unwrap(), n.to_string()).unwrap(),
        name: None,
        position: Some(Position::new(native, PositionUncertainty::NoneReported).unwrap()),
        distances: vec![],
        redshifts,
        velocity: None,
        parallax: None,
        proper_motion: None,
        magnitude: None,
        group_id: None,
        supergalactic_velocity: None,
        flags: BTreeMap::new(),
    }
}

/// A simulated catalogue: `outside` galaxies in each cell outside the void and `inside` in each
/// void cell, for cells whose centre `sees` and lies between `near` and 70 h⁻¹ Mpc; and one
/// sky-only entry per direction of a fine sky grid that `sees`.
pub fn catalogue(
    source: &str,
    outside: usize,
    inside: usize,
    near: f64,
    sees: impl Fn(Vec3) -> bool,
) -> Vec<CatalogueEntry> {
    let g = geometry();
    let mut out = Vec::new();
    for cell in cells(&g) {
        let p = g.centre(cell);
        let r = dot(p, p).sqrt();
        if sees(p) && (near..70.0).contains(&r) {
            let n = if in_void(p) { inside } else { outside };
            for _ in 0..n {
                out.push(entry(source, out.len(), p, true));
            }
        }
    }
    for i in 0..360 {
        for j in 0..180 {
            let (ra, dec) = (f64::from(i) + 0.5, f64::from(j) - 89.5);
            let icrs = univ_model::position::unit_vector(ra, dec);
            let p = between(Frame::Icrs, Frame::Supergalactic).apply(icrs);
            if sees(p) {
                out.push(entry(source, out.len(), p, false));
            }
        }
    }
    out
}

/// A cited coordinate of the simulated paper: `v` h⁻¹ Mpc written as km/s, as the Cosmicflows
/// papers publish.
pub fn coordinate(v: f64) -> String {
    format!(
        "{{ value: {}, unit: kilometre_per_second, kind: reconstructed, uncertainty: {{ form: none_reported }}, cite: {{ paper: simulated, table: \"1\", row: \"void\", column: \"SG\" }} }}",
        v * 100.0
    )
}

/// Supergalactic Cartesian coordinates at `at`, h⁻¹ Mpc.
pub fn coordinates(at: Vec3) -> String {
    format!(
        "{{ frame: supergalactic, x: {}, y: {}, z: {} }}",
        coordinate(at[0]),
        coordinate(at[1]),
        coordinate(at[2])
    )
}

/// A simulated structure file at the published position `at`, outlined on the simulated grid
/// with these catalogues; `extra` is added to the outline record.
pub fn yaml(at: Vec3, catalogues: &str, extra: &str) -> String {
    format!(
        "name: Simulated Void\nkind: void\nstatus: reconstructed\npapers:\n  - {{ id: simulated, reference: \"SIMULATED for this test: not a paper\", title: simulated, bibcode: simulated, read: \"not read: simulated\" }}\nposition: {}\noutline:\n  grid: simulated-density\n  threshold: {{ chosen: -1, why: \"the level of this simulated test\" }}\n  side: below\n{extra}  test:\n    catalogues: [{catalogues}]\n    rule: underdense_in_every_covering_catalogue\n    ratio: 0.85\n    min_expected: 30\n    why: \"the numbers of this simulated test\"\nnotes: \"SIMULATED.\"\n",
        coordinates(at)
    )
}

pub fn read(text: &str) -> Result<Structure, StructureError> {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("simulated-void.yaml");
    fs::write(&file, text).unwrap();
    Structure::read(&file)
}

pub fn structure(at: Vec3, catalogues: &str, extra: &str) -> Structure {
    read(&yaml(at, catalogues, extra)).unwrap()
}

pub use super::common::source_id as id;

/// The reason a structure was published as a point; `None` when its outline was admitted.
pub fn reason(result: &OutlineResult) -> Option<&str> {
    match &result.placed {
        Placed::Point { reason, .. } => Some(reason),
        Placed::Outline { .. } => None,
    }
}

pub fn everywhere(_: Vec3) -> bool {
    true
}
