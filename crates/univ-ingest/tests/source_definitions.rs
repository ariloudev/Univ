//! Source definitions are read whole and checked before anything is fetched (F-1).
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

use std::fs;
use std::path::PathBuf;

use tempfile::TempDir;
use univ_ingest::source::{
    Access, DefinitionError, Field, MAX_FIXES, RedistributionStatus, Review, ScaleDomain,
    SourceDefinition, read_all,
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
reference: { bibcode: "2009MNRAS.399..683J", doi: "10.1111/j.1365-2966.2009.15338.x" }
terms:
  url: "https://cds.unistra.fr/vizier-org/licences_vizier.html"
  quote: "The data are free to use."
  credit: "This research has made use of the VizieR catalogue access tool, CDS, Strasbourg."
redistribution:
  status: permitted_with_credit
  review: provisional
  reasoning: "The terms permit use with the credit above."
scale_domain: cosmic_web
mirrors:
  - "https://vizier.cds.unistra.fr/viz-bin/asu-tsv"
  - "http://vizieridia.saao.ac.za/viz-bin/asu-tsv"
normalization:
  redshifts:
    - { column: z_helio, frame: heliocentric, kind: spectroscopic, basis: "z_helio is heliocentric" }
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
fn a_definition_names_its_primary_reference_by_bibcode_and_doi() {
    let d = read(VALID).unwrap();
    assert_eq!(d.reference.bibcode.as_deref(), Some("2009MNRAS.399..683J"));
    assert_eq!(
        d.reference.doi.as_deref(),
        Some("10.1111/j.1365-2966.2009.15338.x")
    );
    // A DOI alone, or a reason for none, also stands.
    let doi_only = VALID.replace("bibcode: \"2009MNRAS.399..683J\", ", "");
    assert_eq!(read(&doi_only).unwrap().reference.bibcode, None);
    let none = VALID.replace(REFERENCE, "reference: { none: \"cited per structure\" }\n");
    assert!(read(&none).unwrap().reference.none.is_some());
}

/// The reference line of [`VALID`].
const REFERENCE: &str =
    "reference: { bibcode: \"2009MNRAS.399..683J\", doi: \"10.1111/j.1365-2966.2009.15338.x\" }\n";

#[test]
fn a_malformed_primary_reference_is_refused_naming_the_field() {
    for (bad, field, says) in [
        // 18 characters: the volume's padding is one dot short.
        ("2009MNRAS.399.683J", "reference.bibcode", "19-character"),
        ("2009 MNRAS 399 683J", "reference.bibcode", "19-character"),
        ("09MNRAS..399..683J.", "reference.bibcode", "19-character"),
    ] {
        let e = refusal(&VALID.replace("2009MNRAS.399..683J", bad));
        assert_eq!(e.field, field, "{bad}: {e}");
        assert!(e.problem.contains(says), "{bad}: {e}");
    }
    for bad in [
        "doi:10.1111/j.1365-2966.2009.15338.x",
        "11.1111/x",
        "10.1111",
        "10.1111/ j",
    ] {
        let e = refusal(&VALID.replace("10.1111/j.1365-2966.2009.15338.x", bad));
        assert_eq!(e.field, "reference.doi", "{bad}: {e}");
        assert!(e.problem.contains("is not a DOI"), "{bad}: {e}");
    }
    let neither = refusal(&VALID.replace(REFERENCE, "reference: {}\n"));
    assert_eq!(neither.field, "reference", "{neither}");
    let both =
        refusal(&VALID.replace("doi: \"10.1111/j.1365-2966.2009.15338.x\"", "none: \"why\""));
    assert_eq!(both.field, "reference.none", "{both}");
    let missing = refusal(&VALID.replace(REFERENCE, ""));
    assert!(
        missing.problem.contains("missing field `reference`"),
        "{missing}"
    );
}

#[test]
fn every_repository_definition_names_a_well_formed_primary_reference() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../sources");
    for d in read_all(&dir).unwrap() {
        let r = &d.reference;
        assert!(
            r.bibcode.is_some() || r.doi.is_some() || r.none.is_some(),
            "{}",
            d.id
        );
        // The primary reference is the paper the citation names first.
        if let Some(b) = &r.bibcode {
            assert!(d.citation.contains(b.as_str()), "{}: {b}", d.id);
        }
        if let Some(doi) = &r.doi {
            assert!(
                d.citation.contains(&format!("doi:{doi}")),
                "{}: {doi}",
                d.id
            );
        }
    }
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
fn the_redistribution_review_is_typed_and_every_definition_states_it() {
    assert_eq!(
        read(VALID).unwrap().redistribution.review,
        Review::Provisional
    );
    let settled = read(&VALID.replace("review: provisional", "review: settled")).unwrap();
    assert_eq!(settled.redistribution.review, Review::Settled);
    let unknown = refusal(&VALID.replace("review: provisional", "review: maybe"));
    assert_eq!(unknown.field, "redistribution.review");
    let missing = refusal(&VALID.replace("  review: provisional\n", ""));
    assert_eq!(missing.field, "redistribution");
    assert!(missing.problem.contains("review"), "{}", missing.problem);
    // Every definition of the first release is still provisional: its prose said so, and the
    // field now carries it.
    for d in read_all(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../sources")).unwrap() {
        assert_eq!(d.redistribution.review, Review::Provisional, "{}", d.id);
        assert!(
            d.redistribution
                .reasoning
                .to_lowercase()
                .contains("provisional")
        );
    }
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

#[test]
fn each_fix_names_its_defect_change_and_evidence_or_is_refused() {
    let fix = |defect: &str, change: &str, evidence: &str| {
        format!(
            "{VALID}fixes:\n  - {{ defect: \"{defect}\", change: \"{change}\", evidence: \"{evidence}\" }}\n"
        )
    };
    let d = read(&fix("z is too large", "z is read as cz", "the ReadMe")).unwrap();
    assert_eq!(d.fixes[0].defect, "z is too large");
    assert_eq!(d.fixes[0].action, None);
    assert_eq!(refusal(&fix(" ", "c", "e")).field, "fixes[0].defect");
    assert_eq!(refusal(&fix("d", "", "e")).field, "fixes[0].change");
    assert_eq!(refusal(&fix("d", "c", "")).field, "fixes[0].evidence");
    let without = format!("{VALID}fixes:\n  - {{ defect: d, change: c }}\n");
    assert!(refusal(&without).problem.contains("evidence"));
}

#[test]
fn every_fix_of_the_first_release_names_its_defect_change_and_evidence() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../sources");
    let fixes: Vec<_> = read_all(&dir)
        .unwrap()
        .into_iter()
        .flat_map(|d| d.fixes)
        .collect();
    assert!(fixes.len() >= 6, "{} fixes", fixes.len());
    for fix in fixes {
        for text in [&fix.defect, &fix.change, &fix.evidence] {
            assert!(text.split_whitespace().count() >= 5, "{fix:?}");
        }
    }
}

#[test]
fn a_reading_or_rule_or_fix_that_names_a_column_not_taken_is_refused() {
    let reading = VALID.replace("column: z_helio, frame", "column: z_cmb, frame");
    let refused = refusal(&reading);
    assert_eq!(refused.field, "normalization.redshifts[0].column");
    assert!(refused.problem.contains("z_cmb"), "{}", refused.problem);
    let rule = format!(
        "{VALID}  rules:\n    - {{ when: {{ column: q, is: [\"1\"] }}, leave_out: simulated_object, basis: b }}\n"
    );
    assert_eq!(refusal(&rule).field, "normalization.rules[0].when.column");
    let fix = format!(
        "{VALID}fixes:\n  - defect: d\n    change: c\n    evidence: e\n    action: {{ kind: read_as, columns: [SGX], unit: kilometre_per_second }}\n"
    );
    assert_eq!(refusal(&fix).field, "fixes[0].action.columns");
    let local = VALID.replace("frame: heliocentric", "frame: local_group");
    assert_eq!(refusal(&local).field, "normalization.redshifts[0].frame");
    // A redshift published in the CMB frame is read as published.
    let cmb = VALID.replace("frame: heliocentric", "frame: cmb");
    assert!(read(&cmb).is_ok());
}

#[test]
fn a_redshift_or_distance_column_without_a_reading_is_refused() {
    let unread = VALID.replace(
        "normalization:\n  redshifts:\n",
        "normalization:\n  rules: []\n  redshifts_not:\n",
    );
    assert!(read(&unread).is_err());
    let none = VALID.replace(
        "normalization:\n  redshifts:\n    - { column: z_helio, frame: heliocentric, kind: spectroscopic, basis: \"z_helio is heliocentric\" }\n",
        "",
    );
    let refused = refusal(&none);
    assert_eq!(refused.field, "normalization");
    assert!(refused.problem.contains("z_helio"), "{}", refused.problem);
}

#[test]
fn a_definition_with_more_fixes_than_the_normalizer_counts_is_refused() {
    let fix = "  - { defect: \"d\", change: \"c\", evidence: \"e\" }\n";
    let fixes = |n: usize| format!("{VALID}fixes:\n{}", fix.repeat(n));
    assert_eq!(read(&fixes(MAX_FIXES)).unwrap().fixes.len(), MAX_FIXES);
    let e = refusal(&fixes(MAX_FIXES + 1));
    assert_eq!(e.field, "fixes");
    assert!(e.problem.contains("more than 64"), "{e}");
}

#[test]
fn a_colour_between_a_misspelled_or_repeated_band_is_refused() {
    let text = fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sources/hipparcos-2007.yaml"),
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let read = |text: &str| SourceDefinition::read(&write(&dir, "hipparcos-2007", text));
    assert!(read(&text).is_ok());
    for (bands, problem) in [("[b, V]", "band b"), ("[V, V]", "band V twice")] {
        let refused = read(&text.replace("bands: [B, V]", &format!("bands: {bands}"))).unwrap_err();
        assert!(
            refused.field.starts_with("normalization.colours[0]"),
            "{}",
            refused.field
        );
        assert!(refused.problem.contains(problem), "{}", refused.problem);
    }
}
