//! A source definition may give the form its catalogue writes a designation in and what each
//! value of its flag columns means, each cited, and is refused where either cannot be used
//! (define-a-source), and may assign its rows to researched structures by a cited flag column.
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

use std::fs;

use univ_ingest::source::{DefinitionError, SourceDefinition, check_structures};

/// A complete definition with a flag column and neither a form nor meanings; each test adds
/// one.
const PLAIN: &str = r#"
id: test-stars
title: "Test stars"
provider: vizier
access:
  method: files
  files:
    - { path: "cats/test.fits" }
cuts: ["all rows"]
columns:
  - { column: HIP, field: identifier, unit: dimensionless }
  - { column: RArad, field: right_ascension, unit: degree }
  - { column: DErad, field: declination, unit: degree }
  - { column: Sn, field: quality, unit: dimensionless }
frame: icrs
epoch: 1991.25
citation: "van Leeuwen 2007, A&A 474, 653"
reference: { bibcode: "2007A&A...474..653V", doi: "10.1051/0004-6361:20078357" }
terms:
  url: "https://cds.unistra.fr/vizier-org/licences_vizier.html"
  quote: "The data retrieved with VizieR are free of usage in a scientific context."
  credit: "This research has made use of the VizieR catalogue access tool, CDS, Strasbourg, France."
redistribution:
  status: permitted_with_credit
  review: provisional
  reasoning: "VizieR's terms permit scientific use with credit."
scale_domain: stars
mirrors:
  - "https://vizier.cds.unistra.fr/"
"#;

const FORM: &str = "designation: { form: \"HIP {identifier}\", cite: \"ReadMe of I/311, Byte-by-byte description, HIP\" }\n";

const FLAGS: &str = r#"flags:
  Sn:
    cite: "ReadMe of I/311, Note 1"
    values:
      "5": "a five-parameter solution"
      "7": "a seven-parameter solution"
"#;

fn read(text: &str) -> Result<SourceDefinition, DefinitionError> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test-stars.yaml");
    fs::write(&path, text).unwrap();
    SourceDefinition::read(&path)
}

fn refused(text: &str) -> DefinitionError {
    read(text).unwrap_err()
}

#[test]
fn a_definition_may_give_its_designation_form_with_its_citation() {
    let d = read(&format!("{PLAIN}{FORM}")).unwrap();
    let form = d.designation.unwrap();
    assert_eq!(
        univ_model::api::sources::write_in(&form.form, "32754"),
        "HIP 32754"
    );
    assert_eq!(form.cite, "ReadMe of I/311, Byte-by-byte description, HIP");
    assert!(read(PLAIN).unwrap().designation.is_none());
}

#[test]
fn a_definition_may_give_the_meaning_of_each_value_of_a_flag_column_with_its_citation() {
    let d = read(&format!("{PLAIN}{FLAGS}")).unwrap();
    let sn = &d.flags["Sn"];
    assert_eq!(sn.values["5"], "a five-parameter solution");
    assert_eq!(sn.cite, "ReadMe of I/311, Note 1");
    assert!(read(PLAIN).unwrap().flags.is_empty());
}

#[test]
fn a_designation_form_that_does_not_hold_the_identifier_exactly_once_is_refused() {
    // A form of the identifier alone would read every name looked up as a designation.
    for form in [
        "HIP",
        "HIP {identifier} {identifier}",
        "HIP {id}",
        "{identifier}",
        " {identifier} ",
    ] {
        let text = format!("{PLAIN}designation: {{ form: \"{form}\", cite: \"ReadMe\" }}\n");
        let e = refused(&text);
        assert_eq!(e.field, "designation.form", "{form}");
        assert!(e.to_string().contains("test-stars.yaml"), "{e}");
    }
    let uncited = format!("{PLAIN}designation: {{ form: \"HIP {{identifier}}\", cite: \" \" }}\n");
    assert_eq!(refused(&uncited).field, "designation.cite");
}

#[test]
fn a_flag_meaning_for_a_column_not_taken_as_a_flag_is_refused() {
    let not_a_flag = format!("{PLAIN}{}", FLAGS.replace("  Sn:", "  HIP:"));
    let e = refused(&not_a_flag);
    assert_eq!(e.field, "flags.HIP");
    assert!(e.to_string().contains("field quality"), "{e}");
    let not_taken = format!("{PLAIN}{}", FLAGS.replace("  Sn:", "  So:"));
    assert_eq!(refused(&not_taken).field, "flags.So");
    let blank = format!(
        "{PLAIN}{}",
        FLAGS.replace("\"a five-parameter solution\"", "\" \"")
    );
    assert_eq!(refused(&blank).field, "flags.Sn.values.5");
}

/// A membership of the flag column `Sn`, its value 5 naming a structure.
const MEMBERSHIPS: &str = r#"memberships:
  - column: Sn
    cite: "ReadMe of I/311, Note 1"
    structures: { "5": test-structure }
"#;

#[test]
fn a_definition_may_assign_its_rows_to_structures_by_a_cited_flag_column() {
    let d = read(&format!("{PLAIN}{FLAGS}{MEMBERSHIPS}")).unwrap();
    let m = &d.memberships[0];
    assert_eq!(
        (m.column.as_str(), m.cite.as_str()),
        ("Sn", "ReadMe of I/311, Note 1")
    );
    assert_eq!(m.structure("5"), Some("test-structure"));
    // A value the map does not list names no structure.
    assert_eq!(m.structure("7"), None);
    assert_eq!(read(PLAIN).unwrap().memberships, []);
}

#[test]
fn a_membership_of_a_column_not_a_cited_flag_or_of_a_value_it_does_not_give_is_refused() {
    let unexplained = refused(&format!("{PLAIN}{MEMBERSHIPS}"));
    assert_eq!(unexplained.field, "memberships[0].column");
    assert!(
        unexplained.to_string().contains("no meanings"),
        "{unexplained}"
    );
    let not_a_flag = refused(&format!(
        "{PLAIN}{FLAGS}{}",
        MEMBERSHIPS.replace("column: Sn", "column: HIP")
    ));
    assert_eq!(not_a_flag.field, "memberships[0].column");
    let unknown_value = refused(&format!(
        "{PLAIN}{FLAGS}{}",
        MEMBERSHIPS.replace("\"5\": test", "\"9\": test")
    ));
    assert_eq!(unknown_value.field, "memberships[0].structures.9");
    let uncited = refused(&format!(
        "{PLAIN}{FLAGS}{}",
        MEMBERSHIPS.replace("\"ReadMe of I/311, Note 1\"", "\" \"")
    ));
    assert_eq!(uncited.field, "memberships[0].cite");
}

#[test]
fn a_membership_naming_a_structure_the_release_does_not_hold_is_refused() {
    let d = read(&format!("{PLAIN}{FLAGS}{MEMBERSHIPS}")).unwrap();
    let definitions = [d];
    assert!(check_structures(&definitions, ["test-structure"]).is_ok());
    let e = check_structures(&definitions, ["another-structure"]).unwrap_err();
    assert!(e.contains("test-stars"), "{e}");
    assert!(e.contains("\"test-structure\""), "{e}");
}
