//! Geometries of every kind meeting, or missing, regions of every form.

use std::f64::consts::FRAC_PI_4;

use super::tests::{MPC, cone, icrs_box, meets, solid, sphere};
use super::*;
use crate::geometry::{Aabb, Geometry, Member, Primitive, Shape};
use crate::healpix;

#[test]
fn a_sphere_meeting_a_box_only_at_its_corner_meets_it_and_one_just_short_does_not() {
    let corner_distance = 3.0_f64.sqrt();
    let unit_box = solid([0.0; 3], [1.0; 3], Rotation::IDENTITY, false);
    for (radius, expected) in [
        (corner_distance + 1e-9, true),
        (corner_distance - 1e-9, false),
    ] {
        let region = sphere([2.0; 3], radius);
        assert_eq!(
            region.meets(MPC, &unit_box),
            Some(expected),
            "radius {radius}"
        );
        let ball = Primitive::Sphere {
            centre: [2.0; 3],
            radius,
        };
        let boxed = icrs_box([-1.0; 3], [1.0; 3]);
        assert_eq!(
            meets(&boxed, Geometry::Primitive { primitive: ball }),
            expected
        );
        let tile = Aabb {
            min: [-1.0; 3],
            max: [1.0; 3],
        };
        assert_eq!(region.meets_box(&tile), expected);
    }
}

/// For each kind of line or point, a geometry that reaches the sphere of radius 1 about
/// (10, 0, 0) and one that does not.
fn lines_and_points() -> Vec<(&'static str, Geometry, Geometry)> {
    let tracer = |at| Member::Tracer {
        at,
        kind: "galaxy".into(),
    };
    let path = |points: Vec<Vec3>| Geometry::Path { points };
    let x = [1.0, 0.0, 0.0];
    vec![
        (
            "point",
            Geometry::Point {
                at: [10.5, 0.0, 0.0],
            },
            Geometry::Point {
                at: [12.0, 0.0, 0.0],
            },
        ),
        (
            "ray",
            Geometry::Ray {
                origin: [0.0; 3],
                direction: x,
                reach: 20.0,
            },
            Geometry::Ray {
                origin: [0.0; 3],
                direction: [0.0, 1.0, 0.0],
                reach: 20.0,
            },
        ),
        (
            "range",
            Geometry::Range {
                direction: x,
                near: 9.5,
                far: 9.6,
            },
            Geometry::Range {
                direction: x,
                near: 0.0,
                far: 8.0,
            },
        ),
        (
            "path",
            path(vec![[9.0, 5.0, 0.0], [11.0, -5.0, 0.0]]),
            path(vec![[0.0, 5.0, 0.0], [20.0, 5.0, 0.0]]),
        ),
        (
            "members",
            Geometry::Members {
                members: vec![tracer([0.0; 3]), tracer([10.0, 0.5, 0.0])],
            },
            Geometry::Members {
                members: vec![tracer([0.0; 3])],
            },
        ),
    ]
}

/// For each kind of solid or region, a geometry that reaches the sphere of radius 1 about
/// (10, 0, 0) and one that does not.
fn solids() -> Vec<(&'static str, Geometry, Geometry)> {
    let grid = |origin| Geometry::VolumeGrid {
        grid: "g".into(),
        origin,
        cell: 0.25,
        shape: [4; 3],
    };
    let cells = |cells| Geometry::GridRegion {
        grid: "g".into(),
        origin: [8.0, -1.0, -1.0],
        cell: 1.0,
        shape: [4; 3],
        cells,
    };
    let toward = |v: Vec3, near, far| Geometry::SkyRegion {
        order: 3,
        cells: vec![healpix::hash(3, v)],
        near,
        far,
    };
    let x = [1.0, 0.0, 0.0];
    vec![
        (
            "sphere",
            Geometry::Primitive {
                primitive: Primitive::Sphere {
                    centre: [12.0, 0.0, 0.0],
                    radius: 1.5,
                },
            },
            Geometry::Primitive {
                primitive: Primitive::Sphere {
                    centre: [12.0, 0.0, 0.0],
                    radius: 0.5,
                },
            },
        ),
        (
            "volume grid",
            grid([10.5, -0.5, -0.5]),
            grid([12.0, -0.5, -0.5]),
        ),
        (
            "grid region",
            cells(vec![[3, 3, 3], [2, 1, 1]]),
            cells(vec![[3, 3, 3], [0, 3, 3]]),
        ),
        (
            "sky region",
            toward(x, 9.5, 10.5),
            toward([-1.0, 0.0, 0.0], 9.5, 10.5),
        ),
        (
            "sky region beyond",
            toward(x, 9.5, 10.5),
            toward(x, 12.0, 13.0),
        ),
    ]
}

#[test]
fn each_geometry_kind_meets_a_region_it_reaches_and_misses_one_it_does_not() {
    let region = sphere([10.0, 0.0, 0.0], 1.0);
    let cases = lines_and_points().into_iter().chain(solids());
    for (name, reaches, misses) in cases {
        assert!(meets(&region, reaches), "{name} reaches");
        assert!(!meets(&region, misses), "{name} misses");
    }
    for ellipsoid in [false, true] {
        let reaches = solid(
            [13.0, 0.0, 0.0],
            [2.5, 0.1, 0.1],
            Rotation::IDENTITY,
            ellipsoid,
        );
        let misses = solid(
            [13.0, 0.0, 0.0],
            [1.5, 0.1, 0.1],
            Rotation::IDENTITY,
            ellipsoid,
        );
        assert_eq!(
            region.meets(MPC, &reaches),
            Some(true),
            "ellipsoid {ellipsoid}"
        );
        assert_eq!(
            region.meets(MPC, &misses),
            Some(false),
            "ellipsoid {ellipsoid}"
        );
    }
}

#[test]
fn a_cone_meets_solids_by_refinement_on_either_side_of_its_edge() {
    // At 10 Mpc a cone of 20° reaches 10 · tan 10° ≈ 1.763 Mpc from its axis.
    let region = cone(0.0, 0.0, 20.0, 5.0, 15.0);
    for ellipsoid in [false, true] {
        let reaches = solid(
            [10.0, 3.0, 0.0],
            [0.5, 1.4, 0.5],
            Rotation::IDENTITY,
            ellipsoid,
        );
        let misses = solid(
            [10.0, 3.0, 0.0],
            [0.5, 1.0, 0.5],
            Rotation::IDENTITY,
            ellipsoid,
        );
        assert_eq!(
            region.meets(MPC, &reaches),
            Some(true),
            "ellipsoid {ellipsoid}"
        );
        assert_eq!(
            region.meets(MPC, &misses),
            Some(false),
            "ellipsoid {ellipsoid}"
        );
    }
    let behind = Aabb {
        min: [-3.0, -1.0, -1.0],
        max: [-1.0, 1.0, 1.0],
    };
    assert!(!region.meets_box(&behind));
    let across = Aabb {
        min: [9.0, -0.1, -0.1],
        max: [9.5, 0.1, 0.1],
    };
    assert!(region.meets_box(&across));
}

#[test]
fn a_structure_whose_centre_lies_outside_still_meets() {
    let region = icrs_box([0.0; 3], [10.0; 3]);
    assert!(!region.contains([-3.0, 5.0, 5.0]));
    let wide = Primitive::Sphere {
        centre: [-3.0, 5.0, 5.0],
        radius: 4.0,
    };
    assert!(meets(&region, Geometry::Primitive { primitive: wide }));
    let narrow = Primitive::Sphere {
        centre: [-3.0, 5.0, 5.0],
        radius: 2.9,
    };
    assert!(!meets(&region, Geometry::Primitive { primitive: narrow }));
}

#[test]
fn a_box_region_meets_a_turned_box_by_separating_axes() {
    let region = icrs_box([0.0; 3], [1.0; 3]);
    let (c, s) = (FRAC_PI_4.cos(), FRAC_PI_4.sin());
    let turned = Rotation([[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]]);
    // Turned 45° about z, a cube of half-edge 0.5 reaches 0.5·√2 ≈ 0.707 along x.
    let near = solid([1.6, 0.5, 0.5], [0.5; 3], turned, false);
    let far = solid([1.75, 0.5, 0.5], [0.5; 3], turned, false);
    assert_eq!(region.meets(MPC, &near), Some(true));
    assert_eq!(region.meets(MPC, &far), Some(false));
}

#[test]
fn a_shape_is_placed_in_the_icrs_and_the_regions_unit_before_it_is_tested() {
    let to_icrs = between(Frame::Galactic, Frame::Icrs).rotation().unwrap();
    let centre = to_icrs.apply([0.008, 0.0, 0.0]);
    let region = sphere(centre, 1e-4);
    let point = Shape::new(
        Frame::Galactic,
        Unit::Kiloparsec,
        Geometry::Point {
            at: [8.0, 0.0, 0.0],
        },
    )
    .unwrap();
    assert_eq!(region.meets(MPC, &point), Some(true));
    let elsewhere = Shape::new(
        Frame::Icrs,
        Unit::Kiloparsec,
        Geometry::Point {
            at: [8.0, 0.0, 0.0],
        },
    )
    .unwrap();
    assert_eq!(region.meets(MPC, &elsewhere), Some(false));
}
