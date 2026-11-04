//! univ-model's `HEALPix` nested scheme, which the Worker uses to test sky regions, agrees with
//! `cdshealpix` (Pineau, CDS), which ingest builds sky regions with.

use std::f64::consts::PI;

use univ_model::healpix;
use univ_model::position::{dot, unit_vector};

/// Directions on the boundaries of cells: the poles, a corner of four cells on the equator,
/// and the edge of the polar caps at z = 2/3.
const ON_BOUNDARIES: [(f64, f64); 4] = [
    (0.0, 90.0),
    (0.0, -90.0),
    (0.0, 0.0),
    (90.0, 41.810_314_895_778_6),
];

/// Directions spread over the sphere, off every cell boundary.
fn directions() -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    // A Fibonacci lattice: a quasi-uniform set with no lattice of HEALPix's own.
    let golden = f64::midpoint(1.0, 5.0_f64.sqrt());
    for i in 0..4_000_u32 {
        let k = f64::from(i) + 0.5;
        let lat = (1.0 - 2.0 * k / 4_000.0).asin().to_degrees();
        let lon = (360.0 * k / golden).rem_euclid(360.0);
        out.push((lon, lat));
    }
    out
}

#[test]
fn each_direction_falls_in_the_cell_cdshealpix_gives() {
    for order in [0_u8, 1, 4, 9, 14, 20, 29] {
        for (lon, lat) in directions() {
            let expected = cdshealpix::nested::hash(order, lon.to_radians(), lat.to_radians());
            let got = healpix::hash(order, unit_vector(lon, lat));
            assert_eq!(got, expected, "order {order}, ({lon}, {lat})");
        }
        // On a boundary either cell that touches the point is right; the two break ties apart.
        for (lon, lat) in ON_BOUNDARIES {
            let v = unit_vector(lon, lat);
            let centre = healpix::centre(order, healpix::hash(order, v));
            let angle = dot(centre, v).clamp(-1.0, 1.0).acos();
            assert!(
                angle <= healpix::radius_bound(order),
                "order {order}, ({lon}, {lat})"
            );
        }
    }
}

#[test]
fn each_cells_centre_matches_and_its_vertices_lie_within_the_radius_bound() {
    for order in [0_u8, 2, 6, 11, 18] {
        let cells = 12_u64 << (2 * order);
        let step = (cells / 1_500).max(1);
        let bound = healpix::radius_bound(order);
        let mut widest: f64 = 0.0;
        for hash in (0..cells).step_by(usize::try_from(step).unwrap()) {
            let centre = healpix::centre(order, hash);
            let (lon, lat) = cdshealpix::nested::center(order, hash);
            let theirs = unit_vector(lon.to_degrees(), lat.to_degrees());
            assert!(
                dot(centre, theirs) > (1e-12_f64).cos() - 1e-15,
                "order {order} cell {hash}"
            );
            for (lon, lat) in cdshealpix::nested::vertices(order, hash) {
                let vertex = unit_vector(lon.to_degrees(), lat.to_degrees());
                let angle = dot(centre, vertex).clamp(-1.0, 1.0).acos();
                widest = widest.max(angle / bound);
                assert!(
                    angle <= bound,
                    "order {order} cell {hash}: {angle} > {bound}"
                );
            }
        }
        assert!(
            widest > 0.5,
            "the bound at order {order} is not loose beyond use: {widest}"
        );
    }
    assert!(healpix::radius_bound(0) < PI / 2.0);
}
