//! A source definition records a request for permission while its status is pending, and the
//! clause that limits a source to non-commercial use (F-1).
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

use std::fs;

use univ_ingest::source::{DefinitionError, RedistributionStatus, SourceDefinition};

/// A complete definition whose permission is pending; each test changes one part of it.
const PENDING: &str = r#"
id: test-photo-z
title: "Test photometric redshifts"
provider: ssa
access:
  method: files
  files:
    - { path: "cats/test.fits" }
cuts: ["all rows"]
columns:
  - { column: ra, field: right_ascension, unit: degree }
  - { column: dec, field: declination, unit: degree }
  - { column: zPhoto, field: redshift, unit: dimensionless }
frame: icrs
epoch: 2000.0
citation: "Bilicki et al. 2014, ApJS 210, 9"
reference: { bibcode: "2014ApJS..210....9B", doi: "10.1088/0067-0049/210/1/9" }
terms:
  url: "http://ssa.roe.ac.uk/credits.html"
  quote: "All data retrieved from the SuperCOSMOS Science Archive are subject to the copyright given in this copyright summary."
  credit: "This research has made use of data obtained from the SuperCOSMOS Science Archive."
redistribution:
  status: permission_pending
  review: provisional
  reasoning: "The archive names a copyright summary it does not link."
  permission: { asked_of: "WFAU, mar@roe.ac.uk", asked_on: "2026-11-05" }
scale_domain: cosmic_web
mirrors:
  - "http://ssa.roe.ac.uk/"
normalization:
  redshifts:
    - { column: zPhoto, frame: heliocentric, kind: photometric, basis: "photometric redshifts" }
"#;

const ASKED: &str = r#"  permission: { asked_of: "WFAU, mar@roe.ac.uk", asked_on: "2026-11-05" }
"#;

fn read(text: &str) -> Result<SourceDefinition, DefinitionError> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test-photo-z.yaml");
    fs::write(&path, text).unwrap();
    SourceDefinition::read(&path)
}

fn refused_at(text: &str) -> String {
    read(text).unwrap_err().field
}

#[test]
fn a_pending_source_names_whom_permission_was_asked_of_and_when() {
    let d = read(PENDING).unwrap();
    assert_eq!(
        d.redistribution.status,
        RedistributionStatus::PermissionPending
    );
    let asked = d.redistribution.permission.unwrap();
    assert_eq!(asked.asked_of, "WFAU, mar@roe.ac.uk");
    assert_eq!(asked.asked_on.as_deref(), Some("2026-11-05"));
}

#[test]
fn a_pending_source_not_yet_asked_names_whom_and_no_date() {
    let unsent = PENDING.replace(", asked_on: \"2026-11-05\"", "");
    assert!(!unsent.contains("asked_on"), "{unsent}");
    let d = read(&unsent).unwrap();
    let asked = d.redistribution.permission.unwrap();
    assert_eq!(asked.asked_of, "WFAU, mar@roe.ac.uk");
    assert_eq!(asked.asked_on, None);
    assert!(read(&unsent).unwrap().publishable().is_ok());
}

#[test]
fn a_pending_source_is_publishable_as_the_owner_chose() {
    assert!(read(PENDING).unwrap().publishable().is_ok());
}

#[test]
fn a_pending_status_without_its_request_is_refused() {
    assert_eq!(
        refused_at(&PENDING.replace(ASKED, "")),
        "redistribution.permission"
    );
}

#[test]
fn a_request_is_refused_beside_any_other_status() {
    let permitted = PENDING.replace(
        "status: permission_pending",
        "status: permitted_with_credit",
    );
    assert_eq!(refused_at(&permitted), "redistribution.permission");
}

#[test]
fn a_request_names_someone_and_a_calendar_date() {
    let nobody = PENDING.replace("asked_of: \"WFAU, mar@roe.ac.uk\"", "asked_of: \" \"");
    assert_eq!(refused_at(&nobody), "redistribution.permission.asked_of");
    for date in ["2026-13-05", "5 Nov 2026", "2026-11-5", "2026-11-05T00:00"] {
        let undated = PENDING.replace("asked_on: \"2026-11-05\"", &format!("asked_on: \"{date}\""));
        assert_eq!(
            refused_at(&undated),
            "redistribution.permission.asked_on",
            "{date}"
        );
    }
}

#[test]
fn a_non_commercial_source_quotes_the_clause_that_limits_it() {
    let clause = "Tabular data coming from AAS journals are under CC-BY-NC licence";
    let text = PENDING.replace(
        "  credit: \"This research",
        &format!("  non_commercial: \"{clause}\"\n  credit: \"This research"),
    );
    let d = read(&text).unwrap();
    assert_eq!(d.terms.non_commercial.as_deref(), Some(clause));
    assert!(read(PENDING).unwrap().terms.non_commercial.is_none());
    let blank = PENDING.replace(
        "  credit: \"This research",
        "  non_commercial: \" \"\n  credit: \"This research",
    );
    assert_eq!(refused_at(&blank), "terms.non_commercial");
}
