//! Normalization keeps what each source published beside its normalized form, and leaves no
//! row out without counting it under a reason (F-4). Tests that need a case no recording holds,
//! such as a zero error, change one cell of a recorded row and say so.
#![expect(clippy::unwrap_used, reason = "a failed unwrap is the test failing")]

mod common;

use std::collections::BTreeMap;

use common::{definition_of, entry, every_table, fixture_of, normalize_body, normalized_fixture};
use univ_ingest::normalize::FixApplied;
use univ_ingest::source::{LeftOutReason, ScaleDomain, SourceDefinition};
use univ_model::distance::{DistanceKind, Method};
use univ_model::entry::{CatalogueEntry, PositionUncertainty};
use univ_model::units::{Dimension, Unit};
use univ_model::value::{KindOfValue, Uncertainty, Value};

/// The recorded answer of `id` with the cell of `column` in data row `row` (from 0) set to
/// `cell`, for a case no recording holds.
fn edited(id: &str, edits: &[(usize, &str, &str)]) -> (SourceDefinition, Vec<u8>) {
    let definition = definition_of(id);
    let text = std::fs::read_to_string(fixture_of(&definition)).unwrap();
    let separator = if text.contains('\t') { '\t' } else { ',' };
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let content: Vec<usize> = (0..lines.len())
        .filter(|i| !lines[*i].starts_with('#') && !lines[*i].trim().is_empty())
        .collect();
    let header: Vec<String> = lines[content[0]]
        .split(separator)
        .map(|c| c.trim().to_owned())
        .collect();
    let first_row = if separator == '\t' { 3 } else { 1 };
    for (row, column, cell) in edits {
        let at = content[first_row + row];
        let index = header.iter().position(|h| h == column).unwrap();
        let mut cells: Vec<String> = lines[at].split(separator).map(str::to_owned).collect();
        (*cell).clone_into(&mut cells[index]);
        lines[at] = cells.join(&separator.to_string());
    }
    (definition, (lines.join("\n") + "\n").into_bytes())
}

/// Every value of an entry with the name of the field it fills.
fn values(e: &CatalogueEntry) -> Vec<(&'static str, &Value)> {
    let mut all = Vec::new();
    all.extend(e.velocity.iter().map(|v| ("velocity", v)));
    all.extend(e.parallax.iter().map(|v| ("parallax", v)));
    if let Some(pm) = &e.proper_motion {
        all.extend([("proper motion", &pm.ra), ("proper motion", &pm.dec)]);
    }
    all.extend(e.magnitude.iter().map(|m| ("magnitude", &m.value)));
    if let Some(sg) = &e.supergalactic_velocity {
        all.extend([("sg", &sg.x), ("sg", &sg.y), ("sg", &sg.z)]);
    }
    if let Some(PositionUncertainty::Published(errors)) =
        e.position.as_ref().map(|p| &p.uncertainty)
    {
        all.extend([
            ("position error", &errors.ra),
            ("position error", &errors.dec),
        ]);
    }
    all
}

#[test]
fn each_position_is_stored_in_icrs_with_the_epoch_of_its_source() {
    for (d, n) in every_table() {
        for e in &n.entries {
            let Some(p) = &e.position else { continue };
            assert_eq!(p.icrs.epoch, d.epoch, "{}", d.id);
            assert_eq!(p.native.frame, d.frame, "{}", d.id);
            assert!((0.0..360.0).contains(&p.icrs.ra) && p.icrs.dec.abs() <= 90.0);
        }
    }
}

#[test]
fn each_native_value_keeps_its_published_unit_beside_the_normalized_value() {
    let gcns = normalized_fixture("gcns");
    let d = &entry(&gcns, "530861741656374272").distances[0];
    assert_eq!(
        (d.native.value, d.native.unit),
        (0.097_26, Unit::Kiloparsec)
    );
    assert_eq!(d.normalized.unit, Unit::Parsec);
    for (definition, n) in every_table() {
        let unit_of = |column: &str| {
            definition
                .columns
                .iter()
                .find(|c| c.column == column)
                .unwrap()
                .unit
        };
        for e in &n.entries {
            if let Some(m) = &e.magnitude {
                assert_eq!(m.value.native.unit, unit_of(&m.band), "{}", definition.id);
            }
            for (field, v) in values(e) {
                let native = v.native.unit.dimension();
                assert_eq!(
                    native,
                    v.normalized.unit.dimension(),
                    "{} {field}",
                    definition.id
                );
            }
        }
    }
    let hip = normalized_fixture("hipparcos-2007");
    let plx = entry(&hip, "1").parallax.unwrap();
    assert_eq!(
        (plx.native.value, plx.native.unit),
        (4.55, Unit::Milliarcsecond)
    );
}

#[test]
fn normalized_values_use_degrees_km_s_parsecs_for_stars_and_megaparsecs_beyond() {
    for (d, n) in every_table() {
        let length = if d.scale_domain == ScaleDomain::Stars {
            Unit::Parsec
        } else {
            Unit::Megaparsec
        };
        for e in &n.entries {
            for distance in &e.distances {
                assert_eq!(distance.normalized.unit, length, "{}", d.id);
            }
            for (field, v) in values(e) {
                let expected = match v.native.unit.dimension() {
                    Dimension::Angle => Unit::Degree,
                    Dimension::Velocity => Unit::KilometrePerSecond,
                    Dimension::ProperMotion => Unit::MilliarcsecondPerYear,
                    Dimension::Magnitude => Unit::Magnitude,
                    other => panic!("{field} in {other:?}"),
                };
                assert_eq!(v.normalized.unit, expected, "{} {field}", d.id);
            }
        }
    }
}

#[test]
fn each_value_is_classified_by_its_kind_of_value() {
    let bj = normalized_fixture("bailer-jones-2021");
    assert_eq!(
        entry(&bj, "139396191545402112").distances[0].kind_of_value,
        KindOfValue::Estimated
    );
    let gaia = normalized_fixture("gaia-dr3-bright");
    assert_eq!(
        entry(&gaia, "139396191545402112").parallax.unwrap().kind,
        KindOfValue::Measured
    );
    // 2M++ row 1 with Cln set to 1: a cloned redshift is assigned, not measured.
    let (d, body) = edited("twompp-catalogue", &[(0, "Cln", "1")]);
    let n = normalize_body(&d, &body);
    let cloned = entry(&n, "01413509-8920041");
    assert_eq!(cloned.redshifts[0].kind_of_value, KindOfValue::Estimated);
    assert_eq!(cloned.velocity.unwrap().kind, KindOfValue::Estimated);
    assert_eq!(
        entry(&n, "23085574-8859141").redshifts[0].kind_of_value,
        KindOfValue::Measured
    );
    // McConnachie row 2 with u_(m-M) set to ":l": a distance assumed to be M31's.
    let (d, body) = edited("mcconnachie-2012", &[(1, "u_(m-M)", ":l")]);
    let n = normalize_body(&d, &body);
    assert_eq!(
        entry(&n, "Canis Major").distances[0].kind_of_value,
        KindOfValue::Estimated
    );
}

#[test]
fn each_uncertainty_is_typed_by_the_form_its_source_published() {
    let form = |u: Uncertainty| match u {
        Uncertainty::NoneReported => "none reported",
        Uncertainty::Symmetric { .. } => "symmetric",
        Uncertainty::Asymmetric { .. } => "asymmetric",
        Uncertainty::UpperLimit | Uncertainty::LowerLimit => "limit",
        Uncertainty::CredibleRegion { .. } | Uncertainty::CredibleInterval { .. } => "credible",
    };
    let mut forms = BTreeMap::new();
    for (d, n) in every_table() {
        for e in &n.entries {
            for distance in &e.distances {
                forms.insert(form(distance.native.uncertainty), d.id.to_string());
            }
        }
    }
    assert_eq!(
        forms.keys().copied().collect::<Vec<_>>(),
        ["asymmetric", "credible", "none reported", "symmetric"]
    );
    assert_eq!(forms["asymmetric"], "mcconnachie-2012");
}

#[test]
fn a_value_published_without_an_error_is_marked_none_reported() {
    let ungc = normalized_fixture("updated-nearby-galaxies");
    let d = &entry(&ungc, "WLM").distances[0];
    assert_eq!(d.native.uncertainty, Uncertainty::NoneReported);
    // 2MRS row 1 with e_cz blank.
    let (definition, body) = edited("twomrs", &[(0, "e_cz", "   ")]);
    let n = normalize_body(&definition, &body);
    let m31 = entry(&n, "00424433+4116074");
    assert_eq!(
        m31.velocity.unwrap().native.uncertainty,
        Uncertainty::NoneReported
    );
    assert_eq!(
        m31.redshifts[0].cmb.value.uncertainty,
        Uncertainty::NoneReported
    );
    // 2M++ row 1 with e_HV = 0, which its ReadMe gives when the error was not measured.
    let (definition, body) = edited("twompp-catalogue", &[(0, "e_HV", "     0")]);
    let n = normalize_body(&definition, &body);
    let g = entry(&n, "01413509-8920041");
    assert_eq!(
        g.velocity.unwrap().native.uncertainty,
        Uncertainty::NoneReported
    );
}

#[test]
fn the_kinds_of_distance_are_the_six_the_specification_names() {
    let kinds: Vec<DistanceKind> = serde_norway::from_str(
        "[parallax, geometric, photogeometric, primary_indicator, secondary_indicator, spectroscopic_redshift, photometric_redshift]",
    )
    .unwrap();
    assert_eq!(
        kinds.len(),
        7,
        "geometric and photogeometric are one kind of two variants"
    );
    assert!(serde_norway::from_str::<DistanceKind>("tully_fisher").is_err());
    let mut found: Vec<DistanceKind> = every_table()
        .iter()
        .flat_map(|(_, n)| {
            n.entries
                .iter()
                .flat_map(|e| e.distances.iter().map(|d| d.kind))
        })
        .collect();
    found.sort_by_key(|k| format!("{k:?}"));
    found.dedup();
    assert_eq!(
        found,
        [
            DistanceKind::Geometric,
            DistanceKind::Photogeometric,
            DistanceKind::PrimaryIndicator,
            DistanceKind::SecondaryIndicator
        ]
    );
}

#[test]
fn the_method_names_the_indicator() {
    let ungc = normalized_fixture("updated-nearby-galaxies");
    assert_eq!(
        entry(&ungc, "UGC12894").distances[0].method,
        Method::TullyFisher
    );
    assert_eq!(
        entry(&ungc, "WLM").distances[0].method,
        Method::TipOfTheRedGiantBranch
    );
    let cf4 = normalized_fixture("cosmicflows-4");
    let Method::Mixed(methods) = &entry(&cf4, "12").distances[0].method else {
        panic!("mixed")
    };
    for indicator in [
        Method::Cepheids,
        Method::TipOfTheRedGiantBranch,
        Method::Masers,
        Method::TypeIaSupernovae,
        Method::SurfaceBrightnessFluctuations,
        Method::TullyFisher,
        Method::FundamentalPlane,
    ] {
        assert!(methods.contains(&indicator), "{indicator:?}");
    }
    // UNGC row 1 with f_Dist set to a code its ReadMe does not define.
    let (d, body) = edited("updated-nearby-galaxies", &[(0, "f_Dist", "XYZ ")]);
    let n = normalize_body(&d, &body);
    assert_eq!(n.report.left_out[&LeftOutReason::UnknownMethodCode], 1);
}

#[test]
fn a_distance_in_h_inverse_mpc_or_for_a_stated_hubble_constant_keeps_that_constant() {
    let dir = tempfile::tempdir().unwrap();
    let access = "  method: tap\n  query: q\n  chunks: 1";
    let text = std::fs::read_to_string({
        common::definition(&dir, access, "https://example.org");
        dir.path().join("test-galaxies.yaml")
    })
    .unwrap()
    .replace(
        "columns:\n",
        "columns:\n  - { column: id, field: identifier, unit: dimensionless }\n  - { column: d, field: distance, unit: megaparsec_over_h }\n  - { column: d73, field: distance, unit: megaparsec }\n",
    ) + "normalization:\n  distances:\n    - { value: d, kind: secondary_indicator, method: tully_fisher, measure: unspecified, kind_of_value: measured, basis: b }\n    - { value: d73, kind: secondary_indicator, method: tully_fisher, measure: unspecified, kind_of_value: estimated, hubble_constant: 73, basis: b }\n";
    let path = dir.path().join("test-galaxies.yaml");
    std::fs::write(&path, text).unwrap();
    let definition = SourceDefinition::read(&path).unwrap();
    let n = normalize_body(&definition, b"id,ra,dec,d,d73\nA,10,20,70.5,8.0\n");
    let [scaled, at_73] = &entry(&n, "A").distances[..] else {
        panic!("two distances")
    };
    assert_eq!(
        (scaled.native.value, scaled.native.unit),
        (70.5, Unit::MegaparsecOverH)
    );
    assert_eq!(
        (scaled.normalized.value, scaled.hubble_constant),
        (70.5, Some(100.0))
    );
    assert_eq!(
        (at_73.native.value, at_73.hubble_constant),
        (8.0, Some(73.0))
    );
}

#[test]
fn a_distance_modulus_is_kept_as_published_beside_the_distance_derived_from_it() {
    let cf4 = normalized_fixture("cosmicflows-4");
    let d = &entry(&cf4, "16").distances[0];
    // Row 3: DMzp 34.655 ± 0.450, so 10^(34.655/5 + 1) pc = 85.3 Mpc.
    assert_eq!(d.modulus().unwrap().value, 34.655);
    assert_eq!(
        d.modulus().unwrap().uncertainty,
        Uncertainty::Symmetric { sigma: 0.45 }
    );
    assert!(
        (d.normalized.value - 85.310_6).abs() < 1e-3,
        "{}",
        d.normalized.value
    );
    let mcc = normalized_fixture("mcconnachie-2012");
    let d = &entry(&mcc, "Sagittarius dSph").distances[0];
    // Row 3: (m-M) 17.10, D 26 kpc as published beside it.
    assert_eq!(
        (d.modulus().unwrap().value, d.also_published.unwrap().value),
        (17.1, 26.0)
    );
}

#[test]
fn no_entry_is_left_out_without_a_reason() {
    for (d, n) in every_table() {
        let rows = common::count_rows(&d);
        assert_eq!(
            n.report.published + n.report.left_out_total(),
            rows,
            "{}",
            d.id
        );
        assert_eq!(n.entries.len() as u64, n.report.published);
    }
    // 2MRS rows 1-4 each broken in one way; row 5 kept.
    let (d, body) = edited(
        "twomrs",
        &[
            (0, "ID", " "),
            (1, "_DEJ2000", ""),
            (2, "_DEJ2000", "+95.0"),
            (3, "cz", "fast"),
        ],
    );
    let n = normalize_body(&d, &body);
    let expected = BTreeMap::from([
        (LeftOutReason::MissingIdentifier, 1),
        (LeftOutReason::MissingPosition, 1),
        (LeftOutReason::DeclinationOutOfRange, 1),
        (LeftOutReason::UnparsableNumber, 1),
    ]);
    assert_eq!((n.report.published, &n.report.left_out), (1, &expected));
}

#[test]
fn a_simulated_galaxy_and_an_unsuitable_redshift_are_left_out_and_counted() {
    // 2M++ row 2 with Ref set to "zoa", a fake galaxy filling the Zone of Avoidance.
    let (d, body) = edited("twompp-catalogue", &[(1, "Ref", "zoa")]);
    let n = normalize_body(&d, &body);
    assert_eq!(
        n.report.left_out,
        BTreeMap::from([(LeftOutReason::SimulatedObject, 1)])
    );
    // 6dFGS rows 1 and 2 with e_cz = 0, the ReadMe's mark of an unsuitable redshift.
    let (d, body) = edited("sixdfgs-dr3", &[(0, "e_cz", "  0"), (1, "e_cz", "  0")]);
    let n = normalize_body(&d, &body);
    assert_eq!(
        n.report.left_out,
        BTreeMap::from([(LeftOutReason::UnsuitableRedshift, 2)])
    );
    assert_eq!(n.report.fixes[0].entries_changed, 2);
}

#[test]
fn cosmicflows_sgx_sgy_and_sgz_are_read_as_km_s_though_vizier_labels_them_mpc() {
    let d = definition_of("cosmicflows-4");
    let sgx = d.columns.iter().find(|c| c.column == "SGX").unwrap();
    assert_eq!(
        sgx.unit,
        Unit::Megaparsec,
        "the definition keeps VizieR's label"
    );
    let n = normalized_fixture("cosmicflows-4");
    // Row 2, 1PGC 14: SGX 5942, SGY -9967, SGZ 3194.
    let sg = entry(&n, "14").supergalactic_velocity.unwrap();
    assert_eq!(
        (sg.x.native.value, sg.x.native.unit),
        (5_942.0, Unit::KilometrePerSecond)
    );
    assert_eq!(
        (sg.y.normalized.value, sg.z.normalized.value),
        (-9_967.0, 3_194.0)
    );
    assert!(
        entry(&n, "14")
            .distances
            .iter()
            .all(|d| d.native.unit == Unit::Magnitude)
    );
}

#[test]
fn the_report_lists_each_fix_applied_with_the_entries_it_changed() {
    let n = normalized_fixture("cosmicflows-4");
    assert_eq!(n.report.fixes.len(), 1);
    assert_eq!(n.report.fixes[0].entries_changed, 5);
    assert!(
        n.report.fixes[0]
            .change
            .starts_with("SGX, SGY and SGZ are read as km/s")
    );
    // 2M++ rows 1, 3 and 4 with e_HV = 0, and row 4 also a fake ZoA galaxy, left out: the
    // fix changed two entries.
    let (d, body) = edited(
        "twompp-catalogue",
        &[
            (0, "e_HV", "0"),
            (2, "e_HV", "0"),
            (3, "e_HV", "0"),
            (3, "Ref", "zoa"),
        ],
    );
    let n = normalize_body(&d, &body);
    assert_eq!(
        n.report.fixes,
        [FixApplied {
            change: d.fixes[0].change.clone(),
            entries_changed: 2
        }]
    );
    // The Hipparcos fix is expressed by its columns' unit, so the normalizer applies none.
    assert_eq!(normalized_fixture("hipparcos-2007").report.fixes, []);
}
