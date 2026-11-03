//! Source definitions are read whole and checked before anything is fetched (F-1).
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

use std::fs;
use std::path::PathBuf;

use tempfile::TempDir;
use univ_ingest::source::{
    Access, DefinitionError, Field, RedistributionStatus, ScaleDomain, SourceDefinition, read_all,
};
use univ_model::position::Frame;
use univ_model::units::Unit;

/// A complete definition; each test changes one part of it.
const VALID: &str = r#"
id: test-galaxies
title: "Test galaxies"
provider: vizier
access:
  method: vizier_tsv
  table: "VII/259/spectra"
  constraints: ["z_helio=0.005..0.35"]
  rows: 125071
cuts: ["0.005 < z < 0.35"]
columns:
  - { column: _RAJ2000, field: right_ascension, unit: degree }
  - { column: _DEJ2000, field: declination, unit: degree }
  - { column: z_helio, field: redshift, unit: dimensionless }
frame: icrs
epoch: 2000.0
citation: "Jones et al. 2009, MNRAS 399, 683"
terms:
  url: "https://cds.unistra.fr/vizier-org/licences_vizier.html"
  quote: "The data are free to use."
  credit: "This research has made use of the VizieR catalogue access tool, CDS, Strasbourg."
redistribution:
  status: permitted_with_credit
  reasoning: "The terms permit use with the credit above."
scale_domain: cosmic_web
mirrors:
  - "https://vizier.cds.unistra.fr/viz-bin/asu-tsv"
  - "http://vizieridia.saao.ac.za/viz-bin/asu-tsv"
"#;

fn write(dir: &TempDir, id: &str, text: &str) -> PathBuf {
    let path = dir.path().join(format!("{id}.yaml"));
    fs::write(&path, text).unwrap();
    path
}

fn read(text: &str) -> Result<SourceDefinition, DefinitionError> {
    let dir = tempfile::tempdir().unwrap();
    SourceDefinition::read(&write(&dir, "test-galaxies", text))
}

fn refusal(text: &str) -> DefinitionError {
    read(text).unwrap_err()
}

#[test]
fn a_definition_gives_id_title_provider_access_query_and_cuts() {
    let d = read(VALID).unwrap();
    assert_eq!(d.id.as_str(), "test-galaxies");
    assert_eq!(d.title, "Test galaxies");
    assert_eq!(d.provider, "vizier");
    let Access::VizierTsv {
        table,
        constraints,
        rows,
        ..
    } = &d.access
    else {
        panic!("vizier")
    };
    assert_eq!(
        (table.as_str(), constraints.len(), *rows),
        ("VII/259/spectra", 1, Some(125_071))
    );
    assert_eq!(d.cuts, ["0.005 < z < 0.35"]);
}

#[test]
fn each_column_maps_to_a_model_field_with_its_unit() {
    let d = read(VALID).unwrap();
    let z = &d.columns[2];
    assert_eq!(
        (z.column.as_str(), z.field, z.unit),
        ("z_helio", Field::Redshift, Unit::Dimensionless)
    );
}

#[test]
fn a_definition_names_frame_epoch_citation_terms_credit_and_scale_domain() {
    let d = read(VALID).unwrap();
    assert_eq!((d.frame, d.epoch.0), (Frame::Icrs, 2000.0));
    assert!(d.citation.contains("MNRAS 399, 683"));
    assert!(d.terms.quote.contains("free to use"));
    assert!(d.terms.credit.contains("VizieR"));
    assert_eq!(d.scale_domain, ScaleDomain::CosmicWeb);
}

#[test]
fn mirrors_are_kept_in_the_order_written() {
    let d = read(VALID).unwrap();
    assert_eq!(
        d.mirrors[0],
        "https://vizier.cds.unistra.fr/viz-bin/asu-tsv"
    );
    assert_eq!(d.mirrors[1], "http://vizieridia.saao.ac.za/viz-bin/asu-tsv");
}

#[test]
fn an_unknown_unit_frame_or_scale_domain_is_refused_naming_the_file_and_field() {
    let unit = refusal(&VALID.replace("unit: dimensionless", "unit: furlongs"));
    assert_eq!(unit.field, "columns[2].unit");
    assert!(unit.problem.contains("furlongs"), "{unit}");
    assert!(unit.file.ends_with("test-galaxies.yaml"));
    let frame = refusal(&VALID.replace("frame: icrs", "frame: fk4"));
    assert_eq!(frame.field, "frame");
    assert!(frame.problem.contains("fk4"));
    let domain = refusal(&VALID.replace("scale_domain: cosmic_web", "scale_domain: galaxies"));
    assert_eq!(domain.field, "scale_domain");
    let shown = domain.to_string();
    assert!(
        shown.contains("test-galaxies.yaml: scale_domain: unknown variant"),
        "{shown}"
    );
}

#[test]
fn an_incomplete_definition_is_refused_naming_the_missing_field() {
    let missing = refusal(&VALID.replace("citation: \"Jones et al. 2009, MNRAS 399, 683\"\n", ""));
    assert_eq!(missing.field, "(top level)");
    assert!(
        missing.problem.contains("missing field `citation`"),
        "{missing}"
    );
    let empty = refusal(&VALID.replace("title: \"Test galaxies\"", "title: \"\""));
    assert_eq!(
        (empty.field.as_str(), empty.problem.as_str()),
        ("title", "is empty")
    );
    let no_mirror = refusal(
        &VALID
            .replace("  - \"http://vizieridia.saao.ac.za/viz-bin/asu-tsv\"\n", "")
            .replace(
                "  - \"https://vizier.cds.unistra.fr/viz-bin/asu-tsv\"\n",
                "  []\n",
            )
            .replace("mirrors:\n  []", "mirrors: []"),
    );
    assert_eq!(no_mirror.field, "mirrors");
    let unknown = refusal(&format!("{VALID}colour: blue\n"));
    assert!(
        unknown.problem.contains("unknown field `colour`"),
        "{unknown}"
    );
}

#[test]
fn a_chunked_query_without_its_chunk_placeholder_is_refused() {
    let sql = VALID.replace(
        "  method: vizier_tsv\n  table: \"VII/259/spectra\"\n  constraints: [\"z_helio=0.005..0.35\"]\n  rows: 125071\n",
        "  method: skyserver_sql\n  query: \"SELECT ra FROM SpecObj\"\n  chunks: 4\n",
    );
    let refused = refusal(&sql);
    assert_eq!(refused.field, "access.query");
    assert!(
        read(&sql.replace(
            "FROM SpecObj",
            "FROM SpecObj WHERE specObjID % {chunks} = {chunk}"
        ))
        .is_ok()
    );
}

#[test]
fn the_id_must_match_the_file_name() {
    let dir = tempfile::tempdir().unwrap();
    let refused = SourceDefinition::read(&write(&dir, "other-name", VALID)).unwrap_err();
    assert_eq!(refused.field, "id");
}

#[test]
fn the_redistribution_status_is_one_of_three_with_quoted_terms_and_reasoning() {
    for (written, status) in [
        ("permitted", RedistributionStatus::Permitted),
        (
            "permitted_with_credit",
            RedistributionStatus::PermittedWithCredit,
        ),
        ("forbidden", RedistributionStatus::Forbidden),
    ] {
        let d = read(&VALID.replace(
            "status: permitted_with_credit",
            &format!("status: {written}"),
        ))
        .unwrap();
        assert_eq!(d.redistribution.status, status);
    }
    let maybe = refusal(&VALID.replace("status: permitted_with_credit", "status: probably"));
    assert_eq!(maybe.field, "redistribution.status");
    let unreasoned = refusal(&VALID.replace(
        "reasoning: \"The terms permit use with the credit above.\"",
        "reasoning: \" \"",
    ));
    assert_eq!(unreasoned.field, "redistribution.reasoning");
    let unquoted = refusal(&VALID.replace("quote: \"The data are free to use.\"", "quote: \"\""));
    assert_eq!(unquoted.field, "terms.quote");
}

#[test]
fn a_forbidden_source_blocks_a_release_naming_it_and_quoting_its_terms() {
    let d = read(&VALID.replace("status: permitted_with_credit", "status: forbidden")).unwrap();
    let refused = d.publishable().unwrap_err().to_string();
    assert!(
        refused.contains("test-galaxies") && refused.contains("\"The data are free to use.\""),
        "{refused}"
    );
    assert!(read(VALID).unwrap().publishable().is_ok());
}

#[test]
fn a_run_reads_every_definition_and_stops_at_the_first_bad_one() {
    let dir = tempfile::tempdir().unwrap();
    write(&dir, "test-galaxies", VALID);
    assert_eq!(read_all(dir.path()).unwrap().len(), 1);
    write(
        &dir,
        "test-stars",
        &VALID
            .replace("id: test-galaxies", "id: test-stars")
            .replace("frame: icrs", "frame: nowhere"),
    );
    let refused = read_all(dir.path()).unwrap_err();
    assert!(refused.file.ends_with("test-stars.yaml") && refused.field == "frame");
}

#[test]
fn two_files_kept_under_the_same_name_are_refused() {
    let files = VALID.replace(
        "  method: vizier_tsv\n  table: \"VII/259/spectra\"\n  constraints: [\"z_helio=0.005..0.35\"]\n  rows: 125071",
        "  method: files\n  files: [{ path: a/grid.npz }, { path: b/grid.npz }]",
    );
    let error = refusal(&files);
    assert_eq!(error.field, "access.files");
    assert!(error.problem.contains("same file name"), "{error}");
}
