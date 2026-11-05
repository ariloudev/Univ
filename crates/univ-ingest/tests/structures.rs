//! Researched structures are read from structure files in which every number cites its paper and
//! the sentence, table or figure it was read from (RQ-20 to RQ-23).
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use tempfile::TempDir;
use univ_ingest::source::ScaleDomain;
use univ_ingest::structures::{
    Coordinates, Locator, MemberSource, Side, Structure, StructureError, StructureGeometry,
    read_all,
};
use univ_model::geometry::{Geometry, Member, Primitive};
use univ_model::position::{Frame, Rotation};
use univ_model::value::KindOfValue;

/// The structure files of the repository.
fn repository_structures() -> PathBuf {
    common::structures_dir()
}

/// A complete structure; each test changes one part of it.
const VALID: &str = r#"
name: Test Void
kind: void
status: reconstructed
scale_domain: cosmic_web
papers:
  - id: tully-2019
    reference: "Tully et al. 2019, ApJ 880, 24"
    title: "Cosmicflows-3: Cosmography of the Local Void"
    doi: 10.3847/1538-4357/ab2597
    read: "arXiv source"
position:
  frame: supergalactic
  x: { value: 1650, unit: kilometre_per_second, kind: reconstructed, uncertainty: { form: none_reported },
       cite: { paper: tully-2019, sentence: "SGX, SGY, SGZ of [+1650, -700, +1650] km s-1" } }
  y: { value: -700, unit: kilometre_per_second, kind: reconstructed, uncertainty: { form: none_reported },
       cite: { paper: tully-2019, sentence: "SGX, SGY, SGZ of [+1650, -700, +1650] km s-1" } }
  z: { value: 1650, unit: kilometre_per_second, kind: reconstructed, uncertainty: { form: none_reported },
       cite: { paper: tully-2019, table: "1", row: "Lacerta-2.4", column: "SGZ" } }
values:
  extent:
    value: 5200
    unit: kilometre_per_second
    kind: estimated
    uncertainty: { form: none_reported }
    cite: { paper: tully-2019, sentence: "dSGX,SGY,SGZ = 5200,3000,4500 km s-1" }
notes: "A test structure."
"#;

fn write(dir: &TempDir, name: &str, text: &str) -> PathBuf {
    let path = dir.path().join(format!("{name}.yaml"));
    fs::write(&path, text).unwrap();
    path
}

fn read(text: &str) -> Result<Structure, StructureError> {
    let dir = tempfile::tempdir().unwrap();
    Structure::read(&write(&dir, "test-void", text))
}

fn refusal(text: &str) -> StructureError {
    read(text).unwrap_err()
}

#[test]
fn every_ported_structure_loads() {
    let structures = read_all(&repository_structures()).unwrap();
    let files = fs::read_dir(repository_structures())
        .unwrap()
        .filter(|e| e.as_ref().unwrap().path().extension() == Some("yaml".as_ref()))
        .count();
    // The newsroom atlas's 26, and those release 8 adds.
    assert!(structures.len() > 26, "{} structures", structures.len());
    assert_eq!(structures.len(), files, "every structure file loads");
    let names: HashSet<&str> = structures.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names.len(), structures.len(), "names are unique");
}

#[test]
fn each_number_cites_its_paper_and_the_sentence_table_or_figure() {
    // AC-54: in the test structure and in every ported one.
    let mut structures = read_all(&repository_structures()).unwrap();
    structures.push(read(VALID).unwrap());
    for s in &structures {
        let papers: HashSet<&str> = s.papers.iter().map(|p| p.id.as_str()).collect();
        assert!(
            s.papers
                .iter()
                .all(|p| p.doi.is_some() || p.bibcode.is_some())
        );
        for (field, value) in s.cited_values() {
            assert!(
                papers.contains(value.cite.paper.as_str()),
                "{}: {field}",
                s.name
            );
            match &value.cite.locator {
                Locator::Sentence { quote, .. } => {
                    assert!(value.appears_in(quote), "{}: {field}", s.name);
                }
                Locator::Table {
                    number,
                    row,
                    column,
                    ..
                } => {
                    assert!(![number, row, column].iter().any(|t| t.is_empty()));
                }
                Locator::Figure { number, how } => {
                    assert!(!number.is_empty() && !how.is_empty());
                }
            }
        }
        if let Some(StructureGeometry::Members(m)) = &s.geometry {
            assert!(papers.contains(m.source.paper()), "{}", s.name);
            assert!(m.rows.iter().all(|r| r.values.len() == m.columns.len()));
        }
    }
    let lacerta = read(VALID).unwrap();
    let Some(Coordinates::Cartesian { x, z, .. }) = &lacerta.position else {
        panic!("cartesian")
    };
    assert!(matches!(&x.cite.locator, Locator::Sentence { quote, .. } if quote.contains("1650")));
    assert!(matches!(&z.cite.locator, Locator::Table { row, .. } if row == "Lacerta-2.4"));
}

#[test]
fn a_quoted_sentence_must_hold_the_number_it_is_cited_for() {
    let wrong = VALID.replace("value: 5200", "value: 5300");
    let e = refusal(&wrong);
    assert_eq!(e.field, "values.extent");
    assert!(e.problem.contains("5300"), "{e}");
}

/// The tag univ-model gives each geometry kind, read from univ-model itself.
fn model_kinds() -> HashSet<String> {
    let x = [1.0, 0.0, 0.0];
    let kinds = [
        Geometry::Point { at: x },
        Geometry::Ray {
            origin: x,
            direction: x,
            reach: 1.0,
        },
        Geometry::Range {
            direction: x,
            near: 0.0,
            far: 1.0,
        },
        Geometry::Primitive {
            primitive: Primitive::OrientedBox {
                centre: x,
                half_extents: x,
                orientation: Rotation::IDENTITY,
            },
        },
        Geometry::Path { points: vec![x, x] },
        Geometry::Members {
            members: vec![Member::Tracer {
                at: x,
                kind: "quasar".into(),
            }],
        },
        Geometry::SkyRegion {
            order: 0,
            cells: vec![0],
            near: 0.0,
            far: 1.0,
        },
        Geometry::VolumeGrid {
            grid: "g".into(),
            origin: x,
            cell: 1.0,
            shape: [1, 1, 1],
        },
    ];
    kinds
        .iter()
        .map(|g| {
            let yaml = serde_norway::to_string(g).unwrap();
            let line = yaml.lines().find(|l| l.starts_with("kind:")).unwrap();
            line.trim_start_matches("kind:").trim().to_owned()
        })
        .collect()
}

#[test]
fn a_structure_geometry_is_a_univ_model_geometry_kind_in_a_named_frame() {
    // AC-55.
    let kinds = model_kinds();
    let structures = read_all(&repository_structures()).unwrap();
    let mut seen = HashSet::new();
    for s in &structures {
        match s.geometry_kind() {
            Some(kind) => {
                assert!(kinds.contains(kind), "{}: {kind}", s.name);
                seen.insert(kind);
                if let Some(position) = &s.position {
                    assert!(position.frame().is_some(), "{}", s.name);
                }
                // A geometry names its frame; only a sphere about the observer has none.
                if let Some(g) = &s.geometry
                    && g.frame().is_none()
                {
                    let StructureGeometry::Primitive(p) = g else {
                        panic!("{}", s.name)
                    };
                    assert_eq!(p.centre, Coordinates::Observer);
                }
            }
            None => assert!(s.unplaced.is_some(), "{}", s.name),
        }
    }
    assert!(seen.len() >= 3, "{seen:?}");
    // Every frame is a univ-model frame, B1950 and an unstated equinox among them.
    let frames: Vec<Frame> =
        serde_norway::from_str("[supergalactic, fk4_b1950, equatorial_equinox_unstated]").unwrap();
    assert_eq!(
        frames,
        [
            Frame::Supergalactic,
            Frame::Fk4B1950,
            Frame::EquatorialEquinoxUnstated
        ]
    );
    // An unknown kind or frame is refused.
    let e = refusal(&VALID.replace("frame: supergalactic", "frame: ecliptic"));
    assert_eq!(e.field, "position.frame");
    let e = refusal(&format!("{VALID}geometry: {{ kind: mesh }}\n"));
    assert_eq!(e.field, "geometry.kind");
}

#[test]
fn an_uncited_number_is_refused_naming_the_file_the_field_and_the_number() {
    // AC-56: a bare number, and a value without its citation.
    let bare = VALID.replace(
        "values:\n  extent:\n    value: 5200",
        "values:\n  depth: 4500\n  extent:\n    value: 5200",
    );
    let e = refusal(&bare);
    assert!(e.file.ends_with("test-void.yaml"));
    assert_eq!(e.field, "values.depth");
    assert!(e.problem.contains("the number 4500 cites nothing"), "{e}");
    let message = e.to_string();
    assert!(message.contains("test-void.yaml") && message.contains("values.depth"));

    let no_cite = VALID.replace(
        "    cite: { paper: tully-2019, sentence: \"dSGX,SGY,SGZ = 5200,3000,4500 km s-1\" }\n",
        "",
    );
    let e = refusal(&no_cite);
    assert_eq!(e.field, "values.extent");
    assert!(e.problem.contains("the number 5200 cites nothing"), "{e}");

    let unknown_paper = VALID.replace(
        "cite: { paper: tully-2019, table",
        "cite: { paper: tully-2020, table",
    );
    let e = refusal(&unknown_paper);
    assert_eq!(e.field, "position.z.cite.paper");
    assert!(e.problem.contains("tully-2020"));

    let no_locator = VALID.replace(", table: \"1\", row: \"Lacerta-2.4\", column: \"SGZ\"", "");
    assert_eq!(refusal(&no_locator).field, "position.z.cite");
}

/// A structure with members digitised from a figure.
fn digitised(kind: &str) -> String {
    format!(
        r#"
name: Test Arc
kind: arc
status: statistical_claim
scale_domain: cosmic_web
papers:
  - {{ id: lopez-2022, reference: "Lopez et al. 2022", title: "A Giant Arc on the Sky", doi: 10.1093/mnras/stac2204, read: "arXiv source" }}
geometry:
  kind: members
  members:
    frame: icrs
    tracer: Mg II absorber
    source: {{ paper: lopez-2022, figure: "5", how: "circle centres read from the vector figure" }}
    columns:
      - {{ name: lon, header: RA, unit: degree, kind: {kind}, uncertainty: {{ form: symmetric, sigma: 0.01 }} }}
      - {{ name: lat, header: Dec, unit: degree, kind: {kind}, uncertainty: {{ form: symmetric, sigma: 0.01 }} }}
    rows:
      - [black-1, 206.4, 38.39]
      - [black-2, 208.95, 38.21]
values:
  slice_centre:
    value: 0.802
    unit: dimensionless
    kind: {kind}
    uncertainty: {{ form: none_reported }}
    cite: {{ paper: lopez-2022, figure: "5", how: "read from the figure title" }}
notes: "A test arc."
"#
    )
}

#[test]
fn a_digitised_value_cites_its_figure_and_is_estimated() {
    // AC-57: refused unless estimated, both as a single value and as a member table.
    let arc = read(&digitised("estimated")).unwrap();
    let Some(StructureGeometry::Members(m)) = &arc.geometry else {
        panic!("members")
    };
    assert!(matches!(&m.source, MemberSource::Figure { number, .. } if number == "5"));
    assert_eq!(m.rows[1].values, vec![208.95, 38.21]);
    let e = refusal(&digitised("measured"));
    assert!(e.problem.contains("estimated"), "{e}");
    let single = digitised("estimated").replace(
        "kind: estimated\n    uncertainty: { form: none_reported }",
        "kind: measured\n    uncertainty: { form: none_reported }",
    );
    let e = refusal(&single);
    assert_eq!(e.field, "values.slice_centre");
    assert!(
        e.problem.contains("figure 5") && e.problem.contains("estimated"),
        "{e}"
    );
    // Every figure citation among the ported structures is estimated.
    for s in read_all(&repository_structures()).unwrap() {
        for (field, value) in s.cited_values() {
            if matches!(value.cite.locator, Locator::Figure { .. }) {
                assert_eq!(value.kind, KindOfValue::Estimated, "{}: {field}", s.name);
            }
        }
        if let Some(StructureGeometry::Members(m)) = &s.geometry
            && matches!(m.source, MemberSource::Figure { .. })
        {
            assert!(m.columns.iter().all(|c| c.kind == KindOfValue::Estimated));
        }
    }
}

const OUTLINE: &str = r#"
outline:
  grid: cf4pp-density
  threshold:
    value: -0.7
    unit: dimensionless
    kind: reconstructed
    uncertainty: { form: none_reported }
    cite: { paper: tully-2019, sentence: "the isodensity contour -0.7" }
  side: below
  test:
    catalogues: [sdss-dr17, sixdfgs-dr3]
    rule: underdense_in_every_covering_catalogue
    ratio: 0.85
    min_expected: 100
    why: "fewer galaxies than other cells at its distance in every catalogue that covers it"
"#;

#[test]
fn a_grid_outline_records_its_grid_threshold_and_test_beside_a_published_position() {
    // RQ-23 and AC-59: the fields, and the published position kept as the fallback.
    let s = read(&format!("{VALID}{OUTLINE}")).unwrap();
    let outline = s.outline.as_ref().unwrap();
    assert_eq!(outline.grid.as_str(), "cf4pp-density");
    assert!((outline.threshold.value() + 0.7).abs() < 1e-12);
    assert_eq!(outline.side, Side::Below);
    assert_eq!(outline.test.catalogues.len(), 2);
    assert!(s.position.is_some());
    assert!(
        s.cited_values()
            .iter()
            .any(|(f, _)| f == "outline.threshold")
    );

    let unplaced = format!("{VALID}{OUTLINE}");
    let start = unplaced.find("position:").unwrap();
    let end = unplaced.find("values:").unwrap();
    let without_position = format!(
        "{}{}",
        &unplaced[..start],
        &unplaced[end..]
    )
    .replace("notes:", "geometry:\n  kind: primitive\n  primitive:\n    form: sphere\n    centre: { observer: true }\n    radius: { value: 5200, unit: kilometre_per_second, kind: estimated, uncertainty: { form: none_reported }, cite: { paper: tully-2019, sentence: \"5200\" } }\nnotes:");
    let e = refusal(&without_position);
    assert!(e.problem.contains("radius or diameter is a length"), "{e}");
    let e = refusal(&without_position.replace(
        "unit: kilometre_per_second, kind: estimated",
        "unit: megaparsec, kind: estimated",
    ));
    assert_eq!(e.field, "position");

    let e = refusal(&format!("{VALID}{OUTLINE}").replace("    value: -0.7\n", ""));
    assert_eq!(e.field, "outline.threshold");
    let e = refusal(&format!("{VALID}{OUTLINE}").replace("[sdss-dr17, sixdfgs-dr3]", "[]"));
    assert!(e.problem.contains("catalogues"), "{e}");
    let e = refusal(&format!("{VALID}{OUTLINE}").replace("grid: cf4pp-density", "grid: CF4"));
    assert_eq!(e.field, "outline.grid");
}

#[test]
fn a_member_table_without_a_source_names_its_first_number() {
    let text = digitised("estimated").replace(
        "    source: { paper: lopez-2022, figure: \"5\", how: \"circle centres read from the vector figure\" }\n",
        "",
    );
    let e = refusal(&text);
    assert_eq!(e.field, "geometry.members");
    assert!(e.problem.contains("206.4"), "{e}");
}

#[test]
fn a_structure_file_is_named_in_kebab_case() {
    let dir = tempfile::tempdir().unwrap();
    write(&dir, "Test_Void", VALID);
    assert_eq!(read_all(dir.path()).unwrap_err().field, "(file name)");
}

#[test]
fn a_structure_names_its_scale_domain_or_is_refused() {
    let missing = refusal(&VALID.replace("scale_domain: cosmic_web\n", ""));
    assert!(missing.problem.contains("scale_domain"), "{missing}");
    let unknown =
        refusal(&VALID.replace("scale_domain: cosmic_web", "scale_domain: superclusters"));
    assert_eq!(unknown.field, "scale_domain", "{unknown}");
    assert_eq!(read(VALID).unwrap().scale_domain, ScaleDomain::CosmicWeb);
}

#[test]
fn extents_name_lengths_or_velocities_among_the_values_about_a_placed_position() {
    let with = |extents: &str| VALID.replace("values:\n", &format!("{extents}values:\n"));
    let ok = read(&with("extents: { x: extent, y: extent, z: extent }\n")).unwrap();
    assert_eq!(ok.extents.unwrap().names(), ["extent"; 3]);
    let missing = refusal(&with("extents: { x: extent, y: depth, z: extent }\n"));
    assert_eq!(missing.field, "extents.y");
    assert!(missing.problem.contains("depth"), "{}", missing.problem);
    let angle = with("extents: { x: extent, y: extent, z: extent }\n").replace(
        "unit: kilometre_per_second\n    kind: estimated",
        "unit: degree\n    kind: estimated",
    );
    assert_eq!(refusal(&angle).field, "extents.x");
    let text = with("extents: { x: extent, y: extent, z: extent }\n");
    let (head, tail) = (
        &text[..text.find("position:").unwrap()],
        &text[text.find("extents:").unwrap()..],
    );
    let unplaced = refusal(&format!(
        "{head}unplaced: \"No position is published.\"\n{tail}"
    ));
    assert_eq!(unplaced.field, "extents");
}
