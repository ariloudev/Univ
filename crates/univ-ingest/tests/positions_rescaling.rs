//! Lengths published for another Hubble constant, rescaled to the release cosmology's (AC-32,
//! AC-33), and structure positions published in FK4 B1950 or without their equinox.
#![expect(
    clippy::unwrap_used,
    clippy::float_cmp,
    reason = "a failed unwrap is the test failing, and values passed through unchanged compare exactly"
)]

mod common;

use std::path::PathBuf;

use common::near;
use serde::Deserialize;
use univ_ingest::grids::{Window, read_cf4_basins};
use univ_ingest::structures::{Coordinates, Radial, Structure, StructureGeometry, read_all};
use univ_model::cosmology::{AssumedCosmology, Cosmology};
use univ_model::derive::{DerivedPosition, Direction, Release, Transformation};
use univ_model::distance::{Distance, DistanceKind, Measure, Method};
use univ_model::position::{Epoch, Frame, dot, lon_lat};
use univ_model::units::Unit;
use univ_model::value::{KindOfValue, Quantity, Uncertainty};

fn structures() -> Vec<Structure> {
    read_all(&common::structures_dir()).unwrap()
}

fn structure(name: &str) -> Structure {
    structures().into_iter().find(|s| s.name == name).unwrap()
}

#[derive(Deserialize)]
struct Sidecar {
    crop: Crop,
}

#[derive(Deserialize)]
struct Crop {
    start: [usize; 3],
}

fn grids() -> PathBuf {
    common::grids_dir()
}

#[test]
fn an_h_inverse_mpc_grid_distance_is_rescaled_and_kept_beside_its_native_value() {
    // AC-32. The Laniakea attractor in the CF4 basins grid, at [−62, −8, 39] h⁻¹ Mpc
    // supergalactic (Dupuy & Courtois 2023, A&A 678, A176, §3), a reconstruction for
    // H0 = 74.6 km/s/Mpc.
    let text = std::fs::read_to_string(grids().join("cf4-basins-crop.yaml")).unwrap();
    let start = serde_norway::from_str::<Sidecar>(&text).unwrap().crop.start;
    let grid = read_cf4_basins(
        &grids().join("cf4-basins-crop.zip"),
        Window::Cutout { start },
    )
    .unwrap();
    let g = &grid.geometry;
    assert_eq!(g.unit, Unit::MegaparsecOverH);
    assert_eq!(g.assumed_cosmology, Some(AssumedCosmology::hubble(74.6)));
    let cell = g.cell_at([-62.0, -8.0, 39.0]).unwrap();
    let centre = g.centre(cell);
    let r = dot(centre, centre).sqrt();
    let release = Release::planck_2018();
    let native = Quantity::new(r, Unit::MegaparsecOverH, Uncertainty::NoneReported).unwrap();
    let how = (
        DistanceKind::SecondaryIndicator,
        Method::Other("CF4 basins".into()),
    );
    let distance = Distance::new(
        native,
        None,
        Unit::Megaparsec,
        KindOfValue::Reconstructed,
        how,
        Measure::Unspecified,
        None,
    )
    .unwrap();
    assert_eq!(
        distance.assumed_cosmology,
        Some(AssumedCosmology::HUBBLE_SCALED)
    );
    let radial = release.radial_from_distance(&distance, None).unwrap();
    // h⁻¹ Mpc is Mpc for H0 = 100: r × 100 / 67.66.
    assert!(near(radial.distance().value, r * 100.0 / 67.66, 1e-12));
    assert_eq!(radial.native(), &native);
    assert_eq!(radial.assumed(), Some(&AssumedCosmology::HUBBLE_SCALED));
    // The same length as the source's Mpc, for its H0 = 74.6, rescales to the same value.
    let theirs = release
        .hubble_rescaling(g.assumed_cosmology.unwrap())
        .unwrap();
    let in_their_mpc = Quantity::new(r / 0.746, Unit::Megaparsec, Uncertainty::NoneReported);
    let rescaled = theirs.apply(&in_their_mpc.unwrap()).unwrap();
    assert!(near(
        rescaled.rescaled.value,
        radial.distance().value,
        1e-12
    ));
    assert_eq!(rescaled.assumed.hubble_constant, 74.6);
    assert!(near(rescaled.native.value, r / 0.746, 1e-15));
    // Placed in its own frame, the cell keeps its direction and grows by 100 / 67.66.
    let (lon, lat) = lon_lat(centre);
    let direction = Direction::new(Frame::Supergalactic, lon, lat, Epoch::J2000).unwrap();
    let placed = DerivedPosition::new(direction, radial);
    let sg = placed.cartesian(Frame::Supergalactic).unwrap();
    for (ours, theirs) in sg.iter().zip(centre) {
        assert!((ours - theirs * 100.0 / 67.66).abs() < 1e-9, "{sg:?}");
    }
}

/// A structure value as a quantity.
fn value(s: &Structure, name: &str) -> (Quantity, AssumedCosmology) {
    let cited = &s.values[name];
    let assumed = cited.assumes.as_ref().unwrap().cosmology.unwrap();
    (cited.quantity, assumed)
}

#[test]
fn a_size_is_rescaled_with_the_same_factor_as_its_structures_distance() {
    // AC-33. Tully 1987: the Pisces-Cetus complex is centred ~200 h75^-1 Mpc away and 300
    // h75^-1 Mpc long, both for H0 = 75.
    let release = Release::planck_2018();
    let pisces = structure("Pisces-Cetus Supercluster Complex");
    let (distance, assumed) = value(&pisces, "main_plane_distance");
    let (size, same) = value(&pisces, "long_dimension");
    assert_eq!((assumed, same), (AssumedCosmology::hubble(75.0), assumed));
    let rescaling = release.hubble_rescaling(assumed).unwrap();
    let (d, l) = (
        rescaling.apply(&distance).unwrap(),
        rescaling.apply(&size).unwrap(),
    );
    assert!(near(d.rescaled.value, 200.0 * 75.0 / 67.66, 1e-12));
    assert!(near(l.rescaled.value, 300.0 * 75.0 / 67.66, 1e-12));
    assert!(near(l.rescaled.value / d.rescaled.value, 1.5, 1e-12));
    assert_eq!((d.native, l.native), (distance, size));
    // Lopez, Clowes & Williger 2022: the Giant Arc at z = 0.802 is 1000 Mpc across for H0 = 70, ΩM = 0.27,
    // ΩΛ = 0.73. Its size scales with the ratio of comoving distances at its redshift, so it
    // subtends the same angle in either cosmology.
    let arc = structure("Giant Arc");
    let z = arc.values["central_redshift"].quantity.value;
    let (extent, wmap) = value(&arc, "extent");
    assert_eq!(
        (wmap.hubble_constant, wmap.omega_matter, wmap.omega_lambda),
        (70.0, Some(0.27), Some(0.73))
    );
    let at_z = release.rescaling_at(wmap, z).unwrap();
    let ours = release.table().distance(z).unwrap();
    let theirs = Cosmology::new("wmap", 70.0, 0.27)
        .unwrap()
        .comoving_distance(z)
        .unwrap();
    let rescaled = at_z.apply(&extent).unwrap().rescaled.value;
    assert!(near(rescaled / ours, 1000.0 / theirs, 1e-12));
    assert!(matches!(
        at_z.step().transformation,
        Transformation::ComovingRescaled { redshift, .. } if redshift == z
    ));
    // Here the ratio of Hubble constants alone would be more than 1 % off.
    assert!((at_z.factor / (70.0 / 67.66) - 1.0).abs() > 0.01);
}

#[test]
fn every_structure_cosmology_reads_as_a_typed_assumed_cosmology() {
    let mut typed = 0;
    for s in structures() {
        for (name, cited) in &s.values {
            let Some(assumes) = cited.assumes.as_deref() else {
                continue;
            };
            match assumes.cosmology {
                Some(c) => {
                    assert!(c.check().is_ok(), "{}: {name}", s.name);
                    typed += 1;
                }
                None => assert!(assumes.stated.starts_with("not stated"), "{}", s.name),
            }
        }
    }
    assert!(typed >= 20, "{typed}");
    // "flat ΛCDM, ΩΛ = 0.7, ΩM = 0.3, h = 0.7": h is H0 / 100.
    let ring = structure("Giant GRB Ring");
    let grb = ring
        .values
        .values()
        .find_map(|c| c.assumes.as_ref()?.cosmology);
    let grb = grb.unwrap();
    assert!(near(grb.hubble_constant, 70.0, 1e-12));
    assert_eq!((grb.omega_matter, grb.omega_lambda), (Some(0.3), Some(0.7)));
}

#[test]
fn a_b1950_structure_position_is_moved_to_the_icrs_with_its_e_terms() {
    // Kirshner et al. 1987: the Boötes void is centred at α = 14h50m, δ = +46° (B1950), cz =
    // 15,500 km/s. pyerfa 2.0.1.5 fk45z then astropy 8.0.2 FK5 → ICRS give α = 222.945839°,
    // δ = 45.795533°.
    let bootes = structure("Bootes Void");
    let Some(Coordinates::Spherical {
        frame,
        lon: Some(lon),
        lat,
        radial: Some(Radial::Velocity(cz)),
    }) = &bootes.position
    else {
        panic!("{:?}", bootes.position)
    };
    assert_eq!(*frame, Frame::Fk4B1950);
    let direction = Direction::new(*frame, lon.value(), lat.value(), Epoch(1950.0)).unwrap();
    let icrs = direction.icrs();
    assert!((icrs.ra - 222.945_838_880).abs() < 1e-8, "{}", icrs.ra);
    assert!((icrs.dec - 45.795_532_937).abs() < 1e-8, "{}", icrs.dec);
    assert_eq!(
        (icrs.epoch, direction.systematic_degrees()),
        (Epoch::J2000, None)
    );
    let z = univ_model::redshift::CmbRedshift {
        value: Quantity::new(
            cz.value() / 299_792.458,
            Unit::Dimensionless,
            Uncertainty::NoneReported,
        )
        .unwrap(),
        heliocentric: Quantity::new(0.0, Unit::Dimensionless, Uncertainty::NoneReported).unwrap(),
        dipole: univ_model::redshift::DipoleModel::Planck2018,
    };
    let radial = Release::planck_2018().radial_from_redshift(&z).unwrap();
    let placed = DerivedPosition::new(direction, radial);
    assert!(placed.chain().iter().any(|s| matches!(
        s.transformation,
        Transformation::ToIcrs {
            from: Frame::Fk4B1950
        }
    ) && s.citation.contains("Standish 1982")));
}

#[test]
fn an_unstated_equinox_is_placed_as_j2000_and_carries_its_systematic_error() {
    // Geller & Huchra 1989 give the CfA2 Great Wall's box in equatorial coordinates without
    // naming the equinox.
    let wall = structure("CfA2 Great Wall");
    let Some(StructureGeometry::SkyRegion(region)) = &wall.geometry else {
        panic!("a sky region")
    };
    assert_eq!(region.frame, Frame::EquatorialEquinoxUnstated);
    let (lon, lat) = (region.lon[0].value(), region.lat[0].value());
    let direction = Direction::new(region.frame, lon, lat, Epoch::J2000).unwrap();
    let as_j2000 = Direction::new(Frame::Fk5J2000, lon, lat, Epoch::J2000).unwrap();
    assert_eq!(direction.icrs(), as_j2000.icrs());
    let error = direction.systematic_degrees().unwrap();
    assert!((error - 0.6986).abs() < 1e-3, "{error}");
    let r0 = Distance::new(
        Quantity::new(100.0, Unit::Megaparsec, Uncertainty::NoneReported).unwrap(),
        None,
        Unit::Megaparsec,
        KindOfValue::Estimated,
        (DistanceKind::SpectroscopicRedshift, Method::Redshift),
        Measure::Comoving,
        None,
    )
    .unwrap();
    let radial = Release::planck_2018()
        .radial_from_distance(&r0, None)
        .unwrap();
    let placed = DerivedPosition::new(direction, radial);
    assert!(placed.chain().iter().any(|s| matches!(
        s.transformation,
        Transformation::EquinoxAssumedJ2000 { systematic_degrees } if systematic_degrees == error
    )));
}
