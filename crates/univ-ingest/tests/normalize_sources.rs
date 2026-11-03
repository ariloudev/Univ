//! Every tabular source normalizes its recorded answer into catalogue entries. For each source
//! two rows are checked by hand: each expected number below is copied from the fixture's text,
//! or worked from it as the comment beside it says.
#![expect(
    clippy::unwrap_used,
    clippy::float_cmp,
    reason = "a failed unwrap is the test failing, and values read from text compare exactly"
)]

mod common;

use common::{entry, near, normalized_fixture};
use univ_ingest::normalize::normalize;
use univ_model::distance::{DistanceKind, Method};
use univ_model::entry::{CatalogueEntry, PositionUncertainty, RedshiftKind};
use univ_model::position::{Epoch, Frame, cross, unit_vector};
use univ_model::redshift::{CmbCorrection, DipoleModel};
use univ_model::units::Unit;
use univ_model::value::{KindOfValue, Uncertainty, Value};

const C: f64 = 299_792.458;
const MAS: f64 = 1.0 / 3_600_000.0;

/// The symmetric error of a value as published; NaN, which equals nothing, for any other form.
fn sigma(value: &Value) -> f64 {
    match value.native.uncertainty {
        Uncertainty::Symmetric { sigma } => sigma,
        _ => f64::NAN,
    }
}

/// The ICRS position of an entry, with its native frame and epoch.
fn at(e: &CatalogueEntry) -> (f64, f64, Frame, Epoch) {
    let p = e.position.as_ref().unwrap();
    (p.icrs.ra, p.icrs.dec, p.native.frame, p.icrs.epoch)
}

/// An FK5 J2000 position is kept as published and moved to ICRS by tens of milliarcseconds.
fn fk5(e: &CatalogueEntry, ra: f64, dec: f64) {
    let p = e.position.as_ref().unwrap();
    assert_eq!((p.native.lon, p.native.lat), (ra, dec));
    assert_eq!(
        (p.native.frame, p.icrs.epoch),
        (Frame::Fk5J2000, Epoch::J2000)
    );
    let (a, b) = (unit_vector(ra, dec), p.icrs.unit_vector());
    let moved = cross(a, b)
        .iter()
        .map(|c| c * c)
        .sum::<f64>()
        .sqrt()
        .to_degrees();
    assert!(moved > 0.0 && moved < 0.1 / 3_600.0, "moved {moved}°");
}

#[test]
fn twomrs_rows_keep_cz_and_derive_heliocentric_and_cmb_redshifts() {
    let n = normalized_fixture("twomrs");
    assert_eq!((n.report.published, n.report.left_out_total()), (5, 0));
    // Row 1: 010.6847100 +41.2687500 00424433+4116074 -300 4 0.797 1991RC3.9.C...0000d.
    let m31 = entry(&n, "00424433+4116074");
    fk5(m31, 10.684_71, 41.268_75);
    let v = m31.velocity.unwrap();
    assert_eq!((v.native.value, sigma(&v)), (-300.0, 4.0));
    let r = &m31.redshifts[0];
    assert_eq!(r.velocity.unwrap().value, -300.0);
    assert_eq!(r.cmb.heliocentric.value, -300.0 / C);
    // Worked in Python from the Hipparcos galactic matrix and the Planck 2018 dipole:
    // cos θ = -0.76785, so z_cmb = (1 + z)γ(1 + β cos θ) - 1 = -0.0019461909.
    assert!((r.cmb.value.value + 0.001_946_190_9).abs() < 1e-9);
    assert_eq!(m31.magnitude.as_ref().unwrap().value.native.value, 0.797);
    assert_eq!(m31.flags["r_cz"], "1991RC3.9.C...0000d");
    // Row 2: 011.8880600 -25.2888000 00473313-2517196 243 2 3.815 2004AJ....128...16K.
    let ngc253 = entry(&n, "00473313-2517196");
    fk5(ngc253, 11.888_06, -25.288_8);
    assert_eq!(sigma(&ngc253.velocity.unwrap()), 2.0);
    assert_eq!(ngc253.redshifts[0].cmb.heliocentric.value, 243.0 / C);
}

#[test]
fn sixdfgs_rows_keep_cz_their_quality_and_magnitude() {
    let n = normalized_fixture("sixdfgs-dr3");
    assert_eq!(n.report.published, 5);
    // Row 1: 000.0107500 -23.3680278 g0000026-232205 19966 45 4 16.64.
    let g = entry(&n, "g0000026-232205");
    fk5(g, 0.010_75, -23.368_027_8);
    assert_eq!(g.velocity.unwrap().native.value, 19_966.0);
    // Worked in Python as for 2MRS: cos θ = -0.84315, z_cmb = 0.0654908544.
    assert!((g.redshifts[0].cmb.value.value - 0.065_490_854_4).abs() < 1e-9);
    assert_eq!(g.flags["q_cz"], "4");
    // Row 5: 000.0184583 -0.0833333 g0000044-000500 5400 100 9 15.87.
    let g = entry(&n, "g0000044-000500");
    assert_eq!(sigma(&g.velocity.unwrap()), 100.0);
    assert_eq!(g.flags["q_cz"], "9");
    assert_eq!(g.magnitude.as_ref().unwrap().band, "bJmag");
}

#[test]
fn twompp_rows_keep_their_reference_and_no_group() {
    let n = normalized_fixture("twompp-catalogue");
    assert_eq!(n.report.published, 5);
    // Row 1: 025.3962083 -89.3344722 01413509-8920041 2429 5 11.09 (no GID) 20096dF...C...0000J 0.
    let g = entry(&n, "01413509-8920041");
    fk5(g, 25.396_208_3, -89.334_472_2);
    assert_eq!(g.redshifts[0].velocity.unwrap().value, 2_429.0);
    assert_eq!(g.redshifts[0].kind_of_value, KindOfValue::Measured);
    assert_eq!(
        (g.group_id.as_deref(), g.flags["Cln"].as_str()),
        (None, "0")
    );
    // Row 4: 235.8802917 -87.6785278 15433127-8740427 12651 300 11.96.
    let g = entry(&n, "15433127-8740427");
    assert_eq!(sigma(&g.velocity.unwrap()), 300.0);
    assert_eq!(g.magnitude.as_ref().unwrap().value.normalized.value, 11.96);
}

#[test]
fn cosmicflows_groups_keep_their_modulus_and_read_sg_coordinates_as_km_s() {
    let n = normalized_fixture("cosmicflows-4");
    assert_eq!(n.report.published, 5);
    // Row 1: 000.036000 -06.373900 12 34.995 0.410 6532 1848 -6271 1312.
    let g = entry(&n, "12");
    fk5(g, 0.036, -6.3739);
    let d = &g.distances[0];
    assert_eq!(d.native.value, 34.995);
    assert_eq!(d.native.uncertainty, Uncertainty::Symmetric { sigma: 0.41 });
    // 10^(34.995/5 + 1) pc = 99.7700 Mpc.
    assert!(near(d.normalized.value, 99.770_006_382, 1e-9));
    assert_eq!(d.normalized.unit, Unit::Megaparsec);
    assert_eq!(d.kind, DistanceKind::SecondaryIndicator);
    assert!(matches!(&d.method, Method::Mixed(m) if m.len() == 8));
    assert_eq!(d.hubble_constant, None);
    let sg = g.supergalactic_velocity.unwrap();
    assert_eq!(
        (sg.x.native.value, sg.y.native.value, sg.z.native.value),
        (1_848.0, -6_271.0, 1_312.0)
    );
    assert_eq!(sg.x.native.unit, Unit::KilometrePerSecond);
    // Row 4: 000.093800 +39.495500 35 29.730 0.100 335 518 -260 215.
    let g = entry(&n, "35");
    // 10^(29.73/5 + 1) pc = 8.8308 Mpc.
    assert!(near(g.distances[0].normalized.value, 8.830_799_6, 1e-7));
    assert_eq!(g.redshifts[0].cmb.heliocentric.value, 335.0 / C);
}

#[test]
fn updated_nearby_galaxies_take_each_rows_method_and_leave_out_hubble_distances() {
    let n = normalized_fixture("updated-nearby-galaxies");
    // Row 5, AGC748778, has f_Dist h': a Hubble-flow distance, left out.
    assert_eq!(n.report.published, 4);
    assert_eq!(n.report.left_out.len(), 1);
    // Row 1: 000.093750 +39.495556 UGC12894 8.47 TF 335 16.80.
    let g = entry(&n, "UGC12894");
    fk5(g, 0.093_75, 39.495_556);
    let d = &g.distances[0];
    assert_eq!((d.native.value, d.native.unit), (8.47, Unit::Megaparsec));
    assert_eq!(d.native.uncertainty, Uncertainty::NoneReported);
    assert_eq!(
        (d.kind, &d.method),
        (DistanceKind::SecondaryIndicator, &Method::TullyFisher)
    );
    assert_eq!(g.velocity.unwrap().native.value, 335.0);
    // Row 2: 000.492083 -15.461111 WLM 0.97 TRGB -122 11.03.
    let wlm = entry(&n, "WLM");
    let d = &wlm.distances[0];
    assert_eq!(d.normalized.value, 0.97);
    assert_eq!(
        (d.kind, &d.method),
        (
            DistanceKind::PrimaryIndicator,
            &Method::TipOfTheRedGiantBranch
        )
    );
    assert_eq!(wlm.flags["f_Dist"], "TRGB");
}

#[test]
fn mcconnachie_keeps_each_modulus_beside_the_distance_derived_from_it() {
    let n = normalized_fixture("mcconnachie-2012");
    assert_eq!(n.report.published, 5);
    // Row 1, The Galaxy: a position (Sgr A*) and nothing else.
    let mw = entry(&n, "The Galaxy");
    fk5(mw, 266.416_667, -29.007_778);
    assert!(mw.distances.is_empty() && mw.velocity.is_none());
    // Row 2: Canis Major 14.29 0.30 0.30 b 7 1 1 87.0 4.0 c -0.1.
    let cma = entry(&n, "Canis Major");
    let d = &cma.distances[0];
    assert_eq!(d.modulus().unwrap().value, 14.29);
    // 10^(14.29/5 + 1) pc = 7211.07 pc; the bounds at 13.99 and 14.59 are 930.49 pc below
    // and 1068.35 pc above it.
    assert!(near(d.normalized.value, 7_211.074_79e-6, 1e-8));
    let Uncertainty::Asymmetric { minus, plus } = d.normalized.uncertainty else {
        panic!("asymmetric")
    };
    assert!(near(minus, 930.491_2e-6, 1e-6) && near(plus, 1_068.346_8e-6, 1e-6));
    let published = d.also_published.unwrap();
    assert_eq!((published.value, published.unit), (7.0, Unit::Kiloparsec));
    assert_eq!(d.kind, DistanceKind::PrimaryIndicator);
    assert_eq!(d.kind_of_value, KindOfValue::Measured);
    assert_eq!(
        (cma.flags["u_(m-M)"].as_str(), cma.flags["f_HRV"].as_str()),
        ("b", "c")
    );
    // Row 5: Ursa Major II 17.50 0.30 0.30 32 4 4 -116.5 1.9 13.3.
    let uma2 = entry(&n, "Ursa Major II");
    let v = uma2.velocity.unwrap();
    assert_eq!((v.native.value, sigma(&v)), (-116.5, 1.9));
    assert_eq!(uma2.magnitude.as_ref().unwrap().value.native.value, 13.3);
}

#[test]
fn gaia_rows_keep_icrs_at_2016_their_errors_parallax_and_proper_motion() {
    let n = normalized_fixture("gaia-dr3-bright");
    assert_eq!(n.report.published, 5);
    // Row 1: 139396191545402112, ra 47.36721742552817, dec 37.30128833573518, ra_error
    // 0.02968816, parallax 6.8907168392017235 ± 0.03628909, radial_velocity -3.105404.
    let s = entry(&n, "139396191545402112");
    assert_eq!(
        at(s),
        (
            47.367_217_425_528_17,
            37.301_288_335_735_18,
            Frame::Icrs,
            Epoch(2016.0)
        )
    );
    let PositionUncertainty::Published(errors) = &s.position.as_ref().unwrap().uncertainty else {
        panic!("published")
    };
    assert_eq!(errors.ra.native.value, 0.029_688_16);
    assert!(near(errors.ra.normalized.value, 0.029_688_16 * MAS, 1e-12));
    let plx = s.parallax.unwrap();
    assert_eq!(
        (plx.native.value, plx.native.unit),
        (6.890_716_839_201_723_5, Unit::Milliarcsecond)
    );
    assert_eq!(plx.normalized.unit, Unit::Degree);
    assert_eq!(s.velocity.unwrap().native.value, -3.105_404);
    assert_eq!(
        s.proper_motion.unwrap().ra.native.value,
        -42.073_116_442_981_636
    );
    assert_eq!(s.redshifts, []);
    // Row 4: 196955487820819968, parallax 0.455010455134798 ± 0.020734983, G 9.768232.
    let s = entry(&n, "196955487820819968");
    assert_eq!(sigma(&s.parallax.unwrap()), 0.020_734_983);
    assert_eq!(s.magnitude.as_ref().unwrap().value.native.value, 9.768_232);
}

#[test]
fn gcns_rows_keep_their_distance_posterior_as_a_credible_interval() {
    let n = normalized_fixture("gcns");
    assert_eq!(n.report.published, 5);
    // Row 1: 530861741656374272 ... RV -26.204 ± 0.4477, Dist50 0.09726, Dist16 0.09716,
    // Dist84 0.09736 kpc: 97.26 pc, 0.10 pc below and above.
    let s = entry(&n, "530861741656374272");
    assert_eq!(
        at(s),
        (0.005_636_9, 70.887_363_9, Frame::Icrs, Epoch(2016.0))
    );
    let d = &s.distances[0];
    assert!(near(d.normalized.value, 97.26, 1e-12));
    assert_eq!(d.normalized.unit, Unit::Parsec);
    let Uncertainty::CredibleInterval {
        minus,
        plus,
        probability,
    } = d.normalized.uncertainty
    else {
        panic!("credible interval")
    };
    assert!((minus - 0.1).abs() < 1e-9 && (plus - 0.1).abs() < 1e-9 && probability == 0.68);
    assert_eq!(
        (d.kind, d.kind_of_value),
        (DistanceKind::Geometric, KindOfValue::Estimated)
    );
    assert_eq!(sigma(&s.velocity.unwrap()), 0.4477);
    // Row 2: 530685166960018816, no RV, Dist50 0.05581 kpc.
    let s = entry(&n, "530685166960018816");
    assert!(s.velocity.is_none());
    assert!(near(s.distances[0].normalized.value, 55.81, 1e-12));
}

#[test]
fn hipparcos_rows_keep_icrs_at_1991_25_and_their_solution_type() {
    let n = normalized_fixture("hipparcos-2007");
    assert_eq!(n.report.published, 5);
    // Row 1: HIP 1, 000.00091185 +01.08901332, e_RArad 1.29, Plx 4.55 ± 1.33, Sn 5.
    let s = entry(&n, "1");
    assert_eq!(
        at(s),
        (0.000_911_85, 1.089_013_32, Frame::Icrs, Epoch(1991.25))
    );
    assert_eq!(
        (
            s.parallax.unwrap().native.value,
            sigma(&s.parallax.unwrap())
        ),
        (4.55, 1.33)
    );
    assert_eq!(s.flags["Sn"], "5");
    // Row 2: HIP 2, Plx 20.85 ± 1.13, pmRA 182.88, Sn 75.
    let s = entry(&n, "2");
    assert_eq!(s.proper_motion.unwrap().ra.native.value, 182.88);
    assert_eq!(s.flags["Sn"], "75");
}

#[test]
fn bailer_jones_rows_keep_both_distances_as_credible_intervals_without_a_position() {
    let n = normalized_fixture("bailer-jones-2021");
    assert_eq!(n.report.published, 5);
    // Row 1: 139396191545402112, geo 144.4075 (143.72238, 145.18251), photogeo 144.3897
    // (143.61337, 145.27467), flag 10033.
    let s = entry(&n, "139396191545402112");
    assert!(s.position.is_none());
    let [geo, photogeo] = &s.distances[..] else {
        panic!("two distances")
    };
    assert_eq!(
        (geo.native.value, geo.kind),
        (144.4075, DistanceKind::Geometric)
    );
    let Uncertainty::CredibleInterval { minus, plus, .. } = geo.native.uncertainty else {
        panic!("interval")
    };
    assert!((minus - 0.685_12).abs() < 1e-9 && (plus - 0.775_01).abs() < 1e-9);
    assert_eq!(
        (photogeo.native.value, &photogeo.method),
        (144.3897, &Method::BayesianPhotogeometric)
    );
    assert_eq!(s.flags["flag"], "10033");
    // Row 4: 196955487820819968, geo 2001.1107 (1926.5125, 2079.8193).
    let s = entry(&n, "196955487820819968");
    let Uncertainty::CredibleInterval { minus, .. } = s.distances[0].native.uncertainty else {
        panic!("interval")
    };
    assert!((minus - 74.5982).abs() < 1e-9);
}

#[test]
fn the_hipparcos_crossmatch_names_each_gaia_source_by_its_hip_number() {
    let n = normalized_fixture("gaia-dr3-hipparcos2");
    assert_eq!(n.report.published, 5);
    // Row 1: 4612085076726173440, 10768, 0.0012945472, 8.
    let s = entry(&n, "4612085076726173440");
    assert_eq!(s.name.as_deref(), Some("10768"));
    assert_eq!(s.flags["angular_distance"], "0.0012945472");
    // Row 3: 5769897140331951744, 71404, 0.0036472648, 72.
    let s = entry(&n, "5769897140331951744");
    assert_eq!(
        (s.name.as_deref(), s.flags["xm_flag"].as_str()),
        (Some("71404"), "72")
    );
}

#[test]
fn sdss_and_boss_rows_keep_z_and_derive_the_cmb_redshift() {
    let n = normalized_fixture("sdss-dr17-galaxies");
    assert_eq!(n.report.published, 5);
    // Row 1: 1147416020424615936, 184.85921, 56.159283, z 0.05197566 ± 7.782429E-06.
    let g = entry(&n, "1147416020424615936");
    assert_eq!(at(g), (184.859_21, 56.159_283, Frame::Icrs, Epoch::J2000));
    let r = &g.redshifts[0];
    assert_eq!(r.cmb.heliocentric.value, 0.051_975_66);
    assert_eq!(
        r.cmb.heliocentric.uncertainty,
        Uncertainty::Symmetric {
            sigma: 7.782_429e-6
        }
    );
    assert_eq!((r.kind, r.velocity), (RedshiftKind::Spectroscopic, None));
    // Worked in Python: cos θ = 0.42846, z_cmb = 0.0525324715.
    assert!((r.cmb.value.value - 0.052_532_471_5).abs() < 1e-9);
    // Row 3: 7700273112517203968, z 0.3186874.
    assert_eq!(
        entry(&n, "7700273112517203968").redshifts[0]
            .cmb
            .heliocentric
            .value,
        0.318_687_4
    );

    let n = normalized_fixture("boss-dr17-great-wall");
    // Row 1: 5139964890614880256, 150.44297, 42.988546, z 0.4200288 ± 0.0001112926.
    let g = entry(&n, "5139964890614880256");
    assert_eq!(at(g).0, 150.442_97);
    assert_eq!(g.redshifts[0].cmb.heliocentric.value, 0.420_028_8);
    // Row 2: 7988347911129028608, zErr 7.802936E-05.
    let g = entry(&n, "7988347911129028608");
    assert_eq!(
        g.redshifts[0].cmb.heliocentric.uncertainty,
        Uncertainty::Symmetric {
            sigma: 7.802_936e-5
        }
    );
}

#[test]
fn erass1_clusters_join_the_optical_redshift_and_take_each_rows_redshift_kind() {
    let definition = common::definition_of("erass1-clusters");
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/erosita-de/erass1-clusters");
    let files: Vec<_> = [
        "erass1cl_primary_v3.2.fits.tgz",
        "eRASS1_clusters_optical.fits.tgz",
    ]
    .iter()
    .map(|f| dir.join(f))
    .collect();
    for file in &files {
        let sidecar = std::fs::read_to_string(file.with_extension("tgz.yaml")).unwrap();
        let digest = univ_ingest::fetch::hex(&<sha2::Sha256 as sha2::Digest>::digest(
            std::fs::read(file).unwrap(),
        ));
        assert!(
            sidecar.contains(&format!("sha256: {digest}\n")),
            "{}",
            file.display()
        );
    }
    let correction = CmbCorrection::new(DipoleModel::Planck2018);
    let n = normalize(&definition, &files, &correction).unwrap();
    assert_eq!((n.report.published, n.report.left_out_total()), (5, 0));
    // Primary row 1: DETUID em01_002129_020_ML00014_002_c010, NAME 1eRASS J000005.2-383729,
    // RA 0.021729666982255817, DEC -38.6249030550687, BEST_Z 0.3153 ± 0.0079 photo_z, PCONT
    // 0.006; optical row 1: Z_LAMBDA 0.31528 ± 0.00794 as float32 (read with a Python reader
    // of the FITS bytes, scratch script fitsrows.py).
    let c = entry(&n, "em01_002129_020_ML00014_002_c010");
    assert_eq!(c.name.as_deref(), Some("1eRASS J000005.2-383729"));
    assert_eq!(
        at(c),
        (
            0.021_729_666_982_255_817,
            -38.624_903_055_068_7,
            Frame::Icrs,
            Epoch::J2000
        )
    );
    let [best, lambda] = &c.redshifts[..] else {
        panic!("two redshifts")
    };
    assert_eq!(best.cmb.heliocentric.value, 0.3153);
    assert_eq!(
        best.cmb.heliocentric.uncertainty,
        Uncertainty::Symmetric { sigma: 0.0079 }
    );
    assert_eq!(
        (best.kind, best.kind_of_value),
        (RedshiftKind::Photometric, KindOfValue::Estimated)
    );
    assert_eq!(lambda.cmb.heliocentric.value, 0.315_28);
    assert_eq!(
        lambda.cmb.heliocentric.uncertainty,
        Uncertainty::Symmetric { sigma: 0.007_94 }
    );
    assert_eq!(
        (c.flags["BEST_Z_TYPE"].as_str(), c.flags["PCONT"].as_str()),
        ("photo_z", "0.006")
    );
    // Primary row 2: em01_011174_020_ML00047_002_c010, BEST_Z 0.0891 ± 0.0002 cg_spec_z;
    // optical row 2: Z_LAMBDA 0.08594 ± 0.00976.
    let c = entry(&n, "em01_011174_020_ML00047_002_c010");
    let best = &c.redshifts[0];
    assert_eq!(best.cmb.heliocentric.value, 0.0891);
    assert_eq!(
        (best.kind, best.kind_of_value),
        (RedshiftKind::Spectroscopic, KindOfValue::Measured)
    );
    assert_eq!(c.redshifts[1].cmb.heliocentric.value, 0.085_94);
    assert_eq!(c.flags["PCONT"], "0");
}
