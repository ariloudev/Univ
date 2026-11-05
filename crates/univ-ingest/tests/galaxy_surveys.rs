//! The galaxy surveys of release 10 normalize from their recorded answers: DESI's barycentric
//! redshifts, the rest of SDSS DR17, 2dFGRS with its catalogue-wide rms and ALFALFA's HI
//! velocities (normalize-records); and a rule can leave out rows whose cell is below a number,
//! or none of its values.

mod common;

use common::{definition_of, entry, normalized_definition, normalized_fixture};
use univ_ingest::normalize::Normalized;
use univ_ingest::source::{Condition, LeftOutReason, Rule};
use univ_model::cosmology::SPEED_OF_LIGHT;
use univ_model::entry::RedshiftKind;
use univ_model::redshift::RedshiftFrame;
use univ_model::value::{KindOfValue, Uncertainty};

fn left_out(n: &Normalized, reason: LeftOutReason) -> u64 {
    n.report.left_out.get(&reason).copied().unwrap_or(0)
}

#[test]
fn desi_redshifts_are_measured_spectroscopic_and_barycentric() {
    let n = normalized_fixture("desi-dr1");
    assert_eq!((n.report.published, n.report.left_out_total()), (6, 0));
    // Row 1: targetid 39628141735447600, z 0.1702654601698496 ± 4.441743535373783e-05.
    let e = entry(&n, "39628141735447600");
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
fn alfalfa_velocities_are_heliocentric_cz_with_half_the_width_error() {
    let n = normalized_fixture("alfalfa-a100");
    assert_eq!((n.report.published, n.report.left_out_total()), (5, 0));
    // AGC 105367: Vhel 11983 km/s.
    let e = entry(&n, "105367");
    let cz = e.redshifts[0].velocity.unwrap();
    assert!((cz.value - 11_983.0).abs() < 1e-9);
    // e_W50 39 km/s: Haynes et al. 2018 adopt half of it as the velocity's error.
    assert_eq!(cz.uncertainty, Uncertainty::Symmetric { sigma: 19.5 });
}

/// `id`'s definition with a rule that leaves out rows `when` meets, as outside the selection.
fn leaving_out(id: &str, when: Condition) -> Normalized {
    let mut d = definition_of(id);
    d.normalization.rules.push(Rule {
        when,
        and: Vec::new(),
        leave_out: Some(LeftOutReason::OutsideSelection),
        kind_of_value: None,
        distance: None,
        basis: "a test".to_owned(),
    });
    normalized_definition(&d)
}

#[test]
fn a_rule_may_leave_out_rows_whose_cell_is_below_a_number() {
    let n = leaving_out(
        "sdss-dr17-distant",
        Condition {
            column: "z".to_owned(),
            below: Some(0.5),
            ..Condition::default()
        },
    );
    // One recorded row lies below 0.5: specObjID 5487829205420103680 at z = 0.4038015, counted as
    // `outside_selection`.
    assert_eq!(n.report.published, 5);
    assert_eq!(left_out(&n, LeftOutReason::OutsideSelection), 1);
}

#[test]
fn a_rule_may_leave_out_rows_whose_cell_is_none_of_its_values() {
    let n = leaving_out(
        "twodfgrs",
        Condition {
            column: "q_z".to_owned(),
            is_not: Some(vec!["4".to_owned(), "5".to_owned()]),
            ..Condition::default()
        },
    );
    // The recorded rows' q_z are 4, 5, 4, 3 and 4.
    assert_eq!(n.report.published, 4);
    assert_eq!(left_out(&n, LeftOutReason::OutsideSelection), 1);
}

#[test]
fn a_condition_gives_exactly_one_test() {
    let text = std::fs::read_to_string("../../sources/twodfgrs.yaml").unwrap();
    for when in [
        r#"{ column: q_z, is: ["4"], below: 3 }"#,
        r"{ column: q_z }",
        r"{ column: q_z, below: .inf }",
        r"{ column: q_z, is_not: [] }",
    ] {
        let rule =
            format!("  rules:\n    - {{ when: {when}, leave_out: outside_selection, basis: b }}\n");
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("twodfgrs.yaml"),
            text.replace("normalization:\n", &format!("normalization:\n{rule}")),
        )
        .unwrap();
        let field = univ_ingest::source::read_all(dir.path()).unwrap_err().field;
        assert!(
            field.starts_with("normalization.rules[0].when"),
            "{when}: {field}"
        );
    }
}

#[test]
fn identifiers_beyond_two_to_the_53_compare_as_text() {
    // SDSS specObjIDs near 7e18 lie 1024 apart as f64: two of them are not the same number.
    let c = Condition::is("specObjID", &["7281375122652682240"]);
    assert!(c.matches("7281375122652682240"));
    assert!(!c.matches("7281375122652682241"));
    // Small numbers still compare as numbers.
    assert!(Condition::is("q_z", &["0"]).matches("0.000"));
}
