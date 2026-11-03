//! Structures outlined from a volume grid (RQ-23, AC-58, AC-59), on a SIMULATED grid and
//! SIMULATED catalogues: nothing here is real data. The grid is 16 × 14 × 12 cells of
//! 10 h⁻¹ Mpc in supergalactic coordinates, stored Z slowest, then X, then Y, so that a reader
//! ignoring the axis order would find the wrong cells. Its density contrast is 0 except for a
//! simulated void of 19 cells at −1.5 around (35, 5, 5) h⁻¹ Mpc. Simulated galaxies sit at cell
//! centres with CMB-frame redshifts |r| × 100 / c, and simulated sky-only entries (a position,
//! no redshift) mark where each simulated survey looked.

#![expect(
    clippy::unwrap_used,
    reason = "a failed unwrap in a test helper is the test failing"
)]

use std::collections::BTreeMap;
use std::fs;

use univ_ingest::outlines::{Coverage, OutlineResult, Placed, place};
use univ_ingest::structures::{Structure, StructureError, Threshold, read_all};
use univ_model::cosmology::SPEED_OF_LIGHT;
use univ_model::entry::{
    CatalogueEntry, NativePosition, Position, PositionUncertainty, Redshift, RedshiftKind,
};
use univ_model::geometry::Geometry;
use univ_model::grid::{Axis, CellUncertainty, Components, Field, Grid, GridGeometry, Samples};
use univ_model::ids::{Designation, SourceId};
use univ_model::position::{Epoch, Frame, Vec3, between, dot, lon_lat};
use univ_model::redshift::{CmbRedshift, DipoleModel};
use univ_model::units::Unit;
use univ_model::value::{KindOfValue, Quantity, Uncertainty};

/// The simulated void's centre, h⁻¹ Mpc, supergalactic.
const VOID: Vec3 = [35.0, 5.0, 5.0];
/// The radius, h⁻¹ Mpc, within which a cell centre lies in the simulated void.
const VOID_RADIUS: f64 = 15.0;

fn geometry() -> GridGeometry {
    GridGeometry {
        frame: Frame::Supergalactic,
        epoch: Epoch::J2000,
        unit: Unit::MegaparsecOverH,
        axes: [Axis::Z, Axis::X, Axis::Y],
        shape: [12, 16, 14],
        cell: 10.0,
        origin: [-75.0, -65.0, -55.0],
        assumed_hubble_constant: None,
    }
}

fn cells(g: &GridGeometry) -> impl Iterator<Item = [usize; 3]> + '_ {
    let n = Axis::XYZ.map(|a| g.cells_along(a));
    (0..n[0]).flat_map(move |x| (0..n[1]).flat_map(move |y| (0..n[2]).map(move |z| [x, y, z])))
}

fn in_void(p: Vec3) -> bool {
    let d = [p[0] - VOID[0], p[1] - VOID[1], p[2] - VOID[2]];
    dot(d, d).sqrt() <= VOID_RADIUS
}

/// The simulated density grid.
fn grid() -> Grid {
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
fn entry(source: &str, n: usize, p: Vec3, redshift: bool) -> CatalogueEntry {
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
fn catalogue(
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
fn coordinate(v: f64) -> String {
    format!(
        "{{ value: {}, unit: kilometre_per_second, kind: reconstructed, uncertainty: {{ form: none_reported }}, cite: {{ paper: simulated, table: \"1\", row: \"void\", column: \"SG\" }} }}",
        v * 100.0
    )
}

/// Supergalactic Cartesian coordinates at `at`, h⁻¹ Mpc.
fn coordinates(at: Vec3) -> String {
    format!(
        "{{ frame: supergalactic, x: {}, y: {}, z: {} }}",
        coordinate(at[0]),
        coordinate(at[1]),
        coordinate(at[2])
    )
}

/// A simulated structure file at the published position `at`, outlined on the simulated grid
/// with these catalogues; `extra` is added to the outline record.
fn yaml(at: Vec3, catalogues: &str, extra: &str) -> String {
    format!(
        "name: Simulated Void\nkind: void\nstatus: reconstructed\npapers:\n  - {{ id: simulated, reference: \"SIMULATED for this test: not a paper\", title: simulated, bibcode: simulated, read: \"not read: simulated\" }}\nposition: {}\noutline:\n  grid: simulated-density\n  threshold: {{ chosen: -1, why: \"the level of this simulated test\" }}\n  side: below\n{extra}  test:\n    catalogues: [{catalogues}]\n    rule: underdense_in_every_covering_catalogue\n    ratio: 0.85\n    min_expected: 30\n    why: \"the numbers of this simulated test\"\nnotes: \"SIMULATED.\"\n",
        coordinates(at)
    )
}

fn read(text: &str) -> Result<Structure, StructureError> {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("simulated-void.yaml");
    fs::write(&file, text).unwrap();
    Structure::read(&file)
}

fn structure(at: Vec3, catalogues: &str, extra: &str) -> Structure {
    read(&yaml(at, catalogues, extra)).unwrap()
}

fn id(s: &str) -> SourceId {
    SourceId::new(s).unwrap()
}

/// The reason a structure was published as a point; `None` when its outline was admitted.
fn reason(result: &OutlineResult) -> Option<&str> {
    match &result.placed {
        Placed::Point { reason, .. } => Some(reason),
        Placed::Outline { .. } => None,
    }
}

fn everywhere(_: Vec3) -> bool {
    true
}

#[test]
fn an_outline_emptier_than_every_covering_catalogue_expects_is_admitted_as_a_reconstructed_grid_region()
 {
    // AC-58: the outline is the void's 19 cells, and each covering catalogue is compared with
    // the cells at the same distance outside it.
    let full = catalogue("simulated-full", 3, 0, 0.0, everywhere);
    // A survey of low supergalactic latitudes, which misses the void's five cells nearest the
    // supergalactic pole.
    let south = catalogue("simulated-south", 3, 1, 0.0, |p| {
        p[2] < 0.2 * dot(p, p).sqrt()
    });
    let s = structure(VOID, "simulated-full, simulated-south", "");
    let catalogues = [
        (id("simulated-full"), &full[..]),
        (id("simulated-south"), &south[..]),
    ];
    let result = place(&s, &grid(), &catalogues).unwrap();
    let Placed::Outline {
        frame,
        unit,
        kind,
        geometry: Geometry::GridRegion { cells, shape, .. },
    } = &result.placed
    else {
        panic!("not admitted: {result:?}")
    };
    assert_eq!(*frame, Frame::Supergalactic);
    assert_eq!(*unit, Unit::MegaparsecOverH);
    assert_eq!(*kind, KindOfValue::Reconstructed);
    assert_eq!(*shape, [16, 14, 12]);
    assert_eq!(cells.len(), 19);
    assert!(cells.contains(&[11, 7, 6]));
    let g = geometry();
    for c in cells {
        assert!(in_void(g.centre(c.map(|i| i as usize))));
    }
    let seed = result.seed.as_ref().unwrap();
    assert_eq!((seed.cell, seed.value), ([11, 7, 6], -1.5));
    let [full, south] = &result.catalogues[..] else {
        panic!("two checks")
    };
    assert_eq!(full.coverage, Coverage::Covers);
    assert_eq!(full.held, 0);
    assert!((full.expected - 3.0 * 19.0).abs() < 1e-9, "{full:?}");
    assert_eq!(full.passes, Some(true));
    assert_eq!(south.coverage, Coverage::Covers);
    assert_eq!((south.judged_cells, south.held), (14, 14), "{south:?}");
    assert!(south.ratio.unwrap() < 0.5, "{south:?}");
    // The outline, the grid, threshold, test, numbers and verdict are recorded with it.
    let yaml = serde_norway::to_string(&result).unwrap();
    for field in [
        "grid: simulated-density",
        "chosen: -1",
        "ratio:",
        "kind: grid_region",
    ] {
        assert!(yaml.contains(field), "{field} in {yaml}");
    }
}

#[test]
fn an_outline_a_covering_catalogue_finds_overdense_is_published_as_a_point_with_the_reason() {
    // AC-59: one catalogue finds the void empty, another finds it fuller than its surroundings.
    let empty = catalogue("simulated-empty", 3, 0, 0.0, everywhere);
    let full = catalogue("simulated-crowded", 2, 4, 0.0, everywhere);
    let s = structure(VOID, "simulated-empty, simulated-crowded", "");
    let catalogues = [
        (id("simulated-empty"), &empty[..]),
        (id("simulated-crowded"), &full[..]),
    ];
    let result = place(&s, &grid(), &catalogues).unwrap();
    let why = reason(&result).unwrap();
    assert!(
        why.contains("simulated-crowded holds 76 galaxies against 38.0 expected, a ratio of 2.00"),
        "{why}"
    );
    assert!(!why.contains("simulated-empty"), "{why}");
    let Placed::Point { position, .. } = &result.placed else {
        unreachable!()
    };
    assert_eq!(Some(position.as_ref()), s.position.as_ref());
    assert_eq!(result.cells, 19);
}

#[test]
fn an_outline_no_catalogue_covers_is_published_as_a_point_with_the_reason() {
    // A survey of the other hemisphere, a survey that starts beyond the void, and a catalogue
    // that was not given: none covers the outline.
    let north = catalogue("simulated-north", 3, 0, 0.0, |p| p[0] < 0.0);
    let deep = catalogue("simulated-deep", 3, 0, 55.0, everywhere);
    let s = structure(
        VOID,
        "simulated-north, simulated-deep, simulated-missing",
        "",
    );
    let catalogues = [
        (id("simulated-north"), &north[..]),
        (id("simulated-deep"), &deep[..]),
    ];
    let result = place(&s, &grid(), &catalogues).unwrap();
    assert_eq!(
        reason(&result).unwrap(),
        "no catalogue of the test covers the outline"
    );
    let coverage: Vec<&Coverage> = result.catalogues.iter().map(|c| &c.coverage).collect();
    assert!(
        matches!(coverage[0], Coverage::OutsideFootprint { covered_share } if *covered_share == 0.0)
    );
    assert!(matches!(coverage[1], Coverage::OutsideFootprint { .. }));
    assert!(result.catalogues[1].near > 50.0);
    assert_eq!(coverage[2], &Coverage::NotGiven);
}

#[test]
fn a_published_position_on_the_wrong_side_of_the_threshold_is_published_as_a_point() {
    let full = catalogue("simulated-full", 3, 0, 0.0, everywhere);
    let catalogues = [(id("simulated-full"), &full[..])];
    let beside = [-35.0, 5.0, 5.0];
    let result = place(
        &structure(beside, "simulated-full", ""),
        &grid(),
        &catalogues,
    )
    .unwrap();
    let why = reason(&result).unwrap();
    assert!(
        why.starts_with("the published position lies in cell [4, 7, 6]"),
        "{why}"
    );
    assert!(
        why.contains("density is 0.000, not below the threshold -1"),
        "{why}"
    );
    assert_eq!(result.catalogues.len(), 0);
    // Off the grid, it is a point too.
    let far = structure([500.0, 0.0, 0.0], "simulated-full", "");
    let result = place(&far, &grid(), &catalogues).unwrap();
    assert!(reason(&result).unwrap().contains("outside the grid"));
    // A named minimum the record states seeds the region instead.
    let seed = format!(
        "  seed:\n    name: simulated minimum\n    position: {}\n",
        coordinates(VOID)
    );
    let named = structure(beside, "simulated-full", &seed);
    let result = place(&named, &grid(), &catalogues).unwrap();
    assert!(
        matches!(result.placed, Placed::Outline { .. }),
        "{result:?}"
    );
    assert_eq!(result.seed.unwrap().name, "simulated minimum");
}

#[test]
fn a_threshold_chosen_by_this_pipeline_says_so_and_why_and_is_never_read_as_a_citation() {
    let s = structure(VOID, "simulated-full", "");
    let outline = s.outline.as_ref().unwrap();
    let Threshold::Chosen(choice) = &outline.threshold else {
        panic!("not chosen")
    };
    assert_eq!(choice.chosen, -1.0);
    assert!(
        !s.cited_values()
            .iter()
            .any(|(field, _)| field == "outline.threshold")
    );
    // A chosen threshold without its reason, or a bare number, is refused.
    let chosen = "threshold: { chosen: -1, why: \"the level of this simulated test\" }";
    let text = yaml(VOID, "simulated-full", "");
    for (broken, says) in [
        ("threshold: { chosen: -1 }", "missing field `why`"),
        ("threshold: { chosen: -1, why: \" \" }", "says why"),
        ("threshold: -1", "cites nothing"),
    ] {
        let e = read(&text.replace(chosen, broken)).unwrap_err();
        assert_eq!(e.field, "outline.threshold", "{e}");
        assert!(e.problem.contains(says), "{e}");
    }
    // The Local Void's threshold is this pipeline's choice, said to be one.
    let structures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../structures");
    let local = read_all(&structures)
        .unwrap()
        .into_iter()
        .find(|s| s.name == "Local Void")
        .unwrap();
    let outline = local.outline.unwrap();
    assert_eq!(outline.grid.as_str(), "cf4pp-density");
    let Threshold::Chosen(choice) = &outline.threshold else {
        panic!("the Local Void's threshold is not marked as chosen")
    };
    assert_eq!(choice.chosen, -1.0);
    assert!(
        choice.why.contains("not a published level"),
        "{}",
        choice.why
    );
}
