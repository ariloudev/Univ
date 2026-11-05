//! What a distance is: its measure, the method a row's own cells give it, and the interval a
//! modulus error spans, read from the recorded answers.

mod common;

use common::{entry, normalized_fixture};
use univ_ingest::source::SourceDefinition;
use univ_model::distance::{DistanceKind, Measure, Method};
use univ_model::value::Uncertainty;

#[test]
fn cosmicflows_groups_without_a_five_method_distance_are_direct_calibrations() {
    // ReadMe table3 Note (1): where no five-method measurement exists, DMzp is the direct
    // calibration value; such rows publish DMav = e_DMav = 0.
    let n = normalized_fixture("cosmicflows-4");
    // Row 4: 35 29.730 0.100 0.000 0.000: a calibrator group.
    let d = &entry(&n, "35").distances[0];
    assert_eq!(d.kind, DistanceKind::PrimaryIndicator);
    assert_eq!(
        d.method,
        Method::Mixed(vec![
            Method::TipOfTheRedGiantBranch,
            Method::Cepheids,
            Method::Masers
        ])
    );
    assert_eq!(d.native.value, 29.73);
    // Row 1: 12 34.995 0.410 35.142 0.410: a five-method average on the calibrators' scale.
    let d = &entry(&n, "12").distances[0];
    assert_eq!(d.kind, DistanceKind::SecondaryIndicator);
    assert!(matches!(&d.method, Method::Mixed(m) if m.len() == 8));
    assert_eq!(entry(&n, "12").flags.get("DMav").unwrap(), "35.142");
}

#[test]
fn a_distance_takes_the_measure_its_definition_states_and_a_modulus_gives_a_luminosity_distance() {
    let cf4 = normalized_fixture("cosmicflows-4");
    assert_eq!(entry(&cf4, "12").distances[0].measure, Measure::Luminosity);
    let mcc = normalized_fixture("mcconnachie-2012");
    let d = &entry(&mcc, "Sagittarius dSph").distances[0];
    assert_eq!(d.measure, Measure::Luminosity);
    let gcns = normalized_fixture("gcns");
    let d = &entry(&gcns, "530861741656374272").distances[0];
    assert_eq!(d.measure, Measure::Euclidean);

    let dir = tempfile::tempdir().unwrap();
    let text = std::fs::read_to_string(common::sources_dir().join("cosmicflows-4.yaml"))
        .unwrap()
        .replace("measure: luminosity", "measure: comoving");
    let path = dir.path().join("cosmicflows-4.yaml");
    std::fs::write(&path, text).unwrap();
    let e = SourceDefinition::read(&path).unwrap_err();
    assert_eq!(e.field, "normalization.distances[0].measure");
}

#[test]
fn a_symmetric_modulus_error_spans_an_asymmetric_distance_interval_beside_it() {
    let n = normalized_fixture("cosmicflows-4");
    // Row 1: DMzp 34.995 ± 0.410: d·10^(−0.082) to d·10^(+0.082).
    let d = &entry(&n, "12").distances[0];
    assert_eq!(d.native.uncertainty, Uncertainty::Symmetric { sigma: 0.41 });
    let Uncertainty::Asymmetric { minus, plus } = d.normalized.uncertainty else {
        panic!("asymmetric, not {:?}", d.normalized.uncertainty)
    };
    let v = d.normalized.value;
    assert!((v - minus - v * 10f64.powf(-0.082)).abs() < 1e-9 * v);
    assert!((v + plus - v * 10f64.powf(0.082)).abs() < 1e-9 * v);
}
