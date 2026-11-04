//! Each researched structure placed once (build-a-release): its row's centre and bounds in its
//! scale domain's unit, a grid outline in Mpc of another Hubble constant rescaled to the release
//! cosmology, and an admitted outline centred within its cells. The grid regions here are
//! SIMULATED shapes on the simulated grid of `void`, not data.
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;
mod void;

use std::collections::BTreeMap;

use common::workspace;
use univ_ingest::release::{
    OUTLINE_CENTRE, Placing, admitted, placements, structure_detail_rows, structure_rows,
};
use univ_ingest::source::ScaleDomain;
use univ_ingest::structures::{Structure, read_all_named};
use univ_model::cosmology::AssumedCosmology;
use univ_model::derive::Release;
use univ_model::geometry::{Aabb, Geometry};
use univ_model::position::{Frame, Vec3};
use univ_model::redshift::{CmbCorrection, DipoleModel};
use univ_model::units::Unit;
use void::{VOID, id, structure};

/// The simulated void, outlined on the simulated grid against one simulated catalogue.
fn simulated() -> Structure {
    structure(VOID, id("simulated-full").as_str(), "")
}

fn bootes() -> (String, Structure) {
    read_all_named(&workspace().join("structures"))
        .unwrap()
        .into_iter()
        .find(|(i, _)| i == "bootes-void")
        .unwrap()
}

/// A grid region on a grid of 5 × 5 × 1 cells of edge `cell` from the origin.
fn region(cell: f64, cells: Vec<[u32; 3]>) -> Geometry {
    Geometry::GridRegion {
        grid: "simulated-density".into(),
        origin: [0.0; 3],
        cell,
        shape: [5, 5, 1],
        cells,
    }
}

fn in_a_cell(p: Vec3, cell: f64, cells: &[[u32; 3]]) -> bool {
    cells.iter().any(|c| {
        (0..3).all(|k| {
            let low = f64::from(c[k]) * cell;
            (low..=low + cell).contains(&p[k])
        })
    })
}

#[test]
fn a_stars_structure_records_its_centre_and_bounds_in_parsecs() {
    let release = Release::planck_2018();
    let correction = CmbCorrection::new(DipoleModel::Planck2018);
    let placing = Placing {
        release: &release,
        correction: &correction,
        dipole: "planck2018".to_owned(),
    };
    let (id, mpc) = bootes();
    // SIMULATED: the Boötes void moved into the stars domain, which no structure file is in.
    let mut pc = mpc.clone();
    pc.scale_domain = ScaleDomain::Stars;
    let structures = vec![(id.clone(), mpc), (format!("{id}-in-pc"), pc)];
    let placed = placements(&structures, &[], &placing, &BTreeMap::new()).unwrap();
    let (rows, index) = structure_rows(1, &placed).unwrap();
    let centre = |n: usize| -> Vec3 {
        [rows[n].centre_x, rows[n].centre_y, rows[n].centre_z].map(Option::unwrap)
    };
    let bounds =
        |n: usize| -> Aabb { serde_json::from_str(rows[n].bounds.as_ref().unwrap()).unwrap() };
    let close = |a: f64, b: f64| (a - b).abs() <= 1e-9 * b.abs().max(1.0);
    for k in 0..3 {
        assert!(close(centre(1)[k], centre(0)[k] * 1e6), "{k}");
        assert!(close(bounds(1).min[k], bounds(0).min[k] * 1e6), "{k}");
        assert!(close(bounds(1).max[k], bounds(0).max[k] * 1e6), "{k}");
    }
    // The centre lies within the bounds, in the same unit.
    let (c, b) = (centre(1), bounds(1));
    assert!(
        (0..3).all(|k| b.min[k] <= c[k] && c[k] <= b.max[k]),
        "{c:?} {b:?}"
    );
    // The cell index keeps both boxes in Mpc.
    assert_eq!(index[0].1, index[1].1);
    // The shape is placed in parsecs too, from the same placement.
    let details = structure_detail_rows(1, &placed, &BTreeMap::new(), &release).unwrap();
    assert!(
        details[1]
            .shape
            .as_ref()
            .unwrap()
            .contains("\"unit\":\"parsec\"")
    );
}

#[test]
fn a_grid_in_mpc_for_another_hubble_constant_is_rescaled_to_the_release_cosmology() {
    let release = Release::planck_2018();
    let s = simulated();
    let cells = vec![[1, 1, 0]];
    let h50 = AssumedCosmology::new(50.0, None, None).unwrap();
    let rescaled = admitted(
        &s,
        (Frame::Supergalactic, Unit::Megaparsec, Some(h50)),
        &region(10.0, cells.clone()),
        &release,
    )
    .unwrap();
    let factor = 50.0 / release.cosmology().hubble_constant;
    let Geometry::GridRegion { cell, .. } = rescaled.placed.shape.geometry else {
        panic!("{:?}", rescaled.placed.shape.geometry)
    };
    assert!((cell - 10.0 * factor).abs() < 1e-12, "{cell}");
    let grid = rescaled.placed.derivation.grid.as_ref().unwrap();
    assert!((grid.factor - factor).abs() < 1e-15);
    assert!(
        rescaled
            .placed
            .derivation
            .distance
            .method
            .contains("H0 = 50"),
        "{}",
        rescaled.placed.derivation.distance.method
    );
    // A grid in Mpc that states no cosmology is taken as published.
    let as_published = admitted(
        &s,
        (Frame::Supergalactic, Unit::Megaparsec, None),
        &region(10.0, cells),
        &release,
    )
    .unwrap();
    assert_eq!(as_published.placed.derivation.grid.unwrap().factor, 1.0);
}

#[test]
fn an_l_shaped_outline_is_centred_within_its_cells_not_at_its_box_middle() {
    let release = Release::planck_2018();
    let s = simulated();
    // SIMULATED: an L of the cells with x < 2 or y < 2 of a 5 × 5 layer of 1 Mpc cells, in the
    // ICRS so the cells are not turned.
    let cells: Vec<[u32; 3]> = (0..5)
        .flat_map(|x| (0..5).map(move |y| [x, y, 0]))
        .filter(|c| c[0] < 2 || c[1] < 2)
        .collect();
    let placed = admitted(
        &s,
        (Frame::Icrs, Unit::Megaparsec, None),
        &region(1.0, cells.clone()),
        &release,
    )
    .unwrap();
    let middle = placed.bounds.centre();
    assert!(!in_a_cell(middle, 1.0, &cells), "{middle:?}");
    let c = placed.centre;
    assert!(in_a_cell(c, 1.0, &cells), "{c:?}");
    // The mean of the 16 cells' centres: (5 × 0.5 + 5 × 1.5 + 2 × (2.5 + 3.5 + 4.5)) / 16.
    assert!(
        (c[0] - 1.9375).abs() < 1e-12 && (c[1] - 1.9375).abs() < 1e-12,
        "{c:?}"
    );
    assert!((c[2] - 0.5).abs() < 1e-12);
    let grid = placed.placed.derivation.grid.as_ref().unwrap();
    assert_eq!(grid.centre, OUTLINE_CENTRE);
    // A ring's mean falls in its hole: the centre is then that of its cell nearest the mean.
    let ring: Vec<[u32; 3]> = (0..3)
        .flat_map(|x| (0..3).map(move |y| [x, y, 0]))
        .filter(|c| *c != [1, 1, 0])
        .collect();
    let placed = admitted(
        &s,
        (Frame::Icrs, Unit::Megaparsec, None),
        &region(1.0, ring.clone()),
        &release,
    )
    .unwrap();
    assert!(in_a_cell(placed.centre, 1.0, &ring), "{:?}", placed.centre);
}
