//! The Galaxy's tracers keep the distance their source publishes, with its uncertainty, method and
//! kind of value; a pulsating star keeps its pulsation type as published; a maser keeps its
//! parallax as measured and no distance (normalize-records, the Requirement on Galaxy tracers).

mod common;

use common::{entry, normalized_file, normalized_fixture};
use univ_ingest::source::LeftOutReason;
use univ_model::distance::{Distance, DistanceKind, Measure, Method};
use univ_model::units::Unit;
use univ_model::value::{KindOfValue, Uncertainty};

/// The entry `id`'s one distance, Euclidean and normalized in parsecs, the Galaxy tracers
/// domain's unit.
fn distance<'n>(n: &'n univ_ingest::normalize::Normalized, id: &str) -> &'n Distance {
    let e = entry(n, id);
    assert_eq!(e.distances.len(), 1, "{id}");
    let d = &e.distances[0];
    assert_eq!(d.measure, Measure::Euclidean, "{id}");
    assert_eq!(d.normalized.unit, Unit::Parsec, "{id}");
    d
}

#[test]
fn an_open_cluster_keeps_its_members_parallax_distance_and_percentiles_as_an_estimate() {
    // Recorded 2026-11-10: the first five open clusters of Hunt & Reffert 2023.
    let n = normalized_fixture("hunt-reffert-2023");
    assert_eq!(n.report.published, 5);
    let d = distance(&n, "ASCC_5");
    assert!((d.native.value - 1_740.983_206_90).abs() < 1e-6);
    assert_eq!(d.native.unit, Unit::Parsec);
    assert_eq!(d.kind, DistanceKind::Geometric);
    assert_eq!(d.method, Method::TrigonometricParallax);
    assert_eq!(d.kind_of_value, KindOfValue::Estimated);
    // The 16th and 84th percentiles, kept as the 68% interval they bound.
    let Uncertainty::CredibleInterval {
        minus,
        plus,
        probability,
    } = d.native.uncertainty
    else {
        panic!("{:?}", d.native.uncertainty)
    };
    assert!((d.native.value - minus - 1_721.687_506_05).abs() < 1e-6);
    assert!((d.native.value + plus - 1_760.716_320_64).abs() < 1e-6);
    assert!((probability - 0.68).abs() < 1e-12);
    assert_eq!(entry(&n, "ASCC_5").flags.get("globalPlx").unwrap(), "0");
}

#[test]
fn a_globular_cluster_keeps_its_combined_distance_and_error_as_an_estimate() {
    // Recorded 2026-11-10: five rows of the prepared orbits table; 47 Tuc (NGC 104) at
    // 4.52 ± 0.03 kpc.
    let n = normalized_fixture("baumgardt-vasiliev-2021");
    assert_eq!(n.report.published, 5);
    let d = distance(&n, "NGC_104");
    assert_eq!((d.native.value, d.native.unit), (4.52, Unit::Kiloparsec));
    assert_eq!(d.kind, DistanceKind::PrimaryIndicator);
    assert_eq!(
        d.method,
        Method::Mixed(vec![
            Method::TrigonometricParallax,
            Method::Other("kinematic distance".to_owned()),
            Method::Other("literature distances".to_owned()),
        ])
    );
    assert_eq!(d.kind_of_value, KindOfValue::Estimated);
    assert_eq!(d.native.uncertainty, Uncertainty::Symmetric { sigma: 0.03 });
    // Pal 4, in the far halo at 101 kpc, is kept as published.
    assert_eq!(distance(&n, "Pal_4").native.value, 101.39);
}

#[test]
fn a_cepheid_keeps_its_mid_infrared_distance_its_pulsation_mode_and_its_gaia_source_id() {
    // Recorded 2026-11-10: the first five Cepheids of Skowron et al. 2025; CG Cas at 3,174 ± 131
    // pc, pulsating in the fundamental mode.
    let n = normalized_fixture("skowron-2025-cepheids");
    assert_eq!(n.report.published, 5);
    let cg_cas = "429385923752386944";
    let d = distance(&n, cg_cas);
    assert_eq!((d.native.value, d.native.unit), (3_174.0, Unit::Parsec));
    assert_eq!(d.kind, DistanceKind::PrimaryIndicator);
    assert_eq!(d.method, Method::Cepheids);
    assert_eq!(d.kind_of_value, KindOfValue::Estimated);
    assert_eq!(
        d.native.uncertainty,
        Uncertainty::Symmetric { sigma: 131.0 }
    );
    let e = entry(&n, cg_cas);
    assert_eq!(e.flags.get("Mode").unwrap(), "F");
    assert_eq!(e.name.as_deref(), Some("CG_Cas"));
}

#[test]
fn an_rr_lyrae_star_keeps_its_photometric_distance_and_its_type_and_a_zero_distance_is_left_out() {
    // Recorded 2026-11-10: the first five stars of Li et al. 2023's first slice.
    let n = normalized_fixture("li-2023-rr-lyrae");
    assert_eq!(n.report.published, 5);
    let d = distance(&n, "507222753405440");
    assert_eq!((d.native.value, d.native.unit), (6.34, Unit::Kiloparsec));
    assert_eq!(d.kind, DistanceKind::PrimaryIndicator);
    assert_eq!(d.method, Method::RrLyrae);
    assert_eq!(d.kind_of_value, KindOfValue::Estimated);
    assert_eq!(d.native.uncertainty, Uncertainty::Symmetric { sigma: 0.67 });
    assert_eq!(
        entry(&n, "507222753405440").flags.get("Type").unwrap(),
        "RRab"
    );
    // Five of the 18 stars published at 0.00 kpc, toward the bulge: none stands.
    let zero = normalized_file("li-2023-rr-lyrae", "li-2023-rr-lyrae-zero.tsv");
    assert_eq!(zero.report.published, 0);
    assert_eq!(zero.report.left_out[&LeftOutReason::UnusableValue], 5);
}

#[test]
fn a_maser_keeps_its_parallax_as_measured_and_no_distance() {
    // Recorded 2026-11-10: the first five regions of Reid et al. 2019; NGC 6334 at 0.752 ± 0.069
    // mas.
    let n = normalized_fixture("reid-2019-masers");
    assert_eq!(n.report.published, 5);
    let e = entry(&n, "G351.44+00.65");
    assert!(e.distances.is_empty());
    let parallax = e.parallax.as_deref().unwrap();
    assert_eq!(parallax.kind, KindOfValue::Measured);
    assert_eq!(parallax.native.value, 0.752);
    assert_eq!(parallax.native.unit, Unit::Milliarcsecond);
    assert_eq!(
        parallax.native.uncertainty,
        Uncertainty::Symmetric { sigma: 0.069 }
    );
    assert_eq!(e.name.as_deref(), Some("NGC6334"));
    assert_eq!(e.flags.get("Arm").unwrap(), "CrN");
}
