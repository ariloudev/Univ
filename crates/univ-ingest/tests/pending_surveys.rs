//! The galaxy surveys of release 10 whose permission is pending normalize from their recorded
//! files: 2MPZ's photometric redshifts with the catalogue's error of 0.015 (normalize-records).
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a failed unwrap or match is the test failing"
)]

mod common;

use std::path::PathBuf;

use common::definition_of;
use univ_ingest::normalize::{Normalized, normalize};
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

fn entry<'a>(n: &'a Normalized, identifier: &str) -> &'a CatalogueEntry {
    n.entries
        .iter()
        .find(|e| &*e.designation.identifier == identifier)
        .unwrap_or_else(|| panic!("no entry {identifier}"))
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
