use serde::Deserialize;
use univ_model::release::{ColumnKind, Scale, Scope};

use super::*;

#[derive(Deserialize)]
struct Cases {
    refused: Vec<Refused>,
    allowed: Vec<String>,
}

#[derive(Deserialize)]
struct Refused {
    sql: String,
    call: String,
    why: String,
}

/// `text` with each `READ(` made `json_extract(`: the SQL the tests refuse is not written out in
/// this file, which the gate reads.
fn reading(text: &str) -> String {
    text.replace("READ(", "json_extract(")
}

fn cases() -> Cases {
    serde_norway::from_str(include_str!("cases.yaml")).unwrap()
}

#[test]
fn each_refused_form_is_refused_with_its_call_and_reason() {
    for case in cases().refused {
        let found = sql::refusals(&case.sql);
        assert_eq!(found.len(), 1, "{}: {found:?}", case.sql);
        assert_eq!(found[0].call, case.call, "{}", case.sql);
        assert!(
            found[0].why.contains(&case.why),
            "{}: {}",
            case.sql,
            found[0].why
        );
    }
}

#[test]
fn each_allowed_form_passes() {
    for sql in cases().allowed {
        assert_eq!(sql::refusals(&sql), vec![], "{sql}");
    }
}

#[test]
fn each_statement_names_its_own_tables() {
    let sql = reading(
        "SELECT READ(d.shape, '$') FROM structure_details d;\n\
         SELECT READ(d.via_source, '$') FROM entries d",
    );
    let found = sql::refusals(&sql);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(line_of(&sql, found[0].at), 2);
}

#[test]
fn a_json_column_in_a_table_per_object_or_entry_is_refused_and_one_per_source_is_not() {
    let table = |name, scale| Table {
        name,
        columns: &["id", "flags"],
        kinds: &[ColumnKind::Integer, ColumnKind::Json],
        key: &["id"],
        indexes: &[],
        scope: Scope::PerRelease,
        scale,
    };
    let schema =
        "CREATE TABLE IF NOT EXISTS sources (\n);\nCREATE TABLE IF NOT EXISTS entries (\n);";
    let tables = [
        table("sources", Scale::Source),
        table("entries", Scale::Entry),
    ];
    let refused = super::schema(&tables, schema);
    assert_eq!(
        refused,
        vec![Refusal {
            file: SCHEMA_FILE.to_owned(),
            line: 3,
            what: "entries.flags".to_owned(),
            why: "a JSON column in a table per catalogue entry: give its values rows of their own"
                .to_owned(),
        }]
    );
}

#[test]
fn the_release_schema_holds_no_json_at_scale() {
    assert_eq!(super::schema(&TABLES, SCHEMA), vec![]);
}

#[test]
fn rust_literals_are_read_with_their_offsets_and_comments_characters_and_lifetimes_are_not() {
    let code = include_str!("literals.rs.txt");
    let literals = rust::literals(code);
    let texts: Vec<&str> = literals.iter().map(|(_, t)| t.trim()).collect();
    assert_eq!(
        texts,
        [
            "SELECT a,",
            "b FROM t",
            "x  \"y \" z",
            "raw \"quoted\"",
            "bytes",
            "after 'a' and ' \"'",
        ]
    );
    for (start, text) in &literals {
        assert_eq!(code.as_bytes()[start + text.len()], b'"', "{text}");
    }
    let (start, text) = &literals[1];
    let at = start + text.find("FROM").unwrap();
    assert_eq!(&code[at..at + 4], "FROM");
    assert_eq!(line_of(code, at), 5);
}

#[test]
fn a_refusal_in_rust_is_placed_at_its_line_in_the_file() {
    let code = reading(
        "fn f() {\n    // READ(e.via_source, '$') in a comment\n    let q = \"SELECT 1, \\\n        \
         READ(e.via_source, '$') FROM entries e\";\n}\n",
    );
    let found = in_rust(&code);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, 4);
}

#[test]
fn a_refusal_in_yaml_is_placed_at_its_line_in_the_file() {
    let yaml = reading(
        "checks:\n  - id: a\n    sql: |\n      SELECT 1\n      FROM entries e\n  - id: b\n    \
         sql: |\n      SELECT count(*)\n      FROM entries e\n      WHERE READ(e.via_source, '$.x') = 1\n",
    );
    let found = in_yaml(&yaml).unwrap();
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, 10);
}
