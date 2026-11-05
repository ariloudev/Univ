//! The order of an object's redshift distances among themselves (RQ-24 under the decision "Rank
//! redshift distances by kind of value and velocity uncertainty"): a measured redshift first,
//! then the smallest published velocity uncertainty, one without it after those with it, then
//! method rank and the rule's order of sources; and only the leading redshift distance competes
//! with the other distances. ESO 460-G001 is release 5's object 2044535 with its published
//! values (6dFGS DR3, 2M++ and 2MRS); every other object here is SIMULATED, under the
//! repository's `rules/ranking.yaml` and the Planck 2018 cosmology.
#![expect(clippy::unwrap_used, reason = "a failed unwrap is the test failing")]

mod common;
mod simulated;

use simulated::{entry, ranking_rules, with_bare_cz, with_cz, with_modulus};
use univ_ingest::ranking::rank;
use univ_model::derive::Release;
use univ_model::distance::{DistanceKind, Method};
use univ_model::entry::{CatalogueEntry, Redshift, RedshiftKind};
use univ_model::object::{
    Decided, FractionalUncertainty, MeasurementRef, MethodRank, Object, ObjectEntry,
    PreferredDistance, Resolution,
};
use univ_model::position::unit_vector;
use univ_model::redshift::{CmbCorrection, DipoleModel};
use univ_model::units::Unit;
use univ_model::value::{KindOfValue, Quantity, Uncertainty};

const TF: (DistanceKind, Method) = (DistanceKind::SecondaryIndicator, Method::TullyFisher);

/// RA 78°, Dec 0°: about 90° from the CMB dipole's apex, so a CMB-frame cz is within some tens
/// of km/s of the heliocentric one (see `ranking.rs`).
const RA: f64 = 78.0;

fn preferred(entries: Vec<CatalogueEntry>) -> PreferredDistance {
    let mut o = Object::new(
        entries
            .into_iter()
            .map(|entry| ObjectEntry {
                entry,
                resolved_by: Resolution::Anchor.into(),
            })
            .collect::<Vec<_>>(),
    );
    rank(&mut o, &ranking_rules(), &Release::planck_2018());
    o.preferred_distance.unwrap()
}

/// A measured spectroscopic cz ± `sigma` km/s from `src`.
fn measured(src: &str, cz: f64, sigma: f64) -> CatalogueEntry {
    with_cz(entry(src, "m", RA, 0.0), cz, sigma)
}

/// A 2M++ clone (Cln = 1): a spectroscopic cz ± `sigma` km/s marked estimated, as the source
/// definition marks it.
fn clone(cz: f64, sigma: f64) -> CatalogueEntry {
    let mut e = with_cz(entry("twompp-catalogue", "c", RA, 0.0), cz, sigma);
    e.redshifts[0].kind_of_value = KindOfValue::Estimated;
    e
}

/// A photometric redshift cz ± `sigma` km/s, estimated.
fn photometric(src: &str, cz: f64, sigma: f64) -> CatalogueEntry {
    let mut e = entry(src, "p", RA, 0.0);
    let q = Quantity::new(
        cz,
        Unit::KilometrePerSecond,
        Uncertainty::Symmetric { sigma },
    )
    .unwrap();
    let r = Redshift::heliocentric(
        q,
        RedshiftKind::Photometric,
        KindOfValue::Estimated,
        unit_vector(RA, 0.0),
        &CmbCorrection::new(DipoleModel::Planck2018),
    )
    .unwrap();
    e.redshifts.push(r);
    e
}

fn source(p: &PreferredDistance) -> &str {
    p.measurement.entry().source.as_str()
}

fn runner_up_source(p: &PreferredDistance) -> &str {
    p.runner_up.as_ref().unwrap().0.entry().source.as_str()
}

#[test]
fn eso_460_g001_is_placed_by_2mrs_s_redshift_with_the_smaller_velocity_error() {
    // Release 5's object 2044535: 6dFGS g1925229-321306 cz = 54,179 ± 55 km/s, 2M++
    // 19252294-3213062 54,179 km/s with no error, and 2MRS 19252294-3213062 7,520 ± 10 km/s, all
    // measured and heliocentric. Fractional distance uncertainty placed it at 54,179 km/s, where
    // the 300 km/s term is the smaller fraction; the smaller velocity error places it at 7,520.
    let (ra, dec) = (291.345_576_723_172, -32.218_367_168_674_7);
    let p = preferred(vec![
        with_cz(
            entry("sixdfgs-dr3", "g1925229-321306", ra, dec),
            54_179.0,
            55.0,
        ),
        with_bare_cz(
            entry("twompp-catalogue", "19252294-3213062", ra, dec),
            54_179.0,
        ),
        with_cz(entry("twomrs", "19252294-3213062", ra, dec), 7_520.0, 10.0),
    ]);
    assert_eq!(source(&p), "twomrs");
    assert_eq!(p.decided, Decided::VelocityUncertainty);
    assert!(p.distance_mpc > 95.0 && p.distance_mpc < 115.0, "{p:?}");
    assert_eq!(runner_up_source(&p), "sixdfgs-dr3");
}

#[test]
fn a_measured_6dfgs_redshift_beats_a_2mpp_clone() {
    // The clone has the higher cz, the smaller fraction and the smaller velocity error; it is
    // estimated, and the measured redshift places the object.
    let p = preferred(vec![
        clone(15_000.0, 10.0),
        measured("sixdfgs-dr3", 5_000.0, 45.0),
    ]);
    assert_eq!(source(&p), "sixdfgs-dr3");
    assert_eq!(p.decided, Decided::KindOfValue);
    // A measured redshift without a published error still beats a clone with one.
    let bare = with_bare_cz(entry("sixdfgs-dr3", "b", RA, 0.0), 5_000.0);
    let p = preferred(vec![clone(15_000.0, 10.0), bare]);
    assert_eq!(
        (source(&p), p.decided),
        ("sixdfgs-dr3", Decided::KindOfValue)
    );
    // So does a measured redshift against a photometric one.
    let p = preferred(vec![
        photometric("sdss-dr17-galaxies", 15_000.0, 10.0),
        measured("twomrs", 5_000.0, 45.0),
    ]);
    assert_eq!((source(&p), p.decided), ("twomrs", Decided::KindOfValue));
}

#[test]
fn of_two_measured_redshifts_the_smaller_velocity_error_wins() {
    let p = preferred(vec![
        measured("sixdfgs-dr3", 12_000.0, 45.0),
        measured("twomrs", 9_000.0, 20.0),
    ]);
    assert_eq!(
        (source(&p), p.decided),
        ("twomrs", Decided::VelocityUncertainty)
    );
    let p = preferred(vec![
        measured("sixdfgs-dr3", 9_000.0, 20.0),
        measured("twomrs", 12_000.0, 45.0),
    ]);
    assert_eq!(
        (source(&p), p.decided),
        ("sixdfgs-dr3", Decided::VelocityUncertainty)
    );
}

#[test]
fn a_redshift_without_a_velocity_error_ranks_after_one_with_one_and_is_given_none() {
    let bare = with_bare_cz(entry("twompp-catalogue", "b", RA, 0.0), 20_000.0);
    let p = preferred(vec![bare, measured("twomrs", 9_000.0, 80.0)]);
    assert_eq!(
        (source(&p), p.decided),
        ("twomrs", Decided::VelocityUncertainty)
    );
    let (lost, u) = *p.runner_up.unwrap();
    assert_eq!(lost.entry().source.as_str(), "twompp-catalogue");
    let FractionalUncertainty::Reported { measurement, .. } = u else {
        unreachable!("a redshift distance carries its peculiar-velocity term")
    };
    assert_eq!(measurement, None, "no velocity error is invented");
}

#[test]
fn redshifts_alike_in_kind_and_error_rank_by_method_then_source_order() {
    // Two estimated redshifts of one error: the spectroscopic clone before the photometric one.
    let p = preferred(vec![
        photometric("sdss-dr17-galaxies", 9_000.0, 30.0),
        clone(12_000.0, 30.0),
    ]);
    assert_eq!(
        (p.rank, p.decided),
        (MethodRank::SpectroscopicRedshift, Decided::MethodRank)
    );
    // Two measured ones of one error and method: 2MRS is listed before 6dFGS.
    let p = preferred(vec![
        measured("sixdfgs-dr3", 1_100.0, 10.0),
        measured("twomrs", 1_000.0, 10.0),
    ]);
    assert_eq!((source(&p), p.decided), ("twomrs", Decided::SourceOrder));
}

#[test]
fn only_the_leading_redshift_competes_with_an_indicator_distance() {
    // A measured 1,000 ± 10 km/s (fraction about 0.30, above the 0.25 threshold), a clone at
    // 5,000 ± 10 km/s (about 0.06) and Tully-Fisher ±0.8 mag (about 0.38). The measured
    // redshift leads the redshifts and is set aside, so the Tully-Fisher distance places the
    // object; the clone never competes, though its fraction is the smallest.
    let tf = with_modulus(entry("cosmicflows-4", "1", RA, 0.0), 30.25, 0.8, TF);
    let p = preferred(vec![
        clone(5_000.0, 10.0),
        measured("twomrs", 1_000.0, 10.0),
        tf.clone(),
    ]);
    assert!(
        matches!(p.measurement, MeasurementRef::Distance { .. }),
        "{p:?}"
    );
    assert_eq!(p.decided, Decided::RedshiftThreshold);
    assert_eq!(runner_up_source(&p), "twomrs");
    // Where the leading redshift beats the indicator and the next candidate is another
    // redshift, the redshifts' own comparison is recorded; where the next is the indicator,
    // the fractional uncertainty is.
    let p = preferred(vec![
        measured("sixdfgs-dr3", 12_000.0, 45.0),
        measured("twomrs", 9_000.0, 20.0),
        tf.clone(),
    ]);
    assert_eq!(
        (source(&p), p.decided),
        ("twomrs", Decided::VelocityUncertainty)
    );
    let p = preferred(vec![measured("twomrs", 9_000.0, 20.0), tf]);
    assert_eq!((source(&p), p.decided), ("twomrs", Decided::Uncertainty));
}

#[test]
fn the_object_names_kind_of_value_or_velocity_uncertainty_as_what_decided() {
    // AC-63: the redshifts' own comparisons are written by name.
    let p = preferred(vec![
        clone(15_000.0, 10.0),
        measured("sixdfgs-dr3", 5_000.0, 45.0),
    ]);
    let yaml = serde_norway::to_string(&p).unwrap();
    assert!(yaml.contains("decided: kind_of_value"), "{yaml}");
    let p = preferred(vec![
        measured("sixdfgs-dr3", 12_000.0, 45.0),
        measured("twomrs", 9_000.0, 20.0),
    ]);
    let yaml = serde_norway::to_string(&p).unwrap();
    assert!(yaml.contains("decided: velocity_uncertainty"), "{yaml}");
}
