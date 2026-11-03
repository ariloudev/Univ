//! Derived radial distances and their chains: a distance rescaled by the rule its size follows,
//! an inverted parallax from the published parallax through its zero point, the peculiar
//! velocity in a redshift distance's interval, a luminosity distance's redshift and its errors,
//! and an unstated equinox. Every value here is SIMULATED, chosen to show one rule.
#![expect(
    clippy::unwrap_used,
    clippy::float_cmp,
    reason = "a failed unwrap is the test failing, and values passed through unchanged compare exactly"
)]

mod common;

use common::near;
use univ_model::cosmology::{AssumedCosmology, SPEED_OF_LIGHT};
use univ_model::derive::{DeriveError, DerivedPosition, Direction, Release, Transformation};
use univ_model::distance::{Distance, DistanceKind, Measure, Method};
use univ_model::entry::{Redshift, RedshiftKind};
use univ_model::position::{Epoch, Frame};
use univ_model::redshift::{CmbRedshift, DipoleModel};
use univ_model::units::Unit;
use univ_model::value::{KindOfValue, Quantity, Uncertainty, Value};

fn quantity(value: f64, unit: Unit, uncertainty: Uncertainty) -> Quantity {
    Quantity::new(value, unit, uncertainty).unwrap()
}

fn redshift(z: f64, uncertainty: Uncertainty, kind: RedshiftKind) -> Redshift {
    let value = quantity(z, Unit::Dimensionless, uncertainty);
    Redshift {
        kind,
        kind_of_value: KindOfValue::Measured,
        velocity: None,
        cmb: CmbRedshift {
            value,
            heliocentric: value,
            dipole: DipoleModel::Planck2018,
            published: univ_model::redshift::RedshiftFrame::Heliocentric,
        },
    }
}

fn steps(p: &[univ_model::derive::Step]) -> Vec<&Transformation> {
    p.iter().map(|s| &s.transformation).collect()
}

fn distance(native: Quantity, measure: Measure, assumed: Option<AssumedCosmology>) -> Distance {
    let how = (DistanceKind::SecondaryIndicator, Method::TullyFisher);
    let kind = KindOfValue::Estimated;
    Distance::new(native, None, Unit::Megaparsec, kind, how, measure, assumed).unwrap()
}

#[test]
fn a_distance_is_rescaled_by_the_rule_its_size_follows() {
    // Like the Sloan Great Wall (Gott et al. 2005): 310 Mpc away and 450 Mpc long for H0 = 71,
    // ΩM = 0.27, ΩΛ = 0.73, here at a simulated redshift of 0.075. The distance and the size
    // take one factor, the ratio of comoving distances at that redshift.
    let release = Release::planck_2018();
    let wmap = AssumedCosmology {
        hubble_constant: 71.0,
        omega_matter: Some(0.27),
        omega_lambda: Some(0.73),
    };
    let z = redshift(
        0.075,
        Uncertainty::NoneReported,
        RedshiftKind::Spectroscopic,
    );
    let native = quantity(310.0, Unit::Megaparsec, Uncertainty::NoneReported);
    let wall = distance(native, Measure::Comoving, Some(wmap));
    let radial = release.radial_from_distance(&wall, Some(&z)).unwrap();
    let size = release.rescaling_at(wmap, 0.075).unwrap();
    let length = quantity(450.0, Unit::Megaparsec, Uncertainty::NoneReported);
    let rescaled = size.apply(&length).unwrap().rescaled.value;
    assert!(near(
        radial.distance().value / 310.0,
        rescaled / 450.0,
        1e-12
    ));
    assert!(matches!(
        steps(radial.chain())[..],
        [
            Transformation::Published { .. },
            Transformation::ComovingRescaled { factor, .. }
        ] if *factor == size.factor
    ));
    // Without a redshift the ratio of Hubble constants stands for both.
    let alone = release.radial_from_distance(&wall, None).unwrap();
    assert!(near(alone.distance().value, 310.0 * 71.0 / 67.66, 1e-12));
}

/// A parallax as Gaia publishes it, in mas, normalized to degrees.
fn parallax(mas: f64, sigma: f64) -> Value {
    let native = quantity(mas, Unit::Milliarcsecond, Uncertainty::Symmetric { sigma });
    Value::normalize(native, Unit::Degree, KindOfValue::Measured).unwrap()
}

#[test]
fn an_inverted_parallax_starts_from_the_published_parallax_and_its_zero_point() {
    // ϖ = 2.000 ± 0.050 mas less a zero point of −0.017 mas is 2.017 mas: 1/ϖ = 495.79 pc,
    // between 1/(ϖ + σ) and 1/(ϖ − σ) exactly.
    let release = Release::planck_2018();
    let published = parallax(2.0, 0.05);
    let radial = release
        .radial_from_parallax(&published, Some(-0.017))
        .unwrap();
    assert_eq!(radial.native(), &published.native);
    assert_eq!(radial.native().unit, Unit::Milliarcsecond);
    let chain = steps(radial.chain());
    assert!(matches!(
        chain[..],
        [
            Transformation::PublishedParallax { native },
            Transformation::ParallaxZeroPoint { zero_point_mas, corrected_mas },
            Transformation::InvertedParallax { parallax_mas },
            Transformation::Euclidean { .. },
        ] if native.value == 2.0 && *zero_point_mas == -0.017
            && near(*corrected_mas, 2.017, 1e-12) && *parallax_mas == *corrected_mas
    ));
    assert!(
        radial.chain()[1]
            .citation
            .contains("Lindegren et al. 2021, A&A 649, A4")
    );
    let d = radial.distance();
    let pc = |mas: f64| 1e-3 / mas;
    assert!(near(d.value, pc(2.017), 1e-12));
    let Uncertainty::Asymmetric { minus, plus } = d.uncertainty else {
        panic!("{:?}", d.uncertainty)
    };
    assert!(near(d.value - minus, pc(2.067), 1e-12));
    assert!(near(d.value + plus, pc(1.967), 1e-12));
    // A source without a zero point, such as Hipparcos, goes straight to the inversion.
    let hipparcos = release.radial_from_parallax(&published, None).unwrap();
    assert!(matches!(
        steps(hipparcos.chain())[..2],
        [
            Transformation::PublishedParallax { .. },
            Transformation::InvertedParallax { parallax_mas }
        ] if *parallax_mas == 2.0
    ));
    // A parallax no larger than its error has no far end.
    assert!(matches!(
        release.radial_from_parallax(&parallax(0.04, 0.05), None),
        Err(DeriveError::Parallax(..))
    ));
}

#[test]
fn a_redshift_distance_s_interval_carries_the_peculiar_velocity() {
    // z = 0.003 ± 1e-5: the redshift's own error moves D by about 0.04 Mpc, the 300 km/s of
    // peculiar velocity by about 4.4 Mpc. σ_D = D′(z) √(σz² + ((1 + z) v / c)²).
    let release = Release::planck_2018();
    assert_eq!(release.peculiar_velocity_km_s(), 300.0);
    let z = redshift(
        0.003,
        Uncertainty::Symmetric { sigma: 1e-5 },
        RedshiftKind::Spectroscopic,
    );
    let radial = release.radial_from_redshift(&z.cmb).unwrap();
    let slope = release.table().slope(0.003).unwrap();
    let sigma_d = slope * 1e-5_f64.hypot(1.003 * 300.0 / SPEED_OF_LIGHT);
    let Uncertainty::Asymmetric { minus, plus } = radial.distance().uncertainty else {
        panic!("{:?}", radial.distance().uncertainty)
    };
    assert!(near(minus, sigma_d, 1e-3), "{minus} against {sigma_d}");
    assert!(near(plus, sigma_d, 1e-3), "{plus} against {sigma_d}");
    // The release's ranking rule sets the parameter.
    let slower = Release::planck_2018().with_peculiar_velocity(150.0);
    let half = slower.radial_from_redshift(&z.cmb).unwrap();
    let Uncertainty::Asymmetric { plus: p, .. } = half.distance().uncertainty else {
        panic!("asymmetric")
    };
    assert!(p < 0.6 * plus);
}

#[test]
fn a_redshift_without_an_error_keeps_the_peculiar_velocity_term_and_a_stated_interval_its_bounds() {
    // z = 0.003 published without an error: its distance still carries the 300 km/s term,
    // D′(z) (1 + z) v / c, never none reported.
    let release = Release::planck_2018();
    let z = redshift(
        0.003,
        Uncertainty::NoneReported,
        RedshiftKind::Spectroscopic,
    );
    let radial = release.radial_from_redshift(&z.cmb).unwrap();
    let sigma_d = release.table().slope(0.003).unwrap() * 1.003 * 300.0 / SPEED_OF_LIGHT;
    let Uncertainty::Asymmetric { minus, plus } = radial.distance().uncertainty else {
        panic!("{:?}", radial.distance().uncertainty)
    };
    assert!(
        near(minus, sigma_d, 1e-3) && near(plus, sigma_d, 1e-3),
        "{minus} {plus}"
    );
    // The slice z = 0.802 ± 0.060 is a stated interval: its bounds become the distance's, with
    // nothing added in quadrature.
    let slice = Uncertainty::Interval {
        minus: 0.06,
        plus: 0.06,
    };
    let z = redshift(0.802, slice, RedshiftKind::Spectroscopic);
    let radial = release.radial_from_redshift(&z.cmb).unwrap();
    let d = radial.distance().value;
    let Uncertainty::Interval { minus, plus } = radial.distance().uncertainty else {
        panic!("{:?}", radial.distance().uncertainty)
    };
    let table = release.table();
    assert!(near(d - minus, table.distance(0.742).unwrap(), 1e-9));
    assert!(near(d + plus, table.distance(0.862).unwrap(), 1e-9));
}

#[test]
fn a_luminosity_distance_takes_its_redshift_s_kind_errors_and_refuses_a_blueshift() {
    let release = Release::planck_2018();
    let native = quantity(
        400.0,
        Unit::Megaparsec,
        Uncertainty::Symmetric { sigma: 20.0 },
    );
    let lum = distance(native, Measure::Luminosity, None);
    // A photometric redshift with an asymmetric error: the chain names its kind, and its upper
    // side widens the lower side of D_C = D_L / (1 + z), its lower side the upper.
    let photo = redshift(
        0.1,
        Uncertainty::Asymmetric {
            minus: 0.01,
            plus: 0.03,
        },
        RedshiftKind::Photometric,
    );
    let radial = release.radial_from_distance(&lum, Some(&photo)).unwrap();
    assert!(matches!(
        steps(radial.chain()).last(),
        Some(Transformation::LuminosityToComoving {
            redshift_kind: RedshiftKind::Photometric,
            ..
        })
    ));
    let d = 400.0 / 1.1;
    let Uncertainty::Asymmetric { minus, plus } = radial.distance().uncertainty else {
        panic!("{:?}", radial.distance().uncertainty)
    };
    let own: f64 = 20.0 / 1.1;
    assert!(near(minus, own.hypot(d * 0.03 / 1.1), 1e-12));
    assert!(near(plus, own.hypot(d * 0.01 / 1.1), 1e-12));
    // A CMB-frame blueshift gives no comoving distance: the distance stays a luminosity one and
    // says why, as a redshift distance refuses it.
    let blue = redshift(
        -0.001,
        Uncertainty::NoneReported,
        RedshiftKind::Spectroscopic,
    );
    let kept = release.radial_from_distance(&lum, Some(&blue)).unwrap();
    assert_eq!(
        (kept.measure(), kept.distance().value),
        (Measure::Luminosity, 400.0)
    );
    assert!(matches!(
        steps(kept.chain()).last(),
        Some(Transformation::KeptAsPublished { reason, .. }) if reason.contains("blueshift")
    ));
    // A redshift error given as a limit cannot be added in quadrature, and is refused.
    let limit = redshift(0.1, Uncertainty::UpperLimit, RedshiftKind::Photometric);
    assert!(matches!(
        release.radial_from_distance(&lum, Some(&limit)),
        Err(DeriveError::Uncombinable(Uncertainty::UpperLimit))
    ));
}

#[test]
fn an_unstated_equinox_is_taken_as_j2000_before_it_is_moved_to_the_icrs() {
    let release = Release::planck_2018();
    let native = quantity(100.0, Unit::Megaparsec, Uncertainty::NoneReported);
    let radial = release
        .radial_from_distance(&distance(native, Measure::Comoving, None), None)
        .unwrap();
    let direction =
        Direction::new(Frame::EquatorialEquinoxUnstated, 150.0, 20.0, Epoch::J2000).unwrap();
    let placed = DerivedPosition::new(direction, radial);
    let chain = steps(placed.chain());
    let at = |f: fn(&Transformation) -> bool| chain.iter().position(|t| f(t)).unwrap();
    let assumed = at(|t| matches!(t, Transformation::EquinoxAssumedJ2000 { .. }));
    let moved = at(|t| matches!(t, Transformation::ToIcrs { .. }));
    assert!(assumed < moved, "{chain:?}");
}
