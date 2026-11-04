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

use std::sync::LazyLock;

use common::workspace;
use univ_ingest::release::{
    Placing, centre as structure_centre, index_bounds, placements, structure_rows,
};
use univ_ingest::source::ScaleDomain;
use univ_ingest::structures::{
    Coordinates, Radial, Sides, Structure, StructureGeometry, read_all_named,
};
use univ_model::cosmology::SPEED_OF_LIGHT;
use univ_model::derive::Release;
use univ_model::geometry::Aabb;
use univ_model::position::{Frame, Vec3, between, dot, unit_vector};
use univ_model::redshift::{CmbCorrection, DipoleModel, RedshiftFrame};
use univ_model::units::Unit;
use univ_model::value::{Quantity, Uncertainty};

/// The repository's structure files, read once.
static STRUCTURES: LazyLock<Vec<(String, Structure)>> =
    LazyLock::new(|| read_all_named(&workspace().join("structures")).unwrap());

fn structure(id: &str) -> Structure {
    STRUCTURES.iter().find(|(i, _)| i == id).unwrap().1.clone()
}

/// The run's dipole, as the pipeline makes it.
fn dipole() -> CmbCorrection {
    CmbCorrection::new(DipoleModel::Planck2018)
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
    let b = index_bounds(&structure("bootes-void"), None, &release, &dipole()).unwrap();
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
    let kbc = index_bounds(&structure("kbc-void"), None, &release, &dipole()).unwrap();
    assert!(centre(&kbc).iter().all(|c| c.abs() < 1e-9));
    assert!((half(&kbc)[0] - 300.0 * 70.0 / h0).abs() < 1e-9);
}

#[test]
fn a_point_is_boxed_where_it_is_placed_and_an_unstated_equinox_widens_it() {
    let release = Release::planck_2018();
    // The Dipole Repeller: supergalactic velocities (11000, -6000, 10000) km/s.
    let b = index_bounds(&structure("dipole-repeller"), None, &release, &dipole()).unwrap();
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

    // The South Pole Wall's equinox is unstated: its box holds both readings.
    let wall = index_bounds(&structure("south-pole-wall"), None, &release, &dipole()).unwrap();
    let c = centre(&wall);
    let reach = dot(c, c).sqrt()
        * Frame::EquatorialEquinoxUnstated
            .systematic_uncertainty()
            .unwrap()
            .to_radians()
            .sin();
    assert!(half(&wall).iter().all(|h| (h - reach).abs() < 1e-9));
}

#[test]
fn a_sky_region_is_bounded_between_its_near_and_far_distances() {
    let release = Release::planck_2018();
    // The CfA2 Great Wall: RA 9h to 16.7h, Dec -8.5° to 42.5°, z from 0.01 to 0.05.
    let wall = structure("cfa2-great-wall");
    let Some(StructureGeometry::SkyRegion(region)) = &wall.geometry else {
        unreachable!()
    };
    let b = index_bounds(&wall, None, &release, &dipole()).unwrap();
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
            "south-pole-wall",
            "its published position gives no distance",
        ),
        ("big-ring", "its member list was not resolved"),
    ] {
        let mut s = structure(id);
        // The South Pole Wall is placed by its position alone; without its distance, nowhere.
        if let Some(Coordinates::Spherical { radial, .. }) = &mut s.position {
            *radial = None;
        }
        let refused = index_bounds(&s, None, &release, &dipole()).unwrap_err();
        assert_eq!(refused, reason, "{id}");
    }
}

#[test]
fn horologium_reticulum_is_placed_at_its_cited_mean_velocity() {
    // Fleenor et al. 2005: mean cz 19,900 km/s, heliocentric ("heliocentric corrections were
    // applied to all redshifts"), near α = 03h19m, δ = -50°02′. The velocity moves to the CMB
    // frame toward the centre with the run's dipole, as an entry's does, before it is placed:
    // 288.3 Mpc, not the 289.5 Mpc of the heliocentric cz.
    // Its position, as placed were it not drawn as its sky region.
    let release = Release::planck_2018();
    let mut s = structure("horologium-reticulum-supercluster");
    s.geometry = None;
    let Some(Coordinates::Spherical {
        frame,
        lon: Some(lon),
        lat,
        radial: Some(Radial::Velocity(v)),
        redshift_frame,
    }) = &s.position
    else {
        unreachable!()
    };
    assert_eq!(v.quantity.value, 19_900.0);
    assert_eq!(*redshift_frame, Some(RedshiftFrame::Heliocentric));
    let b = index_bounds(&s, None, &release, &dipole()).unwrap();
    let centre: Vec3 = std::array::from_fn(|k| b.min[k].midpoint(b.max[k]));
    let placed = dot(centre, centre).sqrt();
    let toward = between(*frame, Frame::Icrs).apply(unit_vector(lon.value(), lat.value()));
    let heliocentric_z = Quantity::new(
        19_900.0 / SPEED_OF_LIGHT,
        Unit::Dimensionless,
        Uncertainty::NoneReported,
    )
    .unwrap();
    let cmb = dipole()
        .apply(heliocentric_z, RedshiftFrame::Heliocentric, toward)
        .unwrap();
    let expected = release
        .cosmology()
        .comoving_distance(cmb.value.value)
        .unwrap();
    let heliocentric = release
        .cosmology()
        .comoving_distance(19_900.0 / SPEED_OF_LIGHT)
        .unwrap();
    assert!(
        (placed - expected).abs() < 1e-6 * expected,
        "{placed} {expected}"
    );
    assert!((placed - 288.3).abs() < 0.1, "{placed}");
    assert!(heliocentric - placed > 1.0, "{placed} {heliocentric}");
}

#[test]
fn a_sky_region_over_every_longitude_spans_all_360_degrees() {
    let release = Release::planck_2018();
    // The CfA2 Great Wall's region, its right ascension widened to the full circle [0, 360].
    let mut wall = structure("cfa2-great-wall");
    let Some(StructureGeometry::SkyRegion(region)) = &mut wall.geometry else {
        unreachable!()
    };
    let Sides::Ranges { lon, .. } = &mut region.sides else {
        unreachable!()
    };
    lon[0].quantity.value = 0.0;
    lon[1].quantity.value = 360.0;
    let to_icrs = between(region.frame, Frame::Icrs);
    let b = index_bounds(&wall, None, &release, &dipole()).unwrap();
    let far = release.cosmology().comoving_distance(0.05).unwrap();
    for ra in (0..360).step_by(15) {
        let p = to_icrs.apply(unit_vector(f64::from(ra), 0.0).map(|c| c * far));
        assert!(contains(&b, p), "RA {ra}: {b:?}");
    }
}

/// The boundary of the `nearby_galaxies` domain: the 11 Mpc of the Updated Nearby Galaxy
/// Catalog, its deepest source.
const NEARBY_GALAXIES_MPC: f64 = 11.0;

#[test]
fn each_structure_is_in_the_scale_domain_its_published_distance_falls_in() {
    let release = Release::planck_2018();
    let mut checked = 0;
    for (id, s) in read_all_named(&workspace().join("structures")).unwrap() {
        let distance = match structure_centre(&s, None, &release, &dipole()) {
            // About the observer: its published radius, the half side of its box.
            Ok(c) if dot(c, c) == 0.0 => {
                half(&index_bounds(&s, None, &release, &dipole()).unwrap())[0]
            }
            Ok(c) => dot(c, c).sqrt(),
            // A member list, or a structure published without a placeable position: its papers
            // put each far beyond the nearby galaxies (redshifts from 0.02 to 2.1).
            Err(_) => continue,
        };
        let expected = if distance > NEARBY_GALAXIES_MPC {
            ScaleDomain::CosmicWeb
        } else {
            ScaleDomain::NearbyGalaxies
        };
        assert_eq!(s.scale_domain, expected, "{id} at {distance} Mpc");
        checked += 1;
    }
    assert!(checked >= 15, "{checked}");
}

#[test]
fn a_structure_row_gives_its_scale_domain_and_centre_or_none_when_unplaced() {
    let release = Release::planck_2018();
    let named = read_all_named(&workspace().join("structures")).unwrap();
    let correction = dipole();
    let placing = Placing {
        release: &release,
        correction: &correction,
        dipole: "planck2018".to_owned(),
    };
    let placed = placements(&named, &[], &placing, &std::collections::BTreeMap::new()).unwrap();
    let (rows, _) = structure_rows(1, &placed).unwrap();
    let row = |id: &str| rows.iter().find(|r| r.id == id).unwrap();
    let bootes = row("bootes-void");
    assert_eq!(bootes.scale_domain, "cosmic_web");
    let at = [
        bootes.centre_x.unwrap(),
        bootes.centre_y.unwrap(),
        bootes.centre_z.unwrap(),
    ];
    let boxed =
        centre(&index_bounds(&structure("bootes-void"), None, &release, &dipole()).unwrap());
    assert!((0..3).all(|k| (at[k] - boxed[k]).abs() < 1e-9), "{at:?}");
    let unplaced = row("hercules-corona-borealis-great-wall");
    assert_eq!(unplaced.scale_domain, "cosmic_web");
    assert_eq!(
        (unplaced.centre_x, unplaced.centre_y, unplaced.centre_z),
        (None, None, None)
    );
    // A sky region's centre lies in its middle direction, halfway between near and far.
    let wall = row("cfa2-great-wall");
    let c = [
        wall.centre_x.unwrap(),
        wall.centre_y.unwrap(),
        wall.centre_z.unwrap(),
    ];
    let cosmology = release.cosmology();
    let mid = f64::midpoint(
        cosmology.comoving_distance(0.01).unwrap(),
        cosmology.comoving_distance(0.05).unwrap(),
    );
    assert!((dot(c, c).sqrt() - mid).abs() < 1e-6);
}
