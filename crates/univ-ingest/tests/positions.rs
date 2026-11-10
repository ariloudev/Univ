//! Derived positions under the release cosmology (AC-28 to AC-31, AC-34, AC-35): the release
//! cosmology and its tabulated comoving distance, redshift and luminosity distances made
//! comoving, M31 placed from its `McConnachie` 2012 modulus, the Galactic centre and the
//! supergalactic pole, and the chain each derived value records.
#![expect(
    clippy::unwrap_used,
    clippy::float_cmp,
    reason = "a failed unwrap is the test failing, and values passed through unchanged compare exactly"
)]

mod common;

use common::{definition_of, entry, near, normalize_body, normalized_fixture};
use univ_model::cosmology::{AssumedCosmology, Cosmology};
use univ_model::derive::{
    DeriveError, DerivedPosition, Direction, MODEL_VERSION, Release, Transformation,
};
use univ_model::distance::{Distance, DistanceKind, Measure, Method};
use univ_model::entry::{Redshift, RedshiftKind};
use univ_model::position::{Epoch, Frame, between, dot, lon_lat, separation_degrees, unit_vector};
use univ_model::redshift::{CmbRedshift, DipoleModel};
use univ_model::units::Unit;
use univ_model::value::{KindOfValue, Quantity, Uncertainty};

/// astropy 8.0.2 `FlatLambdaCDM(H0=67.66, Om0=0.3111, Tcmb0=0).comoving_distance(z)`, Mpc: the
/// release model exactly. Planck 2018 (A&A 641, A6) publishes no `D_C(z)` table at z ≤ 10.
const FLAT: [(f64, f64); 6] = [
    (0.01, 44.205_082_235_662_75),
    (0.1, 432.568_393_622_056_7),
    (0.5, 1_946.482_540_627_506),
    (1.0, 3_395.867_123_692_558),
    (3.0, 6_505.130_213_036_533),
    (10.0, 9_639.919_037_694_108),
];
/// astropy 8.0.2 `Planck18`, its realization of Planck 2018 VI, Table 2, last column, with
/// radiation and 0.06 eV of neutrinos, which the release model leaves out.
const PLANCK18: [(f64, f64); 4] = [
    (0.1, 432.565_697_738_352_36),
    (1.0, 3_395.634_471_151_566_2),
    (3.0, 6_504.003_988_949_642),
    (10.0, 9_636.285_079_457_526),
];

fn quantity(value: f64, unit: Unit, uncertainty: Uncertainty) -> Quantity {
    Quantity::new(value, unit, uncertainty).unwrap()
}

fn cmb(z: f64, uncertainty: Uncertainty) -> CmbRedshift {
    let value = quantity(z, Unit::Dimensionless, uncertainty);
    CmbRedshift {
        value,
        measured: value,
        dipole: DipoleModel::Planck2018,
        published: univ_model::redshift::RedshiftFrame::Heliocentric,
    }
}

/// A spectroscopic redshift whose CMB-frame value is `cmb`.
fn spectroscopic(cmb: CmbRedshift) -> Redshift {
    Redshift {
        kind: RedshiftKind::Spectroscopic,
        kind_of_value: KindOfValue::Measured,
        velocity: None,
        cmb,
    }
}

fn modulus(mu: f64, sigma: f64, assumed: Option<AssumedCosmology>) -> Distance {
    let native = quantity(mu, Unit::Magnitude, Uncertainty::Symmetric { sigma });
    let how = (DistanceKind::SecondaryIndicator, Method::TullyFisher);
    let (unit, kind) = (Unit::Megaparsec, KindOfValue::Measured);
    Distance::new(native, None, unit, kind, how, Measure::Luminosity, assumed).unwrap()
}

fn steps(p: &[univ_model::derive::Step]) -> Vec<&Transformation> {
    p.iter().map(|s| &s.transformation).collect()
}

#[test]
fn the_release_cosmology_is_planck_2018_unless_the_release_names_another() {
    // AC-28.
    let release = Release::planck_2018();
    let planck = release.cosmology();
    assert_eq!(
        (planck.hubble_constant, planck.omega_matter),
        (67.66, 0.3111)
    );
    assert!(planck.name.contains("Planck 2018") && planck.name.contains("A&A 641, A6"));
    let table = release.table();
    for (z, expected) in FLAT {
        let d = table.distance(z).unwrap();
        assert!(near(d, expected, 1e-6), "z = {z}: {d}, astropy {expected}");
        assert!(near(d, planck.comoving_distance(z).unwrap(), 1e-6));
        assert!((table.redshift(d).unwrap() - z).abs() < 1e-9 * z.max(1.0));
    }
    for (z, expected) in PLANCK18 {
        assert!(near(table.distance(z).unwrap(), expected, 5e-4), "z = {z}");
    }
    // A release that names another cosmology derives under it, and says so.
    let wmap9 = Cosmology::new("WMAP9 (Hinshaw et al. 2013, ApJS 208, 19)", 69.32, 0.2865);
    let wmap = Release::new(&wmap9.unwrap());
    let radial = wmap.radial_from_redshift(&cmb(0.1, Uncertainty::NoneReported));
    let radial = radial.unwrap();
    assert!(radial.cosmology().name.starts_with("WMAP9"));
    let ours = Release::planck_2018().radial_from_redshift(&cmb(0.1, Uncertainty::NoneReported));
    assert!(radial.distance().value < ours.unwrap().distance().value);
}

#[test]
fn a_redshift_distance_is_the_comoving_distance_of_the_cmb_frame_redshift() {
    // AC-29, on the 2MRS rows: each comes from the CMB-frame redshift, not the heliocentric one.
    let release = Release::planck_2018();
    let n = normalized_fixture("twomrs");
    let (mut placed, mut refused) = (0, 0);
    for e in &n.entries {
        let Some(r) = e.redshifts.first() else {
            continue;
        };
        let z = r.cmb.value.value;
        match release.radial_from_redshift(&r.cmb) {
            Ok(radial) => {
                let d = radial.distance();
                let expected = Cosmology::planck_2018().comoving_distance(z).unwrap();
                assert!(near(d.value, expected, 1e-6), "{:?}", e.designation);
                assert_eq!(
                    (d.unit, radial.measure()),
                    (Unit::Megaparsec, Measure::Comoving)
                );
                let heliocentric = release.table().distance(r.cmb.measured.value.max(0.0));
                assert!((d.value - heliocentric.unwrap()).abs() > 1e-3);
                assert!(matches!(
                    steps(radial.chain())[..],
                    [Transformation::ComovingFromRedshift { cmb_redshift, dipole: DipoleModel::Planck2018, .. }]
                        if *cmb_redshift == z
                ));
                // A symmetric error in z becomes an asymmetric interval in distance.
                let Uncertainty::Asymmetric { minus, plus } = d.uncertainty else {
                    panic!("{:?}", d.uncertainty)
                };
                assert!(minus > 0.0 && plus > 0.0);
                placed += 1;
            }
            // M31 and NGC 253 approach us in the CMB frame: no redshift distance.
            Err(e) => {
                assert_eq!(e, DeriveError::Blueshift(z));
                refused += 1;
            }
        }
    }
    assert!(
        placed >= 1 && refused >= 1,
        "{placed} placed, {refused} refused"
    );
}

#[test]
fn a_luminosity_distance_at_z_0_1_converts_by_one_over_one_plus_z() {
    // AC-29: the modulus of a source at z = 0.1 in the release cosmology, D_L = 1.1 D_C(0.1).
    let release = Release::planck_2018();
    let d_l = 1.1 * FLAT[1].1;
    let mu = 5.0 * (d_l * 1e6 / 10.0).log10();
    let distance = modulus(mu, 0.1, None);
    let z = cmb(0.1, Uncertainty::NoneReported);
    let radial = release
        .radial_from_distance(&distance, Some(&spectroscopic(z)))
        .unwrap();
    assert!(near(radial.distance().value, d_l / 1.1, 1e-9));
    assert!(
        near(radial.distance().value, FLAT[1].1, 1e-6),
        "the redshift distance"
    );
    assert_eq!(radial.measure(), Measure::Comoving);
    let Uncertainty::Asymmetric { minus, plus } = radial.distance().uncertainty else {
        panic!("asymmetric")
    };
    let Uncertainty::Asymmetric { minus: m, plus: p } = distance.normalized.uncertainty else {
        panic!("asymmetric")
    };
    assert!(near(minus, m / 1.1, 1e-9) && near(plus, p / 1.1, 1e-9));
    // Without a CMB-frame redshift it stays a luminosity distance, and says why.
    let kept = release.radial_from_distance(&distance, None).unwrap();
    assert!(near(kept.distance().value, d_l, 1e-9));
    assert_eq!(kept.measure(), Measure::Luminosity);
    assert!(matches!(
        steps(kept.chain()).last(),
        Some(Transformation::KeptAsPublished {
            measure: Measure::Luminosity,
            ..
        })
    ));
}

/// M31 as `McConnachie` 2012 gives it: the recorded row of Andromeda.
fn m31() -> DerivedPosition {
    let body = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/vizier/mcconnachie-2012-andromeda.tsv"),
    )
    .unwrap();
    let n = normalize_body(&definition_of("mcconnachie-2012"), &body);
    let andromeda = entry(&n, "Andromeda");
    let release = Release::planck_2018();
    let position = andromeda.position.as_ref();
    let distance = andromeda.distances.first();
    release.place(position, distance, None).unwrap().unwrap()
}

#[test]
fn m31_is_placed_at_its_mcconnachie_distance_toward_andromeda() {
    // AC-30, AC-34, AC-35. The row: 010.684583 +041.269167 (FK5 J2000, computed by VizieR),
    // (m-M)0 = 24.47 +0.07 -0.07, D = 783 ± 25 kpc. 10^(24.47/5 + 1) pc = 0.783 430 Mpc.
    let m31 = m31();
    assert_eq!(m31.native().value, 24.47);
    assert!(near(m31.distance().value, 0.783_429_642_766_212, 1e-12));
    assert_eq!(m31.measure(), Measure::Luminosity, "no redshift in the row");
    let Uncertainty::Asymmetric { minus, plus } = m31.distance().uncertainty else {
        panic!("asymmetric")
    };
    // ±0.07 mag is d (1 − 10^−0.014) = 24.85 kpc below and d (10^0.014 − 1) = 25.67 kpc above,
    // which the paper rounds to ±25 kpc.
    assert!((minus - 0.024_852).abs() < 2e-6 && (plus - 0.025_666).abs() < 2e-6);
    for frame in [Frame::Icrs, Frame::Galactic, Frame::Supergalactic] {
        let v = m31.cartesian(frame).unwrap();
        assert!(near(dot(v, v).sqrt(), m31.distance().value, 1e-12));
    }
    // astropy 8.0.2 from the same FK5 position and distance; its galactic frame comes from FK5,
    // 0.1″ off the Hipparcos one, 4e-7 Mpc at this distance.
    let (l, b) = lon_lat(m31.cartesian(Frame::Galactic).unwrap());
    assert!(
        (l - 121.174_235).abs() < 1e-4 && (b + 21.572_890).abs() < 1e-4,
        "{l} {b}"
    );
    let galactic = [-0.377_128_739, 0.623_346_002, -0.288_054_996];
    let supergalactic = [0.699_637_791, -0.308_673_706, 0.170_263_062];
    for (frame, expected) in [
        (Frame::Galactic, galactic),
        (Frame::Supergalactic, supergalactic),
    ] {
        let v = m31.cartesian(frame).unwrap();
        assert!(
            v.iter().zip(expected).all(|(a, e)| (a - e).abs() < 1e-6),
            "{v:?}"
        );
    }
    // The origin is the barycentre: the far end of the radial interval lies along the same
    // line, further out.
    let (near_end, far_end) = m31.radial_interval(Frame::Galactic).unwrap();
    assert!(separation_degrees(near_end, far_end) < 1e-9);
    assert!(near(dot(far_end, far_end).sqrt(), 0.783_43 + plus, 1e-5));
    assert!(near(dot(near_end, near_end).sqrt(), 0.783_43 - minus, 1e-5));
}

#[test]
fn the_galactic_centre_and_the_supergalactic_pole_round_trip() {
    // AC-30, AC-34, AC-35. R0 = 8178 ± 13 (stat.) pc, GRAVITY Collaboration 2019, A&A 625, L10.
    let release = Release::planck_2018();
    let how = (DistanceKind::Geometric, Method::Other("S2 orbit".into()));
    let r0 = Distance::new(
        quantity(
            8_178.0,
            Unit::Parsec,
            Uncertainty::Symmetric { sigma: 13.0 },
        ),
        None,
        Unit::Parsec,
        KindOfValue::Measured,
        how,
        Measure::Euclidean,
        None,
    )
    .unwrap();
    let radial = release.radial_from_distance(&r0, None).unwrap();
    let centre = Direction::new(Frame::Galactic, 0.0, 0.0, Epoch::J2000).unwrap();
    // The Hipparcos definition puts l = b = 0 at α = 266.40499°, δ = −28.93618° (astropy 8.0.2).
    let icrs = centre.icrs();
    assert!((icrs.ra - 266.404_988).abs() < 1e-4 && (icrs.dec + 28.936_178).abs() < 1e-4);
    let gc = DerivedPosition::new(centre, radial.clone());
    let [gx, gy, gz] = gc.cartesian(Frame::Galactic).unwrap();
    assert!(near(gx, 8.178e-3, 1e-12) && gy.abs() < 1e-15 && gz.abs() < 1e-15);
    assert!(matches!(
        steps(&gc.chain())[..2],
        [Transformation::Published { .. }, Transformation::Euclidean { largest_difference }]
            if *largest_difference < 2e-6
    ));
    // The supergalactic pole lies at l = 47.37°, b = +6.32° (Lahav et al. 2000).
    let pole = Direction::new(Frame::Supergalactic, 0.0, 90.0, Epoch::J2000).unwrap();
    let up = DerivedPosition::new(pole, radial);
    let [sx, sy, sz] = up.cartesian(Frame::Supergalactic).unwrap();
    assert!(sx.abs() < 1e-15 && sy.abs() < 1e-15 && near(sz, 8.178e-3, 1e-12));
    let (l, b) = lon_lat(up.cartesian(Frame::Galactic).unwrap());
    assert!(
        (l - 47.37).abs() < 1e-9 && (b - 6.32).abs() < 1e-9,
        "{l} {b}"
    );
    // And back: ICRS → galactic → supergalactic → ICRS leaves a direction where it was.
    let start = unit_vector(123.4, -56.7);
    let around = between(Frame::Supergalactic, Frame::Icrs).apply(
        between(Frame::Galactic, Frame::Supergalactic)
            .apply(between(Frame::Icrs, Frame::Galactic).apply(start)),
    );
    assert!(separation_degrees(start, around) < 1e-12);
}

#[test]
fn each_derived_value_records_the_release_cosmology_its_chain_and_the_model_version() {
    // AC-31: a Tully-Fisher modulus published for H0 = 75, with a CMB-frame redshift, at an
    // FK5 position: every step in order, each with its citation.
    let release = Release::planck_2018();
    let distance = modulus(35.0, 0.4, Some(AssumedCosmology::hubble(75.0)));
    let z = cmb(0.023, Uncertainty::Symmetric { sigma: 1e-4 });
    let radial = release
        .radial_from_distance(&distance, Some(&spectroscopic(z)))
        .unwrap();
    let direction = Direction::new(Frame::Fk5J2000, 54.430_541_7, -22.908_194_4, Epoch::J2000);
    let p = DerivedPosition::new(direction.unwrap(), radial);
    assert_eq!(p.cosmology(), &Cosmology::planck_2018());
    let held = serde_json::to_value(&p).unwrap();
    assert_eq!(
        (held["model_version"].as_str(), MODEL_VERSION),
        (Some("0.1.0"), "0.1.0")
    );
    let factor = 75.0 / 67.66;
    assert!(matches!(
        steps(&p.chain())[..],
        [
            Transformation::Published { measure: Measure::Luminosity, .. },
            Transformation::FromModulus { .. },
            Transformation::HubbleRescaled { factor: f, release_hubble_constant: h, .. },
            Transformation::LuminosityToComoving {
                cmb_redshift,
                redshift_kind: RedshiftKind::Spectroscopic,
            },
            Transformation::ToIcrs { from: Frame::Fk5J2000 },
            Transformation::Cartesian,
            Transformation::Rotated { to: Frame::Galactic },
            Transformation::Rotated { to: Frame::Supergalactic },
        ] if (f - factor).abs() < 1e-15 && *h == 67.66 && *cmb_redshift == 0.023
    ));
    assert!(p.chain().iter().all(|s| !s.citation.is_empty()));
    assert!(p.chain()[2].citation.contains("Hogg 1999"));
    assert!(p.chain()[6].citation.contains("Hipparcos"));
    assert!(p.chain()[7].citation.contains("Lahav et al. 2000"));
    // 100 Mpc for H0 = 75 is 110.85 Mpc for 67.66, then / 1.023.
    assert!(near(p.distance().value, 100.0 * factor / 1.023, 1e-9));
    assert_eq!(p.assumed(), Some(&AssumedCosmology::hubble(75.0)));
    let yaml = serde_norway::to_string(&p).unwrap();
    for field in [
        "model_version: 0.1.0",
        "step: hubble_rescaled",
        "citation:",
        "Planck 2018",
    ] {
        assert!(yaml.contains(field), "{field} missing from {yaml}");
    }
}

#[test]
fn an_object_without_a_distance_or_a_position_is_not_placed() {
    let release = Release::planck_2018();
    let n = normalized_fixture("mcconnachie-2012");
    // The Galaxy's row has a position and nothing else.
    let galaxy = entry(&n, "The Galaxy");
    assert!(
        release
            .place(galaxy.position.as_ref(), None, None)
            .unwrap()
            .is_none()
    );
    let distance = modulus(30.0, 0.1, None);
    assert!(
        release
            .place(None, Some(&distance), None)
            .unwrap()
            .is_none()
    );
}

#[test]
fn a_barycentric_redshift_names_its_frame_in_the_distance_chain() {
    let measured = quantity(
        0.4,
        Unit::Dimensionless,
        Uncertainty::Symmetric { sigma: 1e-5 },
    );
    let correction = univ_model::redshift::CmbCorrection::new(DipoleModel::Planck2018);
    let bary = correction
        .apply(
            measured,
            univ_model::redshift::RedshiftFrame::Barycentric,
            unit_vector(150.0, 2.0),
        )
        .unwrap();
    let radial = Release::planck_2018().radial_from_redshift(&bary).unwrap();
    assert!(matches!(
        steps(radial.chain())[..],
        [Transformation::ComovingFromBarycentricRedshift { barycentric_redshift, dipole: DipoleModel::Planck2018, .. }]
            if *barycentric_redshift == 0.4
    ));
}
