//! The Cosmicflows-4 group velocities read by their definition, and the group of the Milky Way
//! left without a distance, on real recorded rows of the groups and galaxies tables; and the
//! Milky Way's distance, the Galactic centre's that GRAVITY measured, beside UNGC's own.

mod common;
mod simulated;

use common::{entry, normalized_file};
use simulated::{matching_rules, ranking_rules};
use univ_ingest::matching::match_entries;
use univ_ingest::ranking::rank_all;
use univ_model::cosmology::{Cosmology, SPEED_OF_LIGHT};
use univ_model::derive::Release;
use univ_model::distance::{DistanceKind, Measure};
use univ_model::units::Unit;
use univ_model::value::{KindOfValue, Uncertainty};

#[test]
fn a_group_velocity_beyond_13000_km_s_reads_as_a_comoving_distance_and_meets_its_galaxys() {
    let groups = normalized_file("cosmicflows-4", "cosmicflows-4-velocities.tsv");
    let galaxies = normalized_file("cosmicflows-4-galaxies", "cosmicflows-4-galaxies-local.tsv");
    // 1PGC 1365066, a group of one galaxy: V3k = 28573 km/s is 67.8 x the comoving distance of
    // its Tempel et al. 2017 group, 421.434575 Mpc (Omega_m = 0.308). Read as cz it gave z =
    // 0.09531, 2.7% short of its galaxy's 0.09793 (Vcmb 29359, its SDSS redshift).
    for (pgc, v3k) in [("1365066", 28_573.0), ("837279", 15_659.0)] {
        let group = &entry(&groups, pgc).redshifts[0];
        let galaxy = &entry(&galaxies, pgc).redshifts[0];
        assert_eq!(entry(&groups, pgc).velocity.unwrap().native.value, v3k);
        let (zg, zh) = (group.cmb.value.value, galaxy.cmb.value.value);
        assert!((zg / zh - 1.0).abs() < 0.005, "{pgc}: {zg} against {zh}");
        assert!((v3k / SPEED_OF_LIGHT / zh - 1.0).abs() > 0.01, "{pgc}");
        let tempel = Cosmology::new("Tempel et al. 2017", 67.8, 0.308).unwrap();
        let d = tempel.comoving_distance(zg).unwrap();
        assert!((67.8 * d - v3k).abs() < 1e-6 * v3k, "{pgc}: {d}");
    }
    let z = entry(&groups, "1365066").redshifts[0].cmb.value.value;
    assert!((z - 0.097_55).abs() < 0.000_05, "{z}");
}

#[test]
fn a_group_velocity_read_as_a_comoving_distance_is_inferred_and_one_read_as_cz_measured() {
    // The definition infers the reading from 13,000 km/s up, which neither the ReadMe nor the
    // paper states; below, V3k is a cz from the members' spectra.
    let groups = normalized_file("cosmicflows-4", "cosmicflows-4-velocities.tsv");
    for (pgc, kind) in [
        ("1365066", KindOfValue::Inferred),
        ("837279", KindOfValue::Inferred),
        ("5064336", KindOfValue::Measured),
    ] {
        let z = &entry(&groups, pgc).redshifts[0];
        assert_eq!(z.kind_of_value, kind, "{pgc}");
    }
}

#[test]
fn the_milky_ways_group_keeps_no_distance() {
    // 1PGC 5064336: DMzp 20.369, the satellites' average, toward the Galactic centre.
    let groups = normalized_file("cosmicflows-4", "cosmicflows-4-velocities.tsv");
    let g = entry(&groups, "5064336");
    assert!(g.distances.is_empty(), "{:?}", g.distances);
    assert!(g.position.is_some());
    let fix = groups.report.fixes.last().unwrap();
    assert!(
        fix.change
            .starts_with("On that row DMzp and e_DMzp are dropped")
    );
    assert_eq!(fix.entries_changed, 1);
    // Another group keeps its modulus.
    assert_eq!(entry(&groups, "1365066").distances.len(), 1);
}

#[test]
fn the_milky_way_is_placed_at_the_galactic_centres_distance_beside_ungcs_own() {
    let n = normalized_file(
        "updated-nearby-galaxies",
        "updated-nearby-galaxies-duplicates.tsv",
    );
    let mw = entry(&n, "Milky Way");
    // UNGC's own 0.01 Mpc is kept as published, and GRAVITY's R0 stands beside it, cited.
    let [ungc, gravity] = &mw.distances[..] else {
        panic!("{:?}", mw.distances)
    };
    assert_eq!(
        (ungc.native.value, ungc.native.unit),
        (0.01, Unit::Megaparsec)
    );
    assert_eq!(
        (gravity.native.value, gravity.native.unit),
        (8_178.0, Unit::Parsec)
    );
    assert_eq!(
        gravity.native.uncertainty,
        Uncertainty::Symmetric { sigma: 25.6 }
    );
    assert_eq!(gravity.kind, DistanceKind::Geometric);
    assert_eq!(gravity.measure, Measure::Euclidean);
    let flag = mw.flags.get("distance_source").unwrap();
    assert!(
        flag.contains("A&A 625, L10") && flag.contains("8178"),
        "{flag}"
    );
    // Its direction stays Sgr A*'s, and the ranking prefers the measured R0.
    let at = mw.position.as_ref().unwrap().icrs;
    assert!((at.ra - 266.416_667).abs() < 1e-5 && (at.dec + 29.007_778).abs() < 1e-5);
    let mut m = match_entries(&matching_rules(), vec![mw.clone()]);
    rank_all(&mut m.objects, &ranking_rules(), &Release::planck_2018());
    let p = m.objects[0].preferred_distance.as_ref().unwrap();
    assert!((p.distance_mpc - 0.008_178).abs() < 1e-9, "{p:?}");
}
