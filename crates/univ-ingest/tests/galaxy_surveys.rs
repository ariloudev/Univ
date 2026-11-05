//! The galaxy surveys of release 10 normalize from their recorded answers: DESI's barycentric
//! redshifts, the rest of SDSS DR17, 2dFGRS with its catalogue-wide rms and ALFALFA's HI
//! velocities (normalize-records); and a definition can leave out rows below a redshift floor,
//! or rows whose cell is none of a rule's values.
#![expect(clippy::unwrap_used, reason = "a failed unwrap is the test failing")]

mod common;

use common::{definition_of, fixture_of, normalize_body, normalized_fixture};
use univ_ingest::normalize::Normalized;
use univ_ingest::source::{Condition, LeftOutReason, RedshiftFloor, Rule, SourceDefinition};
use univ_model::cosmology::SPEED_OF_LIGHT;
use univ_model::entry::RedshiftKind;
use univ_model::redshift::RedshiftFrame;
use univ_model::value::{KindOfValue, Uncertainty};

fn normalized(d: &SourceDefinition) -> Normalized {
    normalize_body(d, &std::fs::read(fixture_of(d)).unwrap())
}

fn left_out(n: &Normalized, reason: LeftOutReason) -> u64 {
    n.report.left_out.get(&reason).copied().unwrap_or(0)
}

#[test]
fn desi_redshifts_are_measured_spectroscopic_and_barycentric() {
    let n = normalized_fixture("desi-dr1");
    assert_eq!((n.report.published, n.report.left_out_total()), (6, 0));
    // Row 1: targetid 39628141735447600, z 0.1702654601698496 ± 4.441743535373783e-05.
    let e = n
        .entries
        .iter()
        .find(|e| e.designation.identifier.as_str() == "39628141735447600")
        .unwrap();
    let r = &e.redshifts[0];
    assert_eq!(r.kind, RedshiftKind::Spectroscopic);
    assert_eq!(r.kind_of_value, KindOfValue::Measured);
    assert_eq!(r.cmb.published, RedshiftFrame::Barycentric);
    assert!((r.cmb.measured.value - 0.170_265_460_169_849_6).abs() < 1e-12);
    assert_eq!(
        r.cmb.measured.uncertainty,
        Uncertainty::Symmetric {
            sigma: 4.441_743_535_373_783e-5
        }
    );
    assert_ne!(
        r.cmb.value.value, r.cmb.measured.value,
        "moved to the CMB frame"
    );
}

#[test]
fn the_rest_of_sdss_takes_distant_galaxies_and_quasars() {
    let n = normalized_fixture("sdss-dr17-distant");
    assert_eq!((n.report.published, n.report.left_out_total()), (6, 0));
    let mut z: Vec<f64> = n
        .entries
        .iter()
        .map(|e| e.redshifts[0].cmb.measured.value)
        .collect();
    z.sort_by(f64::total_cmp);
    assert!(z[0] >= 0.35, "{z:?}");
    // A quasar of the recorded answer: specObjID 7281375122652682240 at z = 2.773316.
    assert!((z[5] - 2.773_316).abs() < 1e-6, "{z:?}");
}

#[test]
fn twodfgrs_gives_each_redshift_its_catalogues_rms() {
    let n = normalized_fixture("twodfgrs");
    assert_eq!((n.report.published, n.report.left_out_total()), (5, 0));
    for e in &n.entries {
        let Uncertainty::Symmetric { sigma } = e.redshifts[0].cmb.measured.uncertainty else {
            panic!("{:?}", e.redshifts[0])
        };
        assert!((sigma - 85.0 / SPEED_OF_LIGHT).abs() < 1e-12, "{sigma}");
        assert_eq!(e.redshifts[0].cmb.published, RedshiftFrame::Heliocentric);
    }
}

#[test]
fn alfalfa_velocities_are_heliocentric_cz_with_none_reported() {
    let n = normalized_fixture("alfalfa-a100");
    assert_eq!((n.report.published, n.report.left_out_total()), (5, 0));
    // AGC 105367: Vhel 11983 km/s.
    let e = n
        .entries
        .iter()
        .find(|e| e.designation.identifier.as_str() == "105367")
        .unwrap();
    let cz = e.redshifts[0].velocity.unwrap();
    assert!((cz.value - 11_983.0).abs() < 1e-9);
    assert_eq!(cz.uncertainty, Uncertainty::NoneReported);
}

#[test]
fn rows_below_a_redshift_floor_are_left_out_as_unsuitable() {
    let mut d = definition_of("sdss-dr17-distant");
    d.normalization.redshifts[0].floor = Some(RedshiftFloor {
        below: 0.5,
        basis: "a test".to_owned(),
    });
    let n = normalized(&d);
    // One recorded row lies below 0.5: specObjID 5487829205420103680 at z = 0.4038015.
    assert_eq!(n.report.published, 5);
    assert_eq!(left_out(&n, LeftOutReason::UnsuitableRedshift), 1);
}

#[test]
fn a_rule_may_leave_out_rows_whose_cell_is_none_of_its_values() {
    let mut d = definition_of("twodfgrs");
    d.normalization.rules.push(Rule {
        when: Condition {
            column: "q_z".to_owned(),
            is: vec!["4".to_owned(), "5".to_owned()],
            not: true,
        },
        and: Vec::new(),
        leave_out: Some(LeftOutReason::OtherSurvey),
        kind_of_value: None,
        distance: None,
        basis: "a test".to_owned(),
    });
    let n = normalized(&d);
    // The recorded rows' q_z are 4, 5, 4, 3 and 4.
    assert_eq!(n.report.published, 4);
    assert_eq!(left_out(&n, LeftOutReason::OtherSurvey), 1);
}
