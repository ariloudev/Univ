//! A release's D1 rows loaded into `SQLite`, which is what D1 runs: two spellings of one name
//! that share a name key are both kept, and the CON-3 estimate of a row errs high against the
//! storage `SQLite` measures, records that overflow their B-tree cell included.
#![expect(
    clippy::unwrap_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

use rusqlite::Connection;
use univ_ingest::release::D1Rows;
use univ_model::release::rows::{NameRow, StructureRow, name_key};
use univ_model::release::{ColumnKind, Row, SCHEMA, SqlValue};

/// A database holding the schema and the D1 files of `rows`, executed in order.
fn loaded(db: &Connection, rows: &D1Rows) {
    for (_, sql) in rows.files() {
        db.execute_batch(&String::from_utf8(sql).unwrap()).unwrap();
    }
}

#[test]
fn two_spellings_of_a_name_that_share_its_key_are_both_kept() {
    let name = |name: &str| NameRow {
        release: 1,
        key: name_key(name),
        object_id: 7,
        source: 0,
        name: name.into(),
    };
    assert_eq!(name_key("M 31"), name_key("M31"));
    let mut rows = D1Rows::default();
    rows.add(&[name("M 31"), name("M31")]);
    let db = Connection::open_in_memory().unwrap();
    loaded(&db, &rows);
    let mut names: Vec<String> = db
        .prepare("SELECT name FROM names WHERE key = 'm31' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    names.sort();
    assert_eq!(names, ["M 31", "M31"]);
}

/// A structure row whose record is `bytes` long, every other text short.
fn structure(i: usize, bytes: usize) -> Vec<SqlValue> {
    let table = StructureRow::TABLE;
    table
        .columns
        .iter()
        .zip(table.kinds)
        .map(|(column, kind)| match (*column, kind) {
            ("release", _) => SqlValue::Integer(1),
            ("id", _) => SqlValue::from(format!("structure-{i:05}")),
            ("record", _) => SqlValue::from("r".repeat(bytes)),
            (_, ColumnKind::Text) => SqlValue::from("void"),
            (_, ColumnKind::Integer) => SqlValue::Integer(1),
            (_, ColumnKind::Real) => SqlValue::Real(1.5),
        })
        .collect()
}

/// The bytes per row `SQLite` takes for `rows`, inserted into the schema in a 4096-byte-page
/// database file, by its page count.
fn measured(rows: &[Vec<SqlValue>]) -> f64 {
    let dir = tempfile::tempdir().unwrap();
    let db = Connection::open(dir.path().join("d1.sqlite")).unwrap();
    db.execute_batch("PRAGMA page_size = 4096;").unwrap();
    db.execute_batch(SCHEMA).unwrap();
    let pages =
        |db: &Connection| -> i64 { db.query_row("PRAGMA page_count", [], |r| r.get(0)).unwrap() };
    let before = pages(&db);
    for statement in StructureRow::TABLE.inserts(rows) {
        db.execute_batch(&statement).unwrap();
    }
    let after = pages(&db);
    #[expect(clippy::cast_precision_loss, reason = "page counts are small")]
    let per_row = ((after - before) * 4096) as f64 / rows.len() as f64;
    per_row
}

#[test]
fn a_record_over_its_cells_limit_is_estimated_high_with_its_overflow_page() {
    // The cases measured in review: records of 1.1, 1.5 and 2.5 kB in a WITHOUT ROWID table
    // each take one overflow page beside their cell, about 4683 bytes a row.
    for bytes in [1_100, 1_500, 2_500] {
        let rows: Vec<Vec<SqlValue>> = (0..400).map(|i| structure(i, bytes)).collect();
        let sqlite = measured(&rows);
        assert!((4_500.0..4_900.0).contains(&sqlite), "{bytes}: {sqlite}");
        #[expect(clippy::cast_precision_loss, reason = "a row's bytes")]
        let estimate = StructureRow::TABLE.estimated_bytes(&rows[0]) as f64;
        assert!(
            estimate >= sqlite && estimate < 1.25 * sqlite,
            "{bytes}: estimated {estimate}, SQLite {sqlite}"
        );
    }
    // A record that fits its cell is estimated within its page fill.
    let rows: Vec<Vec<SqlValue>> = (0..400).map(|i| structure(i, 600)).collect();
    let sqlite = measured(&rows);
    #[expect(clippy::cast_precision_loss, reason = "a row's bytes")]
    let estimate = StructureRow::TABLE.estimated_bytes(&rows[0]) as f64;
    assert!(
        estimate >= sqlite && estimate < 2.0 * sqlite,
        "{estimate} {sqlite}"
    );
}
