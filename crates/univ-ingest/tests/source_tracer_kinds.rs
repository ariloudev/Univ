//! A source of the Galaxy's tracers names the Galaxy tracers scale domain and the kind of object
//! its rows are; no other source names a kind (define-a-source, normalize-records).

#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

use univ_ingest::source::{DefinitionError, ScaleDomain, SourceDefinition};
use univ_model::object::ObjectKind;

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

fn read(text: &str) -> Result<SourceDefinition, DefinitionError> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test-galaxies.yaml");
    std::fs::write(&path, text).unwrap();
    SourceDefinition::read(&path)
}

/// `VALID` as a source of the Galaxy's tracers, with `kind` as its kind of object, if any.
fn tracers(kind: Option<&str>) -> String {
    let domain = "scale_domain: galaxy_tracers";
    let named = kind.map_or(domain.to_owned(), |k| format!("{domain}\nobject_kind: {k}"));
    VALID.replace("scale_domain: cosmic_web", &named)
}

#[test]
fn a_tracer_source_names_the_galaxy_tracers_domain_and_its_kind_of_object() {
    for (name, kind) in [
        ("open_cluster", ObjectKind::OpenCluster),
        ("globular_cluster", ObjectKind::GlobularCluster),
        ("classical_cepheid", ObjectKind::ClassicalCepheid),
        ("rr_lyrae_star", ObjectKind::RrLyraeStar),
        ("star_forming_region", ObjectKind::StarFormingRegion),
    ] {
        let definition = read(&tracers(Some(name))).unwrap();
        assert_eq!(definition.scale_domain, ScaleDomain::GalaxyTracers);
        assert_eq!(definition.object_kind, Some(kind));
        assert!(kind.is_tracer());
    }
    assert_eq!(read(VALID).unwrap().object_kind, None);
}

#[test]
fn a_tracer_source_without_a_kind_or_another_source_with_one_is_refused() {
    let missing = read(&tracers(None)).unwrap_err();
    assert_eq!(missing.field, "object_kind");
    assert!(missing.problem.contains("is missing"), "{missing}");
    let elsewhere = VALID.replace(
        "scale_domain: cosmic_web",
        "scale_domain: cosmic_web\nobject_kind: open_cluster",
    );
    assert_eq!(read(&elsewhere).unwrap_err().field, "object_kind");
    // A tracer source names a tracer's kind, not a body's or a group's.
    assert_eq!(
        read(&tracers(Some("galaxy_group"))).unwrap_err().field,
        "object_kind"
    );
}
