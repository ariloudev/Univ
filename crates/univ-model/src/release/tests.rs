use super::*;
use crate::release::rows::{IdReplacedRow, ObjectCellRow, name_key};

/// The columns of `table` in the schema, and its primary key.
fn declared(table: &str) -> (Vec<String>, Vec<String>) {
    let head = format!("CREATE TABLE IF NOT EXISTS {table} (\n");
    let start = SCHEMA
        .find(&head)
        .unwrap_or_else(|| panic!("{table} is declared"))
        + head.len();
    let body = &SCHEMA[start..start + SCHEMA[start..].find("\n)").unwrap()];
    let mut columns = Vec::new();
    let mut key = Vec::new();
    for line in body.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("PRIMARY KEY (") {
            key = rest
                .trim_end_matches([')', ','])
                .split(", ")
                .map(str::to_owned)
                .collect();
        } else if !line.is_empty() && !line.starts_with("--") {
            let name = line.split_whitespace().next().unwrap();
            columns.push(name.to_owned());
            if line.contains("PRIMARY KEY") {
                key = vec![name.to_owned()];
            }
        }
    }
    (columns, key)
}

#[test]
fn every_row_type_matches_its_table_in_the_schema() {
    for table in TABLES {
        let (columns, key) = declared(table.name);
        // Publishing and pruning set these later; a build leaves them NULL.
        let later = ["published_at", "pruned_at"];
        let written: Vec<&str> = columns
            .iter()
            .map(String::as_str)
            .filter(|c| !(table.name == "releases" && later.contains(c)))
            .collect();
        assert_eq!(written, table.columns, "{}", table.name);
        assert_eq!(key, table.key, "{}", table.name);
        // A release's rows lead their key with it; the ledger's `ids_issued` is keyed by the
        // release too, but kept across releases.
        assert!(
            table.scope != Scope::PerRelease || table.key[0] == "release",
            "{}",
            table.name
        );
        // A table whose key is one integer column is a rowid table, every other WITHOUT ROWID.
        let rowid = table.tree() == Tree::Table;
        let declared_without = SCHEMA[SCHEMA
            .find(&format!("TABLE IF NOT EXISTS {} (", table.name))
            .unwrap()..]
            .split(';')
            .next()
            .unwrap()
            .contains("WITHOUT ROWID");
        assert_eq!(rowid, !declared_without, "{}", table.name);
        for index in table.indexes {
            let declared = format!("ON {} ({});", table.name, index.join(", "));
            assert!(SCHEMA.contains(&declared), "{declared}");
        }
    }
    for kept in ["latest", "schema_version"] {
        declared(kept);
    }
}

#[test]
fn version_3_adds_the_source_and_structure_details_per_release() {
    assert_eq!(SCHEMA_VERSION, 3);
    for (name, columns) in [
        (
            "source_details",
            &[
                "release",
                "source",
                "bibcode",
                "doi",
                "reference_note",
                "sky",
                "radial",
            ][..],
        ),
        (
            "structure_details",
            &[
                "release",
                "id",
                "shape",
                "derivation",
                "outline",
                "rescaled",
            ][..],
        ),
    ] {
        let (declared, key) = declared(name);
        assert_eq!(declared, columns, "{name}");
        assert_eq!(key, columns[..2], "{name}");
        let table = TABLES.iter().find(|t| t.name == name).unwrap();
        // Pruning deletes the rows of every per-release table.
        assert_eq!(table.scope, Scope::PerRelease, "{name}");
        assert_eq!(table.columns, columns, "{name}");
    }
}

#[test]
fn every_index_in_the_schema_is_counted_in_the_estimate() {
    let declared = SCHEMA.matches("CREATE INDEX").count();
    let counted: usize = TABLES.iter().map(|t| t.indexes.len()).sum();
    assert_eq!(declared, counted);
}

#[test]
fn a_row_gives_its_values_in_column_order() {
    let row = ObjectCellRow {
        release: 3,
        domain: 2,
        ix: -1,
        iy: 0,
        iz: 7,
        object_id: 42,
    };
    assert_eq!(
        row.values(),
        vec![
            SqlValue::Integer(3),
            SqlValue::Integer(2),
            SqlValue::Integer(-1),
            SqlValue::Integer(0),
            SqlValue::Integer(7),
            SqlValue::Integer(42),
        ]
    );
    let replaced = IdReplacedRow {
        id: 5,
        release: 2,
        change: "merged_into".into(),
        replaced_by: "[3]".into(),
    };
    let statements = super::sql::inserts(IdReplacedRow::TABLE, &[replaced.values()]);
    assert_eq!(
        statements,
        [
            "INSERT OR IGNORE INTO ids_replaced (id, release, change, replaced_by) VALUES\n\
          (5, 2, 'merged_into', '[3]');\n"
        ]
    );
}

#[test]
fn a_name_key_ignores_case_and_white_space() {
    assert_eq!(
        name_key(" Large  Magellanic\tCloud"),
        "largemagellaniccloud"
    );
    assert_eq!(name_key("M 31"), name_key("m31"));
}
