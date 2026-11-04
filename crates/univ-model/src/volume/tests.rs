use super::*;
use crate::geometry::{Geometry, Primitive, Shape};
use crate::position::{lon_lat, separation_degrees};

pub(super) const MPC: Unit = Unit::Megaparsec;

pub(super) fn volume(frame: Frame, unit: Unit, form: VolumeForm) -> Volume {
    Volume { frame, unit, form }
}

pub(super) fn sphere(centre: Vec3, radius: f64) -> Region {
    Region::Sphere { centre, radius }
}

pub(super) fn icrs_box(min: Vec3, max: Vec3) -> Region {
    volume(Frame::Icrs, MPC, VolumeForm::Box { min, max })
        .region_in(MPC)
        .unwrap()
}

pub(super) fn cone(lon: f64, lat: f64, opening_angle: f64, near: f64, far: f64) -> Region {
    let form = VolumeForm::Cone {
        lon,
        lat,
        opening_angle,
        near,
        far,
    };
    volume(Frame::Icrs, MPC, form).region_in(MPC).unwrap()
}

pub(super) fn shape(geometry: Geometry) -> Shape {
    Shape::new(Frame::Icrs, MPC, geometry).unwrap()
}

pub(super) fn solid(centre: Vec3, half: Vec3, orientation: Rotation, ellipsoid: bool) -> Shape {
    let primitive = if ellipsoid {
        Primitive::Ellipsoid {
            centre,
            semi_axes: half,
            orientation,
        }
    } else {
        Primitive::OrientedBox {
            centre,
            half_extents: half,
            orientation,
        }
    };
    shape(Geometry::Primitive { primitive })
}

pub(super) fn meets(region: &Region, geometry: Geometry) -> bool {
    region.meets(MPC, &shape(geometry)).unwrap()
}

/// A deterministic stream of numbers in [0, 1): a 64-bit linear congruential generator.
struct Stream(u64);

impl Stream {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        f64::from(u32::try_from(self.0 >> 40).unwrap()) / f64::from(1_u32 << 24)
    }

    /// A whole number below `n`.
    fn below(&mut self, n: u64) -> u64 {
        self.next();
        (self.0 >> 33) % n
    }

    fn unit(&mut self) -> Vec3 {
        let z = 2.0 * self.next() - 1.0;
        let phi = 2.0 * std::f64::consts::PI * self.next();
        let s = (1.0 - z * z).sqrt();
        [s * phi.cos(), s * phi.sin(), z]
    }
}

fn length(v: Vec3) -> f64 {
    v.iter().map(|c| c * c).sum::<f64>().sqrt()
}

/// Points of the region's surface, densely: where the nearest point to any outside point lies.
fn surface(region: &Region, stream: &mut Stream, n: usize) -> Vec<Vec3> {
    (0..n)
        .map(|_| match region {
            Region::Sphere { centre, radius } => {
                let u = stream.unit();
                std::array::from_fn(|i| centre[i] + radius * u[i])
            }
            Region::Box {
                to_frame,
                centre,
                half,
            } => {
                let face = usize::try_from(stream.below(6)).unwrap();
                let local: Vec3 = std::array::from_fn(|i| {
                    if i == face / 2 {
                        if face.is_multiple_of(2) {
                            -half[i]
                        } else {
                            half[i]
                        }
                    } else {
                        (2.0 * stream.next() - 1.0) * half[i]
                    }
                });
                to_frame
                    .transpose()
                    .apply(std::array::from_fn(|i| centre[i] + local[i]))
            }
            Region::Sector {
                axis,
                half_angle,
                near,
                far,
            } => loop {
                let u = stream.unit();
                let angle = separation_degrees(u, *axis).to_radians();
                if angle > *half_angle {
                    continue;
                }
                let r = match stream.below(3) {
                    0 => *near,
                    1 => *far,
                    _ => near + (far - near) * stream.next(),
                };
                // The conical side: the direction pushed out to the cone's edge.
                let u = if stream.next() < 0.4 {
                    let side = crate::position::cross(*axis, u);
                    let n = length(side);
                    if n < 1e-9 {
                        continue;
                    }
                    let side = side.map(|c| c / n);
                    let normal = crate::position::cross(side, *axis);
                    std::array::from_fn(|i| {
                        axis[i] * half_angle.cos() + normal[i] * half_angle.sin()
                    })
                } else {
                    u
                };
                break u.map(|c| c * r);
            },
        })
        .collect()
}

fn regions() -> Vec<(&'static str, Region)> {
    let galactic = VolumeForm::Box {
        min: [7.0, -1.0, -0.5],
        max: [9.0, 1.5, 0.5],
    };
    vec![
        ("sphere", sphere([3.0, -2.0, 1.0], 4.0)),
        (
            "galactic box",
            volume(Frame::Galactic, MPC, galactic)
                .region_in(MPC)
                .unwrap(),
        ),
        ("cone of 100°", cone(40.0, 25.0, 100.0, 2.0, 10.0)),
        ("cone of 180°", cone(200.0, -60.0, 180.0, 0.0, 6.0)),
        ("narrow cone", cone(0.0, 0.0, 6.0, 5.0, 9.0)),
    ]
}

#[test]
fn a_cone_wider_than_180_degrees_is_refused() {
    let opening = |opening_angle| {
        let form = VolumeForm::Cone {
            lon: 0.0,
            lat: 0.0,
            opening_angle,
            near: 0.0,
            far: 1.0,
        };
        volume(Frame::Icrs, MPC, form).check()
    };
    assert_eq!(opening(181.0)[0].member, "opening_angle");
    assert_eq!(opening(0.0)[0].member, "opening_angle");
    assert_eq!(opening(180.0), Vec::new());
}

#[test]
fn each_malformed_member_is_named() {
    let members = |v: Volume| v.check().into_iter().map(|e| e.member).collect::<Vec<_>>();
    let boxed = VolumeForm::Box {
        min: [0.0, f64::NAN, 0.0],
        max: [1.0; 3],
    };
    assert_eq!(
        members(volume(Frame::Fk4B1950, Unit::Degree, boxed)),
        ["frame", "unit", "min"]
    );
    let flat = VolumeForm::Box {
        min: [0.0; 3],
        max: [1.0, 0.0, 1.0],
    };
    assert_eq!(members(volume(Frame::Icrs, MPC, flat)), ["max"]);
    let ball = VolumeForm::Sphere {
        centre: [0.0; 3],
        radius: -1.0,
    };
    assert_eq!(
        members(volume(Frame::Supergalactic, Unit::MegaparsecOverH, ball)),
        ["unit", "radius"]
    );
    let cone = VolumeForm::Cone {
        lon: f64::INFINITY,
        lat: 91.0,
        opening_angle: 30.0,
        near: -1.0,
        far: 5.0,
    };
    assert_eq!(
        members(volume(Frame::Galactic, Unit::Parsec, cone)),
        ["lon", "lat", "near"]
    );
    let shallow = VolumeForm::Cone {
        lon: 0.0,
        lat: 0.0,
        opening_angle: 30.0,
        near: 5.0,
        far: 5.0,
    };
    assert_eq!(
        members(volume(Frame::Icrs, Unit::LightYear, shallow)),
        ["far"]
    );
}

#[test]
fn distance_is_the_exact_distance_to_the_region_and_zero_within_it() {
    let mut stream = Stream(7);
    for (name, region) in regions() {
        let points = surface(&region, &mut stream, 20_000);
        for p in &points {
            assert!(
                region.distance(*p) < 1e-9,
                "{name}: {p:?} lies {} outside",
                region.distance(*p)
            );
        }
        let bounds = region.bounds();
        let extent = region.extent();
        for _ in 0..200 {
            let q: Vec3 = std::array::from_fn(|i| {
                let span = bounds.max[i] - bounds.min[i] + extent;
                bounds.min[i] - extent / 2.0 + span * stream.next()
            });
            let d = region.distance(q);
            let nearest = points
                .iter()
                .map(|p| length(std::array::from_fn(|i| p[i] - q[i])))
                .fold(f64::INFINITY, f64::min);
            assert!(
                d <= nearest + 1e-9,
                "{name}: {d} exceeds a point at {nearest}"
            );
            if d > 0.0 {
                assert!(
                    nearest - d < 0.03 * extent,
                    "{name}: {d} far below the nearest {nearest}"
                );
            }
        }
    }
}

#[test]
fn the_bounds_hold_the_region_and_touch_it_on_every_side() {
    let mut stream = Stream(11);
    for (name, region) in regions() {
        let points = surface(&region, &mut stream, 20_000);
        let bounds = region.bounds();
        let slack = 0.02 * region.extent();
        for i in 0..3 {
            let lowest = points.iter().map(|p| p[i]).fold(f64::INFINITY, f64::min);
            let highest = points
                .iter()
                .map(|p| p[i])
                .fold(f64::NEG_INFINITY, f64::max);
            assert!(
                lowest >= bounds.min[i] - 1e-9 && highest <= bounds.max[i] + 1e-9,
                "{name} axis {i}"
            );
            assert!(
                lowest - bounds.min[i] < slack && bounds.max[i] - highest < slack,
                "{name} axis {i}"
            );
        }
    }
}

#[test]
fn a_galactic_box_lies_toward_the_galactic_centre_in_the_icrs() {
    let form = VolumeForm::Box {
        min: [7.5, -0.5, -0.5],
        max: [8.5, 0.5, 0.5],
    };
    let boxed = volume(Frame::Galactic, Unit::Kiloparsec, form);
    let centre = between(Frame::Galactic, Frame::Icrs)
        .rotation()
        .unwrap()
        .apply([8.0, 0.0, 0.0]);
    let (ra, dec) = lon_lat(centre);
    // The galactic centre's direction (ESA 1997): RA 266.405°, Dec −28.936°.
    assert!(
        (ra - 266.405).abs() < 1e-3 && (dec + 28.936).abs() < 1e-3,
        "{ra} {dec}"
    );
    let in_kpc = boxed.region_in(Unit::Kiloparsec).unwrap();
    assert!(in_kpc.contains(centre));
    assert!(!in_kpc.contains([8.0, 0.0, 0.0]));
    let in_parsecs = boxed.region_in(Unit::Parsec).unwrap();
    assert!(in_parsecs.contains(centre.map(|c| c * 1_000.0)));
}
