use super::*;
use crate::release::rows::{IdReplacedRow, ObjectCellRow, name_key};

/// The columns of `table` in the schema, but `retired`, which only a later release sets, and
/// its primary key.
fn declared(table: &str) -> (Vec<String>, Vec<String>) {
    let (mut columns, key) = declared_all(table);
    columns.retain(|c| c != RETIRED);
    (columns, key)
}

/// A key of version 5, which led with `release`, as version 6 declares it: the release last.
fn shared_key(v5: &[&str]) -> Vec<String> {
    v5[1..]
        .iter()
        .chain(&v5[..1])
        .map(|c| (*c).to_owned())
        .collect()
}

/// Every column of `table` in the schema, and its primary key.
fn declared_all(table: &str) -> (Vec<String>, Vec<String>) {
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
fn version_8_keeps_json_out_of_every_table_per_object_or_per_entry() {
    assert_eq!(SCHEMA_VERSION, 8);
    for table in &TABLES {
        let json: Vec<&str> = table
            .columns
            .iter()
            .zip(table.kinds)
            .filter(|(_, kind)| **kind == ColumnKind::Json)
            .map(|(column, _)| *column)
            .collect();
        assert!(
            table.scale.allows_json() || json.is_empty(),
            "{} ({}) holds JSON in {json:?}",
            table.name,
            table.scale.words()
        );
    }
    for name in [
        "entry_flags",
        "entry_references",
        "measurement_details",
        "object_links",
        "object_relations",
        "memberships",
        "id_replacements",
    ] {
        let table = TABLES.iter().find(|t| t.name == name).unwrap();
        assert!(!table.scale.allows_json(), "{name}");
        assert_eq!(table.columns, declared(name).0, "{name}");
    }
}

#[test]
fn every_table_names_its_scale_in_the_schema() {
    for table in &TABLES {
        let create = format!("CREATE TABLE IF NOT EXISTS {} (", table.name);
        let at = SCHEMA
            .find(&create)
            .unwrap_or_else(|| panic!("{} is not in the schema", table.name));
        let above = SCHEMA[..at].lines().last();
        assert_eq!(
            above,
            Some(format!("-- Scale: {}.", table.scale.words()).as_str()),
            "{}",
            table.name
        );
    }
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
        // A row shared between releases ends its key with the release that adds it, and
        // declares `retired` last; the ledger's `ids_issued` is keyed by the release too, but
        // kept across releases.
        if table.scope == Scope::PerRelease {
            assert_eq!(table.key.last(), Some(&"release"), "{}", table.name);
            assert_eq!(
                declared_all(table.name).0.last().map(String::as_str),
                Some(RETIRED),
                "{}",
                table.name
            );
        }
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
        assert_eq!(key, shared_key(&columns[..2]), "{name}");
        let table = TABLES.iter().find(|t| t.name == name).unwrap();
        // Rows of every per-release table are shared between the releases they are valid in.
        assert_eq!(table.scope, Scope::PerRelease, "{name}");
        assert_eq!(table.columns, columns, "{name}");
    }
}

#[test]
fn version_4_adds_the_source_terms_per_release() {
    let columns = [
        "release",
        "source",
        "non_commercial",
        "permission",
        "catalogue_uncertainties",
    ];
    let (declared, key) = declared("source_terms");
    assert_eq!(declared, columns);
    assert_eq!(key, shared_key(&columns[..2]));
    let table = TABLES.iter().find(|t| t.name == "source_terms").unwrap();
    assert_eq!(table.scope, Scope::PerRelease);
    assert_eq!(table.columns, columns);
}

#[test]
fn version_7_adds_how_each_source_is_read_per_release() {
    let columns = [
        "release",
        "source",
        "designation_form",
        "designation_cite",
        "rank",
        "share_alike",
        "flags",
    ];
    let (declared, key) = declared("source_reading");
    assert_eq!(declared, columns);
    assert_eq!(key, shared_key(&columns[..2]));
    let table = TABLES.iter().find(|t| t.name == "source_reading").unwrap();
    assert_eq!(table.scope, Scope::PerRelease);
    assert_eq!(table.columns, columns);
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
    };
    let statements = super::statements::inserts(IdReplacedRow::TABLE, &[replaced.values()]);
    assert_eq!(
        statements,
        [
            "INSERT OR IGNORE INTO ids_replaced (id, release, change) VALUES\n\
          (5, 2, 'merged_into');\n"
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

#[test]
fn version_5_adds_the_shards_index_per_release() {
    for (name, columns, key) in [
        ("object_shards", &["release", "id", "shard"][..], 2),
        (
            "shard_cells",
            // Keyed by its first cell too: a shard may hold several runs of one domain.
            &["release", "shard", "domain", "first", "last"][..],
            4,
        ),
        (
            "release_shards",
            &["release", "shard", "objects", "estimated_bytes"][..],
            2,
        ),
    ] {
        let (declared, declared_key) = declared(name);
        assert_eq!(declared, columns, "{name}");
        assert_eq!(declared_key, shared_key(&columns[..key]), "{name}");
        let table = TABLES.iter().find(|t| t.name == name).unwrap();
        assert_eq!(table.scope, Scope::PerRelease, "{name}");
        assert_eq!(table.columns, columns, "{name}");
        assert!(!table.in_shard(), "{name} is in the catalogue");
    }
    let sharded: Vec<&str> = TABLES
        .iter()
        .filter(|t| t.in_shard())
        .map(|t| t.name)
        .collect();
    assert_eq!(sharded, super::shards::SHARD_TABLES);
}

#[test]
fn a_tables_estimate_over_the_databases_adds_up_its_rows_and_fingerprint() {
    use super::fingerprint::ColumnTotal;
    use super::manifest::{TableEstimate, summed};
    let table = |rows, total, sha: Option<&str>| TableEstimate {
        table: "objects".to_owned(),
        rows,
        estimated_bytes: rows * 10,
        added: None,
        retired: None,
        fingerprint: vec![ColumnTotal {
            column: "id".to_owned(),
            total,
            magnitude: None,
            non_null: Some(rows),
        }],
        content_sha256: sha.map(str::to_owned),
    };
    let (a, b, empty) = (
        table(3, 6.0, Some("a")),
        table(2, 9.0, Some("b")),
        table(0, 0.0, Some("empty")),
    );
    let sum = summed(&[&empty, &a, &b]).unwrap();
    assert_eq!((sum.rows, sum.estimated_bytes), (5, 50));
    assert_eq!(sum.fingerprint[0].total, 15.0);
    assert_eq!(sum.fingerprint[0].non_null, Some(5));
    // Two databases hold rows: no single digest stands for them.
    assert_eq!(sum.content_sha256, None);
    let one = summed(&[&empty, &a]).unwrap();
    assert_eq!(one.content_sha256.as_deref(), Some("a"));
    assert!(summed(&[]).is_none());
}

#[test]
fn version_6_shares_rows_and_maps_files_by_hash() {
    assert_eq!(super::SHARED_ROWS_SINCE, 6);
    let (columns, key) = declared_all("release_files");
    assert_eq!(columns, ["release", "path", "sha256", RETIRED]);
    assert_eq!(key, ["path", "release"]);
    assert_eq!(
        valid_in("o.", 4),
        "o.release <= 4 AND (o.retired IS NULL OR o.retired > 4)"
    );
    assert_eq!(Scope::PerRelease.filter(4), valid_in("", 4));
    assert_eq!(crate::valid_in!("o.", "?1"), valid_in("o.", "?1"));
    // Every index is led by its own columns, the release after them.
    for table in TABLES {
        for index in table.indexes {
            assert_eq!(index.last(), Some(&"release"), "{}", table.name);
        }
    }
}

#[test]
fn the_query_for_schema_7_json_names_exactly_its_columns() {
    let query = super::SCHEMA_7_JSON_HELD;
    // Each table once, each of its columns once, and nothing D1 refuses a query.
    assert!(!query.contains("sqlite_master"));
    let tables: std::collections::BTreeSet<&str> =
        super::SCHEMA_7_JSON.iter().map(|(t, _)| *t).collect();
    assert_eq!(query.matches("pragma_table_info(").count(), tables.len());
    for (table, column) in super::SCHEMA_7_JSON {
        let at = query
            .find(&format!("pragma_table_info('{table}')"))
            .unwrap();
        let clause = &query[at..query[at..]
            .find(" UNION ALL")
            .map_or(query.len(), |e| at + e)];
        assert!(
            clause.contains(&format!("'{column}'")),
            "{table}.{column}: {clause}"
        );
    }
}

#[test]
fn every_table_of_an_objects_rows_is_in_its_shard_but_its_names_and_its_shards_index() {
    // A name is looked up across every object, and the index finds each object's shard.
    let catalogue = ["names", "object_shards"];
    for table in &TABLES {
        assert_eq!(
            table.in_shard(),
            table.grows_with_objects() && !catalogue.contains(&table.name),
            "{}",
            table.name
        );
        // A writer forgets their keys once an object's rows are written: unique within each
        // object, they are unique, its id being unique.
        if table.grows_with_objects() {
            assert!(
                table.key.iter().any(|c| *c == "id" || *c == "object_id"),
                "{}",
                table.name
            );
        }
    }
}
