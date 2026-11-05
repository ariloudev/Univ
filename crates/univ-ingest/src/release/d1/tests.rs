use super::*;
use univ_model::release::rows::{IdReplacedRow, IdsIssuedRow, ReleaseRow};

#[test]
fn the_files_load_the_schema_then_each_table_in_order() {
    let mut sink = InMemory::default();
    let mut writer = D1Writer::new(&mut sink, 1).unwrap();
    writer
        .add(&IdsIssuedRow {
            release: 1,
            first: 1,
            last: 9,
        })
        .unwrap();
    writer.add_all::<ReleaseRow>(&[]).unwrap();
    let rows = writer.rows("ids_issued");
    let estimate = writer.finish().unwrap().estimate;
    let files = sink.files;
    let paths: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(
        paths,
        [
            "d1/catalogue/0000-schema.sql",
            "d1/shard-00/0000-schema.sql",
            "d1/catalogue/0012-ids_issued-001.sql"
        ]
    );
    let text = String::from_utf8(files[2].1.clone()).unwrap();
    assert_eq!(
        text,
        "INSERT OR IGNORE INTO ids_issued (release, first, last) VALUES\n(1, 1, 9);\n"
    );
    assert_eq!(estimate.tables.len(), TABLES.len());
    assert!(estimate.estimated_bytes > 0 && estimate.estimated_bytes < 100);
    assert_eq!(rows, 1);
}

#[test]
fn a_table_is_split_into_parts_and_its_keys_and_numbers_checked() {
    let mut sink = InMemory::default();
    let mut writer = D1Writer::new(&mut sink, 1).unwrap();
    let replaced = |id| IdReplacedRow {
        id,
        release: 1,
        change: "retired".into(),
        replaced_by: "x".repeat(20_000),
    };
    // About 20 kB a row: 2,000 rows fill more than one 32 MiB part.
    for id in 0..2_000 {
        writer.add(&replaced(id)).unwrap();
    }
    writer.add(&replaced(7)).unwrap();
    let mut not_finite = ReleaseRow {
        number: 1,
        built_at: String::new(),
        cosmology: String::new(),
        peculiar_velocity_km_s: f64::NAN,
        model_version: String::new(),
        ingest_version: String::new(),
        schema_version: 2,
        matching_rules: String::new(),
        ranking_rule: String::new(),
        ranking_rule_name: String::new(),
        sources: String::new(),
        objects: 0,
        structures: 0,
        grids: 0,
        tiles: 0,
        d1_estimated_bytes: 0,
    };
    writer.add(&not_finite).unwrap();
    not_finite.number = 2;
    writer.add(&not_finite).unwrap();
    let written = writer.finish().unwrap();
    // A part is put once full; the rest when the writer finishes, in table order.
    let mut parts: Vec<&String> = sink.files.iter().map(|(p, _)| p).collect();
    parts.sort();
    assert_eq!(
        parts[1..4],
        [
            "d1/catalogue/0001-releases-001.sql",
            "d1/catalogue/0013-ids_replaced-001.sql",
            "d1/catalogue/0013-ids_replaced-002.sql"
        ]
    );
    assert!(sink.files.iter().all(|(_, b)| b.len() <= MAX_FILE_BYTES));
    let repeated = written.failed.iter().find(|f| f.check == D1_KEYS).unwrap();
    assert!(
        repeated.cause.contains("1 row(s) repeat"),
        "{}",
        repeated.cause
    );
    assert!(repeated.cause.contains("(id = 7)"), "{}", repeated.cause);
    let nan = written
        .failed
        .iter()
        .find(|f| f.check == D1_FINITE)
        .unwrap();
    assert_eq!(
        nan.cause,
        "the catalogue database: table releases, row (number = 1), column peculiar_velocity_km_s: NaN is not a \
             finite number",
        "the first only"
    );
    let table = &written.estimate.tables[12];
    assert_eq!((table.table.as_str(), table.rows), ("ids_replaced", 2_001));
}

#[test]
fn an_objects_rows_go_to_its_shard_and_the_rest_to_the_catalogue() {
    use univ_model::release::rows::{DesignationRow, NameRow, ObjectShardRow};
    let mut sink = InMemory::default();
    let mut writer = D1Writer::new(&mut sink, 2).unwrap();
    let designation = DesignationRow {
        release: 1,
        source: 0,
        identifier: "NGC 224".into(),
        object_id: 7,
    };
    // A row of a shard table needs a shard, and one the release has.
    assert!(writer.add(&designation).is_err());
    writer.set_shard(2);
    assert!(writer.add(&designation).is_err());
    writer.set_shard(1);
    writer.add(&designation).unwrap();
    writer
        .add(&NameRow {
            release: 1,
            key: "m31".into(),
            object_id: 7,
            source: 0,
            name: "M31".into(),
        })
        .unwrap();
    writer
        .add(&ObjectShardRow {
            release: 1,
            id: 7,
            shard: 1,
        })
        .unwrap();
    assert!(writer.estimated_bytes_in(Database::Shard(1)) > 0);
    assert_eq!(writer.estimated_bytes_in(Database::Shard(0)), 0);
    let written = writer.finish().unwrap();
    assert_eq!(written.shards.len(), 2);
    let rows =
        |e: &D1Estimate, table: &str| e.tables.iter().find(|t| t.table == table).unwrap().rows;
    assert_eq!(rows(&written.shards[1], "designations"), 1);
    assert_eq!(rows(&written.estimate, "designations"), 0);
    assert_eq!(rows(&written.estimate, "names"), 1);
    assert_eq!(rows(&written.estimate, "object_shards"), 1);
    let mut paths: Vec<&str> = sink.files.iter().map(|(p, _)| p.as_str()).collect();
    paths.sort_unstable();
    assert_eq!(
        paths,
        [
            "d1/catalogue/0000-schema.sql",
            "d1/catalogue/0011-names-001.sql",
            "d1/catalogue/0022-object_shards-001.sql",
            "d1/shard-00/0000-schema.sql",
            "d1/shard-01/0000-schema.sql",
            "d1/shard-01/0010-designations-001.sql",
        ]
    );
}
