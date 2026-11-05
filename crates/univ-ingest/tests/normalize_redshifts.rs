//! Each redshift is kept as its source published it, and its CMB-frame value derived with the
//! Planck 2018 dipole, the derivation recorded with it (F-4).

mod common;

use common::{definition_of, entry, every_table, fixture_of, normalize_body, normalized_fixture};
use univ_ingest::source::{Condition, Fix, FixAction};
use univ_model::redshift::DipoleModel;
use univ_model::value::Uncertainty;

#[test]
fn a_heliocentric_redshift_is_kept_as_measured() {
    let sdss = normalized_fixture("sdss-dr17-galaxies");
    // Row 2: 375050326987794432, z 0.08733442 ± 2.054854E-05.
    let r = &entry(&sdss, "375050326987794432").redshifts[0];
    assert_eq!(r.cmb.measured.value, 0.087_334_42);
    assert_eq!(
        r.cmb.measured.uncertainty,
        Uncertainty::Symmetric {
            sigma: 2.054_854e-5
        }
    );
    assert_eq!(r.velocity, None);
}

#[test]
fn the_cmb_redshift_uses_the_planck_2018_dipole() {
    for (_, n) in every_table() {
        for r in n.entries.iter().flat_map(|e| &e.redshifts) {
            assert_eq!(r.cmb.dipole, DipoleModel::Planck2018);
        }
    }
    let dipole = DipoleModel::Planck2018.dipole();
    assert_eq!(
        (dipole.speed, dipole.l, dipole.b),
        (369.82, 264.021, 48.253)
    );
}

#[test]
fn the_cmb_derivation_is_recorded_with_the_redshift_it_was_derived_from() {
    let twomrs = normalized_fixture("twomrs");
    // Row 3: 09553318+6903549, cz -34 ± 4 km/s.
    let r = &entry(&twomrs, "09553318+6903549").redshifts[0];
    assert_eq!(r.velocity.as_deref().unwrap().value, -34.0);
    assert_eq!(r.cmb.measured.value, -34.0 / 299_792.458);
    assert_ne!(r.cmb.value.value, r.cmb.measured.value);
    let yaml = serde_norway::to_string(r).unwrap();
    assert!(
        yaml.contains("measured:") && yaml.contains("dipole: planck2018"),
        "{yaml}"
    );
}

#[test]
fn a_fix_drops_a_value_on_the_rows_it_names_keeps_the_entry_and_counts_the_entries_it_changed() {
    // eRASS1: Z_LAMBDA belongs to a foreground or background cluster on lit_z rows (Kluge et
    // al. 2024, Appendix A), so it and its error are dropped there.
    let erass1 = definition_of("erass1-clusters");
    let drops: Vec<_> = erass1
        .fixes
        .iter()
        .filter_map(|f| match &f.action {
            Some(FixAction::DropWhen { columns, when }) => Some((columns, when)),
            _ => None,
        })
        .collect();
    let [(columns, when)] = &drops[..] else {
        panic!("one drop fix")
    };
    assert_eq!(*columns, &["Z_LAMBDA", "Z_LAMBDA_ERR"]);
    assert_eq!(
        (when.column.as_str(), when.is.as_deref()),
        ("BEST_Z_TYPE", Some(&["lit_z".to_owned()][..]))
    );

    // The same action on the recorded 2M++ rows, dropping Ksmag where Cln is x: row 1 has a
    // magnitude, row 2 none to drop, row 3 is left out for its blank
    // identifier; row 4 is untouched.
    let mut definition = definition_of("twompp-catalogue");
    definition.fixes.push(Fix {
        defect: "a test defect".into(),
        change: "Ksmag is dropped where Cln is x".into(),
        evidence: "a test".into(),
        action: Some(FixAction::DropWhen {
            columns: vec!["Ksmag".into()],
            when: Condition::is("Cln", &["x"]),
        }),
    });
    let text = std::fs::read_to_string(fixture_of(&definition)).unwrap();
    let mut row = 0;
    let lines: Vec<String> = text
        .lines()
        .map(|line| {
            let mut cells: Vec<&str> = line.split('\t').collect();
            let data = !line.starts_with('#')
                && cells.len() == 9
                && cells[3].trim().parse::<f64>().is_ok();
            if data {
                row += 1;
                match row {
                    1 => cells[8] = "x",
                    2 => (cells[5], cells[8]) = ("", "x"),
                    3 => (cells[2], cells[8]) = ("", "x"),
                    _ => {}
                }
            }
            cells.join("\t")
        })
        .collect();
    let n = normalize_body(&definition, (lines.join("\n") + "\n").as_bytes());
    let counts: Vec<(&str, u64)> = n
        .report
        .fixes
        .iter()
        .map(|f| (f.change.as_str(), f.entries_changed))
        .collect();
    assert_eq!(counts.len(), 2, "{counts:?}");
    assert_eq!(counts[1], ("Ksmag is dropped where Cln is x", 1));
    assert_eq!(n.report.left_out_total(), 1);
    assert_eq!(entry(&n, "01413509-8920041").magnitude, None);
    assert!(entry(&n, "23085574-8859141").magnitude.is_none());
    assert!(entry(&n, "15433127-8740427").magnitude.is_some());
}
