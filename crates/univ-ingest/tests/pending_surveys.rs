//! The galaxy surveys of release 10 whose permission is pending normalize from their recorded
//! files: 2MPZ's photometric redshifts with the catalogue's error of 0.015, read from its
//! prepared file; GAMA DR4's own redshifts, without other surveys' rows, stars or poor
//! redshifts; and the photometric redshifts of WISE × `SuperCOSMOS`, read from its gzipped CSV
//! with the catalogue's scatter of 0.033 (1 + z) (normalize-records).
#![expect(clippy::unwrap_used, reason = "a failed unwrap is the test failing")]

mod common;

use std::path::PathBuf;

use common::{definition_of, entry};
use univ_ingest::normalize::{Normalized, normalize};
use univ_ingest::source::LeftOutReason;
use univ_model::entry::{CatalogueEntry, RedshiftKind};
use univ_model::redshift::{CmbCorrection, DipoleModel};
use univ_model::value::{KindOfValue, Uncertainty};

fn fixture(provider: &str, path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(provider)
        .join(path)
}

fn normalized(id: &str, file: PathBuf) -> Normalized {
    let correction = CmbCorrection::new(DipoleModel::Planck2018);
    normalize(&definition_of(id), &[file], &correction).unwrap()
}

/// The entry's redshifts as (kind, measured value).
fn redshifts(e: &CatalogueEntry) -> Vec<(RedshiftKind, f64)> {
    e.redshifts
        .iter()
        .map(|r| (r.kind, r.cmb.measured.value))
        .collect()
}

#[test]
fn twompz_reads_its_prepared_file_with_the_catalogues_photometric_error() {
    let n = normalized("twompz", fixture("wfau-ssa", "twompz.csv.gz"));
    assert_eq!((n.report.published, n.report.left_out_total()), (8, 0));
    // zSpec is not taken: every entry holds its photometric redshift alone.
    for e in &n.entries {
        let kinds: Vec<RedshiftKind> = redshifts(e).into_iter().map(|(k, _)| k).collect();
        assert_eq!(
            kinds,
            [RedshiftKind::Photometric],
            "{}",
            e.designation.identifier
        );
    }
    // twomassID 3, zPhoto 0.080838, with the catalogue's error of 0.015 whatever its redshift.
    let r = &entry(&n, "14351759-2955035").redshifts[0];
    assert_eq!(r.kind_of_value, KindOfValue::Estimated);
    assert!((r.cmb.measured.value - 0.080_838).abs() < 1e-9);
    let Uncertainty::Symmetric { sigma } = r.cmb.measured.uncertainty else {
        panic!("{:?}", r.cmb.measured.uncertainty)
    };
    assert!((sigma - 0.015).abs() < 1e-12, "{sigma}");
    // twomassID 15144: a negative zPhoto, kept as published.
    let (_, z) = redshifts(entry(&n, "16330262-2316180"))[0];
    assert!((z + 0.008_322).abs() < 1e-9, "{z}");
}

#[test]
fn gama_keeps_its_own_redshifts_of_quality_three_or_more_above_the_stars() {
    let n = normalized(
        "gama-dr4",
        fixture("gama-survey", "gama-dr4/SpecObjv27.fits"),
    );
    // The seven recorded rows: CATAID 6802, 6803 and 6804 (GAMA, NQ 4), 6658 (SDSS), 6648
    // (2dFGRS), 6924 (GAMA, NQ 4, z = -0.00003, a star) and 7132 (GAMA, NQ 1).
    assert_eq!(n.report.published, 3);
    let left = |r| n.report.left_out.get(&r).copied().unwrap_or(0);
    assert_eq!(
        left(LeftOutReason::OutsideSelection),
        3,
        "two surveys' rows and a star"
    );
    assert_eq!(left(LeftOutReason::UnsuitableRedshift), 1, "NQ 1");
    let ids: Vec<&str> = n
        .entries
        .iter()
        .map(|e| &*e.designation.identifier)
        .collect();
    assert_eq!(ids, ["6802", "6803", "6804"]);
    let r = &n.entries[0].redshifts[0];
    assert_eq!(r.kind, RedshiftKind::Spectroscopic);
    assert!((r.cmb.measured.value - 0.050_54).abs() < 1e-6);
    assert_eq!(r.cmb.measured.uncertainty, Uncertainty::NoneReported);
}

#[test]
fn wise_supercosmos_reads_its_gzipped_csv_with_the_catalogues_scatter() {
    let n = normalized(
        "wise-supercosmos",
        fixture("wfau-ssa", "wise-supercosmos/wiseScosPhotoz160708.csv.gz"),
    );
    assert_eq!((n.report.published, n.report.left_out_total()), (6, 0));
    // Row 1: J000253.16-021232.9, zPhoto_Corr 0.240063.
    let r = &entry(&n, "J000253.16-021232.9").redshifts[0];
    assert_eq!(r.kind, RedshiftKind::Photometric);
    assert_eq!(r.kind_of_value, KindOfValue::Estimated);
    let z = r.cmb.measured;
    assert!((z.value - 0.240_063).abs() < 1e-9);
    let Uncertainty::Symmetric { sigma } = z.uncertainty else {
        panic!("{z:?}")
    };
    assert!((sigma - 0.033 * 1.240_063).abs() < 1e-12, "{sigma}");
}
