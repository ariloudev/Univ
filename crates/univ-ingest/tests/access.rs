//! Each access method asks for its chunks in the provider's own form and reads the answer.
#![expect(
    clippy::unwrap_used,
    reason = "a failed unwrap in a test helper is the test failing"
)]

use univ_ingest::access::{chunk_requests, read_table};
use univ_ingest::source::{Access, SourceDefinition};

/// A definition with `access` and two columns, read from YAML as a run would.
fn definition(access: &str) -> SourceDefinition {
    let yaml = format!(
        "id: test-galaxies\ntitle: t\nprovider: p\naccess:\n{access}\ncuts: []\ncolumns:\n  - {{ column: ra, field: right_ascension, unit: degree }}\n  - {{ column: dec, field: declination, unit: degree }}\nframe: icrs\nepoch: 2000.0\ncitation: c\nterms: {{ url: u, quote: q, credit: c }}\nredistribution: {{ status: permitted, reasoning: r }}\nscale_domain: cosmic_web\nmirrors: [\"https://example.org\"]\n"
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test-galaxies.yaml");
    std::fs::write(&path, yaml).unwrap();
    SourceDefinition::read(&path).unwrap()
}

#[test]
fn a_vizier_table_is_asked_for_in_slices_of_an_integer_column() {
    let d = definition(
        "  method: vizier_tsv\n  table: VII/259/6dfgs\n  constraints: [\"q_cz=3..4\"]\n  slices: { column: cz, from: 1499, to: 5999, step: 2000 }\n  rows: 124650",
    );
    let requests = chunk_requests(&d);
    let items: Vec<&str> = requests.iter().map(|r| r.item.as_str()).collect();
    assert_eq!(
        items,
        ["cz 1499 to 3498", "cz 3499 to 5498", "cz 5499 to 5999"]
    );
    let (stem, extension) = requests[2].file.rsplit_once('.').unwrap();
    assert_eq!(extension, "tsv");
    assert!(
        stem.starts_with("chunk-0002-") && stem.len() == 19,
        "{stem}"
    );
    assert_eq!(
        requests[0].suffix,
        "?-source=VII%2F259%2F6dfgs&-out=ra%2Cdec&-out.max=unlimited&q_cz=3..4&cz=1499..3498"
    );
}

#[test]
fn a_small_vizier_table_is_one_request_and_computed_positions_are_added() {
    let yaml = "  method: vizier_tsv\n  table: J/AJ/144/4/catalog\n  constraints: []";
    let mut d = definition(yaml);
    d.columns[0].column = "_RAJ2000".to_owned();
    let requests = chunk_requests(&d);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].item, "the whole table");
    assert_eq!(
        requests[0].suffix,
        "?-source=J%2FAJ%2F144%2F4%2Fcatalog&-out=dec&-out.add=_RAJ%2C_DEJ&-out.max=unlimited"
    );
}

#[test]
fn an_sql_or_adql_query_is_split_by_its_chunk_placeholders() {
    let d = definition(
        "  method: skyserver_sql\n  query: \"SELECT ra, dec FROM SpecObj WHERE (specObjID % {chunks}) = {chunk}\"\n  chunks: 17",
    );
    let requests = chunk_requests(&d);
    assert_eq!(requests.len(), 17);
    assert_eq!(requests[16].item, "chunk 17 of 17");
    assert_eq!(
        requests[16].suffix,
        "?cmd=SELECT+ra%2C+dec+FROM+SpecObj+WHERE+%28specObjID+%25+17%29+%3D+16&format=csv"
    );
    let d = definition(
        "  method: tap\n  query: \"SELECT ra, dec FROM gaiadr3.gaia_source WHERE MOD(random_index, {chunks}) = {chunk}\"\n  chunks: 4",
    );
    let requests = chunk_requests(&d);
    assert_eq!(requests.len(), 4);
    assert!(
        requests[3]
            .suffix
            .starts_with("/sync?REQUEST=doQuery&LANG=ADQL&FORMAT=csv&QUERY=SELECT"),
        "{}",
        requests[3].suffix
    );
    assert!(requests[3].suffix.ends_with("%3D+3"));
}

#[test]
fn a_file_source_has_no_chunk_requests() {
    let d = definition("  method: file\n  path: grids/cf4.npz");
    assert_eq!(chunk_requests(&d), []);
}

#[test]
fn an_error_page_is_not_read_as_a_table() {
    let tap = Access::Tap {
        query: String::new(),
        chunks: 1,
        count: None,
    };
    let page =
        b"<?xml version=\"1.0\"?><VOTABLE><INFO name=\"QUERY_STATUS\" value=\"ERROR\"/></VOTABLE>";
    assert!(read_table(&tap, page).unwrap_err().contains("markup"));
    let vizier = Access::VizierTsv {
        table: String::new(),
        constraints: vec![],
        slices: None,
        rows: None,
    };
    assert!(read_table(&vizier, b"#INFO\tQUERY_STATUS=ERROR\n").is_err());
}
