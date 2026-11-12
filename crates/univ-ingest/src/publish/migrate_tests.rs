//! The rebuild of a table and the rewrite of a grid header, of the migration to shared rows.

use super::*;

#[test]
fn a_table_left_only_under_its_rebuilt_name_takes_its_name_back_and_is_rebuilt_whole() {
    let db = rusqlite::Connection::open_in_memory().unwrap();
    let objects = TABLES.iter().find(|t| t.name == "objects").unwrap();
    db.execute_batch(&create_as(objects, "objects__v6").unwrap())
        .unwrap();
    let columns = objects.columns.join(", ");
    let values = vec!["1"; objects.columns.len()].join(", ");
    db.execute_batch(&format!(
        "INSERT INTO objects__v6 ({columns}) VALUES ({values})"
    ))
    .unwrap();
    // A run stopped between dropping `objects` and renaming `objects__v6`.
    let held = std::collections::BTreeSet::from(["objects__v6".to_owned()]);
    let sql = rebuild(&held, &Columns::new(), Some(2)).unwrap();
    assert!(
        sql.starts_with("ALTER TABLE objects__v6 RENAME TO objects;\n"),
        "{sql}"
    );
    db.execute_batch(&sql).unwrap();
    let count = |sql: &str| db.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap();
    assert_eq!(count("SELECT COUNT(*) FROM objects WHERE retired = 2"), 1);
    assert_eq!(
        count("SELECT COUNT(*) FROM sqlite_master WHERE name = 'objects__v6'"),
        0
    );
}

#[test]
fn a_grid_header_is_rewritten_to_name_its_samples_by_their_content() {
    let header = |samples: &str, sigma: &str| {
        serde_json::to_vec_pretty(&serde_json::json!({
            "format": "univ-volume-grid",
            "fields": [{
                "samples": samples,
                "uncertainty": { "form": "standard_deviation", "samples": sigma },
            }],
        }))
        .unwrap()
    };
    let old = header(
        "releases/4/grids/g/field-0.f32",
        "releases/4/grids/g/field-0-sigma.f32",
    );
    let shas = BTreeMap::from([
        ("grids/g/field-0.f32", "aa"),
        ("grids/g/field-0-sigma.f32", "bb"),
    ]);
    assert_eq!(
        rename_samples(&old, 4, &shas).unwrap(),
        header(&keys::file("aa"), &keys::file("bb"))
    );
    // A key the manifest does not list is an error, not a guess.
    let stray = header(
        "releases/4/grids/g/field-9.f32",
        "releases/4/grids/g/field-0-sigma.f32",
    );
    assert!(rename_samples(&stray, 4, &shas).is_err());
}
