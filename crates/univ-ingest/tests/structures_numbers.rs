//! Every number of a structure file is held by its citation as a whole token, with its form as
//! published, its uncertainty and, for a length in Mpc, the cosmology it assumes; angles are in
//! range; and numbers written in free text are ones the file cites (RQ-20 to RQ-23).
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

use univ_ingest::structures::{Structure, StructureError};

/// A small structure; each test changes one part of it.
const VALID: &str = r#"
name: Test Void
kind: void
status: reconstructed
papers:
  - id: tully-2019
    reference: "Tully et al. 2019, ApJ 880, 24"
    title: "Cosmicflows-3: Cosmography of the Local Void"
    doi: 10.3847/1538-4357/ab2597
    read: "arXiv source"
position:
  frame: supergalactic
  x: { value: 1650, unit: kilometre_per_second, kind: reconstructed, uncertainty: { form: none_reported },
       cite: { paper: tully-2019, sentence: "SGX, SGY, SGZ of [+1650, −700, +1650] km s-1" } }
  y: { value: -700, unit: kilometre_per_second, kind: reconstructed, uncertainty: { form: none_reported },
       cite: { paper: tully-2019, sentence: "SGX, SGY, SGZ of [+1650, −700, +1650] km s-1" } }
  z: { value: 1650, unit: kilometre_per_second, kind: reconstructed, uncertainty: { form: none_reported },
       cite: { paper: tully-2019, table: "1", row: "Lacerta-2.4", column: "SGZ" } }
values:
  extent:
    value: 16000
    unit: kilometre_per_second
    kind: reconstructed
    uncertainty: { form: symmetric, sigma: 4500 }
    cite: { paper: tully-2019, sentence: "at a distance of 16,000±4,500 km s-1" }
notes: "Table 1 names the minimum Lacerta-2.4, about 16,000 km/s away."
"#;

fn read(text: &str) -> Result<Structure, StructureError> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test-void.yaml");
    std::fs::write(&path, text).unwrap();
    Structure::read(&path)
}

fn refusal(text: &str) -> StructureError {
    read(text).unwrap_err()
}

#[test]
fn a_cited_number_is_a_whole_token_of_its_sentence() {
    read(VALID).unwrap();
    let e = refusal(&VALID.replace("value: 16000", "value: 1600"));
    assert_eq!(e.field, "values.extent");
    assert!(e.problem.contains("1600 does not appear"), "{e}");
    let e = refusal(&VALID.replace("value: 1650,", "value: 165,"));
    assert_eq!(e.field, "position.x");
}

#[test]
fn a_form_as_published_must_stand_for_the_value() {
    let published = |value: &str, form: &str| {
        VALID.replace(
            "    value: 16000\n",
            &format!("    value: {value}\n    as_published: \"{form}\"\n"),
        )
    };
    read(&published("16000", "16,000")).unwrap();
    let e = refusal(&published("9999", "16,000").replace("16,000±", "1650 and 16,000±"));
    assert_eq!(e.field, "values.extent");
    assert!(e.problem.contains("as_published"), "{e}");
    let e = refusal(
        &VALID
            .replace("value: 16000", "value: 9999")
            .replace("    kind: reconstructed\n    uncertainty: { form: symmetric", "    as_published: \"1650\"\n    kind: reconstructed\n    uncertainty: { form: symmetric")
            .replace("16,000±", "1650±"),
    );
    assert!(e.problem.contains("is not the value 9999"), "{e}");
    let ra = |value: &str| {
        VALID
            .replace(
                "    value: 16000\n    unit: kilometre_per_second\n",
                &format!(
                    "    value: {value}\n    unit: degree\n    as_published: \"α = 14h50m\"\n"
                ),
            )
            .replace("at a distance of 16,000", "centred at α = 14h50m, about")
    };
    let e = refusal(&ra("230"));
    assert!(e.problem.contains("14h50m"), "{e}");
    let e = refusal(&ra("222.5"));
    assert!(!e.problem.contains("as_published"), "{e}");
}

#[test]
fn an_uncertainty_appears_in_its_citation_or_cites_its_own() {
    let e = refusal(&VALID.replace("sigma: 4500", "sigma: 450"));
    assert_eq!(e.field, "values.extent");
    assert!(e.problem.contains("uncertainty 450"), "{e}");
    let own = VALID.replace("sigma: 4500 }", "sigma: 450 }\n    uncertainty_cite: { paper: tully-2019, sentence: \"uncertain by 450 km s-1\" }");
    read(&own).unwrap();
    let table = VALID.replace(
        "cite: { paper: tully-2019, sentence: \"at a distance of 16,000±4,500 km s-1\" }",
        "cite: { paper: tully-2019, table: \"2\", row: \"A2151\", column: \"v (km s-1)\", cell: \"16000 ± 4500\" }",
    );
    read(&table).unwrap();
    let e = refusal(&table.replace("16000 ± 4500", "16000"));
    assert!(e.problem.contains("uncertainty 4500"), "{e}");
}

#[test]
fn numbers_in_free_text_are_ones_the_file_cites() {
    let e = refusal(&VALID.replace("about 16,000 km/s away.", "within the inner 5° of it."));
    assert_eq!(e.field, "notes");
    assert!(e.problem.contains("number 5 "), "{e}");
    let dispute = VALID.replace(
        "notes:",
        "disputes:\n  - claim: \"A survey of 7000 galaxies finds voids at z = 0.14.\"\n    cite: { paper: tully-2019, sentence: \"We have found voids at z = 0.14.\" }\nnotes:",
    );
    let e = refusal(&dispute);
    assert_eq!(e.field, "disputes[0].claim");
    assert!(e.problem.contains("7000"), "{e}");
    read(&dispute.replace("A survey of 7000 galaxies finds", "A survey finds")).unwrap();
}

#[test]
fn latitudes_and_longitudes_lie_in_range() {
    let sky = |lon: &str, lat: &str| {
        VALID.replace(
            "position:\n  frame: supergalactic\n",
            &format!(
                "position:\n  frame: galactic\n  lon: {{ value: {lon}, unit: degree, kind: measured, uncertainty: {{ form: none_reported }}, cite: {{ paper: tully-2019, sentence: \"at l = {lon}\" }} }}\n  lat: {{ value: {lat}, unit: degree, kind: measured, uncertainty: {{ form: none_reported }}, cite: {{ paper: tully-2019, sentence: \"and b = {lat}\" }} }}\nx_was:\n  frame: supergalactic\n"
            ),
        )
    };
    let only_sky = |lon: &str, lat: &str| {
        let text = sky(lon, lat);
        let start = text.find("x_was:").unwrap();
        let end = text.find("values:").unwrap();
        format!("{}{}", &text[..start], &text[end..])
    };
    read(&only_sky("350", "-20")).unwrap();
    assert_eq!(refusal(&only_sky("350", "95")).field, "position.lat");
    assert_eq!(refusal(&only_sky("360", "20")).field, "position.lon");
    assert_eq!(refusal(&only_sky("-10", "20")).field, "position.lon");
}

#[test]
fn a_length_in_mpc_names_the_cosmology_its_paper_used() {
    let mpc = VALID
        .replace(
            "value: 16000\n    unit: kilometre_per_second",
            "value: 213\n    unit: megaparsec",
        )
        .replace("sigma: 4500", "sigma: 60")
        .replace("16,000±4,500 km s-1", "213±60 Mpc")
        .replace("about 16,000 km/s away", "about 213 Mpc away");
    let e = refusal(&mpc);
    assert_eq!(e.field, "values.extent");
    assert!(e.problem.contains("assumes"), "{e}");
    let cited = mpc.replace(
        "    cite: { paper: tully-2019, sentence: \"at a distance of 213",
        "    assumes: { cosmology: \"H0 = 75 km/s/Mpc\", cite: { paper: tully-2019, sentence: \"the appropriate value is H0 = 75 km s-1 Mpc-1\" } }\n    cite: { paper: tully-2019, sentence: \"at a distance of 213",
    );
    read(&cited).unwrap();
    let e = refusal(&cited.replace("H0 = 75 km/s/Mpc", "H0 = 70 km/s/Mpc"));
    assert_eq!(e.field, "values.extent.assumes");
}

#[test]
fn a_threshold_refusal_names_its_field_once_and_points_at_the_threshold() {
    let outline = "outline:\n  grid: cf4pp-density\n  threshold:\n    unit: dimensionless\n    kind: reconstructed\n    uncertainty: { form: none_reported }\n    cite: { paper: tully-2019, sentence: \"the isodensity contour -0.7\" }\n  side: below\n  test:\n    catalogues: [sdss-dr17]\n    rule: underdense_in_every_covering_catalogue\n    ratio: 0.85\n    min_expected: 100\n    why: \"fewer galaxies\"\n";
    let text = format!("{VALID}{outline}");
    let line = text.lines().position(|l| l == "  threshold:").unwrap() + 1;
    let e = refusal(&text);
    assert_eq!(e.field, "outline.threshold");
    assert!(!e.problem.contains("outline"), "the path is said once: {e}");
    assert!(e.problem.starts_with("missing field `value`"), "{e}");
    assert!(
        e.problem
            .ends_with(&format!("at line {} column 5", line + 1)),
        "the threshold's first field, under line {line}: {e}"
    );
}
