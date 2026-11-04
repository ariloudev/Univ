//! Each access method asks for its chunks in the provider's own form and reads the answer.
#![expect(
    clippy::unwrap_used,
    reason = "a failed unwrap in a test helper is the test failing"
)]

mod common;

use univ_ingest::access::{check_answer, chunk_requests, read_count};
use univ_ingest::source::{Access, SourceDefinition, TapRun};
use univ_ingest::table::TableReader;

/// A definition with `access` and two columns, read from YAML as a run would.
fn definition(access: &str) -> SourceDefinition {
    let dir = tempfile::tempdir().unwrap();
    common::definition(&dir, access, "https://example.org")
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
        "?-source=J%2FAJ%2F144%2F4%2Fcatalog&-out=dec&-out.add=_RAJ&-out.max=unlimited"
    );
}

#[test]
fn the_computed_columns_asked_for_are_the_ones_the_definition_takes() {
    let yaml = "  method: vizier_tsv\n  table: J/AJ/144/4/catalog\n  constraints: []";
    let mut d = definition(yaml);
    d.columns[0].column = "_DEJ2000".to_owned();
    d.columns[1].column = "_Glon".to_owned();
    let requests = chunk_requests(&d);
    assert!(
        requests[0].suffix.contains("&-out.add=_DEJ%2C_Glon&"),
        "{}",
        requests[0].suffix
    );
}

fn vizier() -> Access {
    Access::VizierTsv {
        table: String::new(),
        constraints: vec![],
        slices: None,
        rows: None,
    }
}

#[test]
fn an_empty_vizier_answer_is_a_table_of_no_rows_unless_it_reports_an_error() {
    let empty = b"#\n#   VizieR Astronomical Server\n#INFO\tqueryParams=-source=VII/259\n\n";
    let table = TableReader::new(&vizier(), empty).unwrap();
    assert_eq!(table.columns(), &[] as &[std::borrow::Cow<str>]);
    assert_eq!(table.count_rows(), Ok(0));
    let d = definition("  method: vizier_tsv\n  table: VII/259/6dfgs\n  constraints: []");
    assert_eq!(check_answer(&d, empty), Ok(0));
    for error in [
        &b"#INFO\tQUERY_STATUS=error\n"[..],
        b"#\n#***  Cannot find table VII/259/nothing\n",
    ] {
        assert!(TableReader::new(&vizier(), error).is_err());
        assert!(check_answer(&d, error).is_err());
    }
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
    let d = definition("  method: files\n  files: [{ path: grids/cf4.npz }]");
    assert_eq!(chunk_requests(&d), []);
}

#[test]
fn an_error_page_is_not_read_as_a_table() {
    let tap = Access::Tap {
        query: String::new(),
        chunks: 1,
        count: None,
        run: TapRun::Sync,
    };
    let page =
        b"<?xml version=\"1.0\"?><VOTABLE><INFO name=\"QUERY_STATUS\" value=\"ERROR\"/></VOTABLE>";
    let refused = TableReader::new(&tap, page).err().unwrap();
    assert!(refused.contains("markup"), "{refused}");
    assert!(TableReader::new(&vizier(), b"#INFO\tQUERY_STATUS=ERROR\n").is_err());
}

#[test]
fn a_table_lends_its_rows_and_refuses_a_row_of_the_wrong_width() {
    let tsv = b"#c\nid\tcz\n \tkm/s\n--\t--\n a \t 12 \nb\t\n";
    let mut table = TableReader::new(&vizier(), tsv).unwrap();
    assert_eq!(table.index("cz"), Some(1));
    assert_eq!(table.next_row().unwrap().unwrap(), ["a", "12"]);
    assert_eq!(table.next_row().unwrap().unwrap(), ["b", ""]);
    assert!(table.next_row().is_none());
    let short = b"#Table1\nra,dec\n1,2\n3\n";
    let tap = Access::Tap {
        query: String::new(),
        chunks: 1,
        count: None,
        run: TapRun::Sync,
    };
    assert_eq!(
        TableReader::new(&tap, short)
            .unwrap()
            .count_rows()
            .err()
            .unwrap(),
        "a row has 1 cells for 2 columns: \"3\""
    );
    assert_eq!(read_count(&tap, b"n\n482106\n"), Ok(482_106));
    assert!(read_count(&tap, b"n\n1\n2\n").is_err());
}

#[test]
fn a_tap_query_run_as_a_job_is_sent_to_async_with_phase_run() {
    let sync = definition(
        "  method: tap\n  query: \"SELECT 1 WHERE MOD(x, {chunks}) = {chunk}\"\n  chunks: 2",
    );
    let jobs = definition(
        "  method: tap\n  query: \"SELECT 1 WHERE MOD(x, {chunks}) = {chunk}\"\n  chunks: 2\n  run: async",
    );
    for r in chunk_requests(&sync) {
        assert!(
            r.suffix.starts_with("/sync?") && !r.suffix.contains("PHASE"),
            "{r:?}"
        );
    }
    let requests = chunk_requests(&jobs);
    assert_eq!(requests.len(), 2);
    for r in &requests {
        assert!(
            r.suffix.starts_with("/async?") && r.suffix.ends_with("&PHASE=RUN"),
            "{r:?}"
        );
    }
    // A job is kept in its own file: a chunk fetched synchronously is not mistaken for it.
    assert_ne!(requests[0].file, chunk_requests(&sync)[0].file);
}
