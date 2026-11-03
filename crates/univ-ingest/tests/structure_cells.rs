//! The boxes the repository's researched structures enter the cell index with, under the
//! release cosmology (Planck 2018): a sphere about its placed centre with its radius rescaled
//! from the paper's Hubble constant, a point, a sky region bounded between its near and far
//! distances rather than by the sphere of its far distance, and the reasons of those not
//! indexed.
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;

use common::workspace;
use univ_ingest::release::index_bounds;
use univ_ingest::structures::{Structure, StructureGeometry, read_all_named};
use univ_model::cosmology::SPEED_OF_LIGHT;
use univ_model::derive::Release;
use univ_model::geometry::Aabb;
use univ_model::position::{Frame, Vec3, between, dot, unit_vector};

fn structure(id: &str) -> Structure {
    read_all_named(&workspace().join("structures"))
        .unwrap()
        .into_iter()
        .find(|(i, _)| i == id)
        .unwrap()
        .1
}

fn centre(b: &Aabb) -> Vec3 {
    std::array::from_fn(|k| f64::midpoint(b.min[k], b.max[k]))
}

fn half(b: &Aabb) -> Vec3 {
    std::array::from_fn(|k| (b.max[k] - b.min[k]) / 2.0)
}

fn contains(b: &Aabb, p: Vec3) -> bool {
    (0..3).all(|k| b.min[k] <= p[k] && p[k] <= b.max[k])
}

#[test]
fn a_sphere_is_boxed_about_its_placed_centre_with_its_rescaled_radius() {
    let release = Release::planck_2018();
    let h0 = release.cosmology().hubble_constant;
    // Bootes Void: centred at cz = 15,500 km/s (FK4 B1950), radius 63 Mpc for H0 = 50.
    let b = index_bounds(&structure("bootes-void"), None, &release).unwrap();
    let distance = release
        .cosmology()
        .comoving_distance(15_500.0 / SPEED_OF_LIGHT)
        .unwrap();
    let c = centre(&b);
    assert!((dot(c, c).sqrt() - distance).abs() < 1e-6, "{c:?}");
    for h in half(&b) {
        assert!((h - 63.0 * 50.0 / h0).abs() < 1e-9, "{h}");
    }
    let published = between(Frame::Fk4B1950, Frame::Icrs).apply(unit_vector(222.5, 46.0));
    let toward = dot(c, published) / dot(c, c).sqrt();
    assert!(toward > (0.01_f64).to_radians().cos(), "{toward}");

    // The KBC void is about the observer: its box is centred on it, 300 Mpc for H0 = 70.
    let kbc = index_bounds(&structure("kbc-void"), None, &release).unwrap();
    assert!(centre(&kbc).iter().all(|c| c.abs() < 1e-9));
    assert!((half(&kbc)[0] - 300.0 * 70.0 / h0).abs() < 1e-9);
}

#[test]
fn a_point_is_boxed_where_it_is_placed_and_an_unstated_equinox_widens_it() {
    let release = Release::planck_2018();
    // The Dipole Repeller: supergalactic velocities (11000, -6000, 10000) km/s.
    let b = index_bounds(&structure("dipole-repeller"), None, &release).unwrap();
    assert_eq!(
        b.min, b.max,
        "a point in a frame that is a rotation of the ICRS"
    );
    let speed = (11_000.0_f64.powi(2) + 6_000.0_f64.powi(2) + 10_000.0_f64.powi(2)).sqrt();
    let distance = release
        .cosmology()
        .comoving_distance(speed / SPEED_OF_LIGHT)
        .unwrap();
    assert!((dot(b.min, b.min).sqrt() - distance).abs() < 1e-6);

    // The Shapley Supercluster's equinox is unstated: its box holds both readings.
    let shapley = index_bounds(&structure("shapley-supercluster"), None, &release).unwrap();
    let c = centre(&shapley);
    let reach = dot(c, c).sqrt()
        * Frame::EquatorialEquinoxUnstated
            .systematic_uncertainty()
            .unwrap()
            .to_radians()
            .sin();
    assert!(half(&shapley).iter().all(|h| (h - reach).abs() < 1e-9));
}

#[test]
fn a_sky_region_is_bounded_between_its_near_and_far_distances() {
    let release = Release::planck_2018();
    // The CfA2 Great Wall: RA 9h to 16.7h, Dec -8.5° to 42.5°, z from 0.01 to 0.05.
    let wall = structure("cfa2-great-wall");
    let Some(StructureGeometry::SkyRegion(region)) = &wall.geometry else {
        unreachable!()
    };
    let b = index_bounds(&wall, None, &release).unwrap();
    let cosmology = release.cosmology();
    let (near, far) = (
        cosmology.comoving_distance(0.01).unwrap(),
        cosmology.comoving_distance(0.05).unwrap(),
    );
    let to_icrs = between(region.frame, Frame::Icrs);
    let steps = 60;
    for i in 0..=steps {
        for j in 0..=steps {
            let ra = 135.0 + 115.5 * f64::from(i) / f64::from(steps);
            let dec = -8.5 + 51.0 * f64::from(j) / f64::from(steps);
            for r in [near, f64::midpoint(near, far), far] {
                let p = to_icrs.apply(unit_vector(ra, dec).map(|c| c * r));
                assert!(contains(&b, p), "{ra} {dec} {r}: {b:?}");
            }
        }
    }
    // Far tighter than the box of the sphere of its far distance.
    let volume = |b: &Aabb| (0..3).map(|k| b.max[k] - b.min[k]).product::<f64>();
    let sphere = (2.0 * far).powi(3);
    assert!(volume(&b) < 0.25 * sphere, "{} of {sphere}", volume(&b));
}

#[test]
fn a_structure_that_cannot_be_placed_is_not_indexed_with_the_reason() {
    let release = Release::planck_2018();
    for (id, reason) in [
        (
            "hercules-corona-borealis-great-wall",
            "it publishes no position or geometry",
        ),
        (
            "horologium-reticulum-supercluster",
            "its published position gives no distance",
        ),
        ("big-ring", "its member list was not resolved"),
    ] {
        let refused = index_bounds(&structure(id), None, &release).unwrap_err();
        assert_eq!(refused, reason, "{id}");
    }
}
