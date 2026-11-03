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
fn a_vizier_table_is_asked_for_in_ranges_of_its_record_number() {
    let d = definition(
        "  method: vizier_tsv\n  table: VII/259/6dfgs\n  constraints: [\"q_cz=3..4\"]\n  rows: 125071\n  chunk_rows: 50000",
    );
    let requests = chunk_requests(&d);
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].item, "records 1 to 50000");
    assert_eq!(requests[2].item, "records 100001 to 125071");
    assert_eq!(requests[2].file, "chunk-0002.tsv");
    assert_eq!(
        requests[0].suffix,
        "?-source=VII%2F259%2F6dfgs&-out=recno%2Cra%2Cdec&-out.max=unlimited&q_cz=3..4&recno=1..50000"
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
    };
    let page =
        b"<?xml version=\"1.0\"?><VOTABLE><INFO name=\"QUERY_STATUS\" value=\"ERROR\"/></VOTABLE>";
    assert!(read_table(&tap, page).unwrap_err().contains("markup"));
    let vizier = Access::VizierTsv {
        table: String::new(),
        constraints: vec![],
        rows: 1,
        chunk_rows: 1,
    };
    assert!(read_table(&vizier, b"#INFO\tQUERY_STATUS=ERROR\n").is_err());
}
