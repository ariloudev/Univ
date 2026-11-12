//! A store of schema 7 brought to schema 8 in place (keep-json-out-of-the-tables-that-grow-with-
//! a-releases-objects): release 2 of the fixture as schema 7 held it, migrated, reads every
//! object as the same release built under schema 8 does; each JSON shape schema 7 held becomes
//! rows that read back; a second run and a store at 8 change nothing; and a run that fails
//! part way leaves the database in hand at 7, which the next run migrates.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test helpers unwrap and panic: a failure there is the test failing"
)]

use rusqlite::Connection;
use rusqlite::types::ValueRef;
use serde_json::{Number, Value, json};
use univ_ingest::release::schema8::{holds_json, migrate_databases};
use univ_model::api::objects::ObjectRecord;
use univ_model::entry::{PositionErrors, PositionUncertainty};
use univ_model::redshift::RedshiftFrame;
use univ_model::release::read::{ObjectRows, Row, Vocabulary, object};
use univ_model::release::{SCHEMA, valid_in};
use univ_model::units::Unit;
use univ_model::value::{KindOfValue, Quantity, Uncertainty, Value as Measured};

/// The schema of version 7, and release 2 of the fixture as it held it.
const SCHEMA_7: &str = include_str!("fixtures/schema-7.sql");
const OBJECTS_7: &str = include_str!("fixtures/schema-7-objects.sql");
/// The same release built under schema 8, as Univ Server's tests read it.
const OBJECTS_8: &str = include_str!("../../univ-api/src/testkit_objects.sql");

const RELEASE_2: &str = r#"
INSERT INTO releases VALUES (2, '2026-11-02T08:00:00Z', '2026-11-02T08:30:00Z', NULL,
  '{"name":"Planck 2018","hubble_constant":67.66,"omega_matter":0.3111}', 300.0, 'm', 'i', 2,
  'rules', 'rule', 'name', '["hipparcos-2007","twomrs"]', 12, 2, 1, 4, 1200);
INSERT INTO latest VALUES (1, 2, '2026-11-02T08:30:00Z');
INSERT INTO schema_version (version) VALUES (7);
"#;

fn store(schema: &str, objects: &str) -> Connection {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch(schema).unwrap();
    db.execute_batch(RELEASE_2).unwrap();
    db.execute_batch(objects).unwrap();
    db
}

fn int(db: &Connection, sql: &str) -> i64 {
    db.query_row(sql, [], |r| r.get(0)).unwrap()
}

/// The rows of `sql` with parameter `id`, as D1 gives them.
fn rows(db: &Connection, sql: &str, id: i64) -> Vec<Row> {
    let mut q = db.prepare(sql).unwrap();
    let names: Vec<String> = q.column_names().map(str::to_owned).collect();
    q.query_map([id], |r| {
        let mut row = Row::new();
        for (i, name) in names.iter().enumerate() {
            let value = match r.get_ref(i)? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(n) => Value::Number(n.into()),
                ValueRef::Real(x) => Value::Number(Number::from_f64(x).unwrap()),
                ValueRef::Text(t) => Value::String(String::from_utf8(t.to_vec()).unwrap()),
                ValueRef::Blob(_) => unreachable!("a release holds no blob"),
            };
            row.insert(name.clone(), value);
        }
        Ok(row)
    })
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

/// Release 2's vocabulary.
fn vocabulary(db: &Connection) -> Vocabulary {
    let valid = valid_in("", 2);
    let pairs = |sql: String| -> Vec<(i64, String)> {
        let mut q = db.prepare(&sql).unwrap();
        q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    let chains = pairs(format!("SELECT id, steps FROM chains WHERE {valid}"))
        .into_iter()
        .map(|(id, steps)| (id, serde_json::from_str(&steps).unwrap()));
    Vocabulary::new(
        2,
        univ_model::cosmology::Cosmology::planck_2018(),
        "rule".to_owned(),
        pairs(format!("SELECT id, term FROM terms WHERE {valid}")),
        chains,
        pairs(format!("SELECT n, id FROM sources WHERE {valid}")),
    )
}

/// Object `id` of release 2 of `db`.
fn read(db: &Connection, id: i64) -> ObjectRecord {
    read_in(db, db, id)
}

/// Object `id` of release 2 of the shard `db`, the catalogue `catalogue` naming its terms.
fn read_in(catalogue: &Connection, db: &Connection, id: i64) -> ObjectRecord {
    let v = vocabulary(catalogue);
    let of = |table: &str, key: &str| {
        rows(
            db,
            &format!(
                "SELECT * FROM {table} WHERE {key} = ?1 AND {}",
                valid_in("", 2)
            ),
            id,
        )
    };
    let object_row = of("objects", "id").remove(0);
    let (entries, measurements) = (of("entries", "object_id"), of("measurements", "object_id"));
    let (details, flags) = (
        of("measurement_details", "object_id"),
        of("entry_flags", "object_id"),
    );
    let (references, links) = (
        of("entry_references", "object_id"),
        of("object_links", "object_id"),
    );
    let (relations, designations) = (
        of("object_relations", "object_id"),
        rows(
            db,
            &format!(
                "SELECT source, identifier FROM designations WHERE object_id = ?1 AND {}",
                valid_in("", 2)
            ),
            id,
        ),
    );
    let names = rows(
        db,
        "SELECT source, name FROM names WHERE object_id = ?1",
        id,
    );
    object(
        &v,
        &ObjectRows {
            object: &object_row,
            entries: &entries,
            measurements: &measurements,
            measurement_details: &details,
            entry_flags: &flags,
            entry_references: &references,
            links: &links,
            relations: &relations,
            designations: &designations,
            names: &names,
        },
    )
    .unwrap_or_else(|e| panic!("object {id}: {e}"))
}

fn ids(db: &Connection) -> Vec<i64> {
    let mut q = db.prepare("SELECT id FROM objects ORDER BY id").unwrap();
    q.query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

/// Every row of every table of `db`, as text, to tell whether anything changed.
fn dump(db: &Connection) -> Vec<String> {
    let mut q = db
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .unwrap();
    let tables: Vec<String> = q
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let mut out = Vec::new();
    for t in tables {
        let mut q = db.prepare(&format!("SELECT * FROM {t}")).unwrap();
        let n = q.column_count();
        let mut text: Vec<String> = q
            .query_map([], |r| {
                Ok((0..n)
                    .map(|i| format!("{:?}", r.get_ref(i).unwrap()))
                    .collect::<Vec<_>>()
                    .join("|"))
            })
            .unwrap()
            .map(|r| format!("{t}: {}", r.unwrap()))
            .collect();
        text.sort();
        out.extend(text);
    }
    out
}

fn migrated(db: &Connection) -> bool {
    migrate_databases(db, &[], &mut |_| {}).unwrap()
}

#[test]
fn a_store_of_schema_7_reads_every_object_as_schema_8_once_migrated_and_again_changes_nothing() {
    let old = store(SCHEMA_7, OBJECTS_7);
    let new = store(SCHEMA, OBJECTS_8);
    assert!(holds_json(&old).unwrap());
    assert!(!holds_json(&new).unwrap());
    assert!(migrated(&old));
    assert!(!holds_json(&old).unwrap());
    assert_eq!(int(&old, "SELECT MAX(version) FROM schema_version"), 8);
    // Flags, spectral types and papers were JSON; they are rows now.
    assert!(int(&old, "SELECT COUNT(*) FROM entry_flags") > 0);
    assert_eq!(
        int(&old, "SELECT COUNT(*) FROM entry_flags"),
        int(&new, "SELECT COUNT(*) FROM entry_flags")
    );
    assert_eq!(ids(&old), ids(&new));
    for id in ids(&new) {
        assert_eq!(read(&old, id), read(&new, id), "object {id}");
    }
    let before = dump(&old);
    assert!(!migrated(&old));
    assert_eq!(dump(&old), before);
}

#[test]
fn a_store_of_schema_8_is_left_alone() {
    let new = store(SCHEMA, OBJECTS_8);
    let before = dump(&new);
    assert!(!migrated(&new));
    assert_eq!(dump(&new), before);
}

/// A value in degrees, measured, with no uncertainty reported.
fn degrees(x: f64) -> Measured {
    let q = Quantity::new(x, Unit::Degree, Uncertainty::NoneReported).unwrap();
    Measured {
        native: q,
        normalized: q,
        kind: KindOfValue::Measured,
    }
}

/// The first `(object, entry, n)` of a measurement of `quantity` in `db`.
fn first(db: &Connection, quantity: &str) -> (i64, i64, i64) {
    db.query_row(
        "SELECT object_id, entry, n FROM measurements m JOIN terms t ON t.id = m.quantity \
         WHERE t.term = ?1 ORDER BY object_id LIMIT 1",
        [quantity],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .unwrap()
}

#[test]
fn each_json_shape_of_schema_7_becomes_rows_that_read_back() {
    let old = store(SCHEMA_7, OBJECTS_7);
    let (z_object, z_entry, z_n) = first(&old, "redshift");
    let (d_object, d_entry, d_n) = first(&old, "distance");
    let errors = PositionUncertainty::Published(Box::new(PositionErrors {
        ra: degrees(0.001),
        dec: degrees(0.002),
    }));
    let also = Quantity::new(25.1, Unit::Magnitude, Uncertainty::Symmetric { sigma: 0.2 }).unwrap();
    let extra = json!({
        "via": {"source": "hipparcos-2007", "identifier": "999"},
        "position_uncertainty": errors,
    });
    // A link in the shape written before the `by` tag, its match method after the tag, and one
    // in the shape since; a relation.
    let anchor = r#"{"source":"hipparcos-2007","identifier":"1"}"#;
    let other = r#"{"source":"hipparcos-1997","identifier":"1"}"#;
    let reason = format!(
        r#""with":{other},"separation_arcsec":2.5,"difference":null,"confidence":0.4}},"confidence":0.4}}"#
    );
    let links = format!(
        r#"[{{"entry":{anchor},"candidate":{other},"reason":{{"method":"match","method":"position",{reason},{{"entry":{anchor},"candidate":{other},"reason":{{"by":"match","method":"position",{reason}]"#
    );
    let relations = format!(
        r#"[{{"kind":"member_of","entry":{anchor},"other":{other},"scheme":"group","identifier":"7"}}]"#
    );
    old.execute_batch(&format!(
        "UPDATE entries SET extra = '{}' WHERE object_id = 6 AND entry = 0;
         UPDATE measurements SET extra = '{{\"published_frame\":\"barycentric\"}}'
           WHERE object_id = {z_object} AND entry = {z_entry} AND n = {z_n}
           AND quantity = (SELECT id FROM terms WHERE term = 'redshift');
         UPDATE measurements SET extra = '{}'
           WHERE object_id = {d_object} AND entry = {d_entry} AND n = {d_n}
           AND quantity = (SELECT id FROM terms WHERE term = 'distance');
         UPDATE objects SET links = '{links}', relations = '{relations}' WHERE id = 6;
         INSERT INTO ids_replaced VALUES (5, 2, 'split_into', '[10,11]');",
        extra,
        json!({"also_published": also, "assumed_cosmology": {"hubble_constant": 70.0}}),
    ))
    .unwrap();
    assert!(migrated(&old));

    let six = read(&old, 6);
    let entry = &six.entries[0];
    let position = entry.position.as_ref().unwrap();
    assert_eq!(
        position.uncertainty, errors,
        "published errors become measurements"
    );
    let held = int(
        &old,
        "SELECT COUNT(*) FROM measurements WHERE object_id = 6",
    );
    assert_eq!(
        i64::try_from(six.measurements.len()).unwrap(),
        held - 2,
        "and are no measurement of the record"
    );
    assert_eq!(
        int(
            &old,
            "SELECT COUNT(*) FROM entries WHERE object_id = 6 AND entry = 0 \
                   AND via_identifier = '999' AND via_source IS NOT NULL"
        ),
        1
    );
    assert_eq!(six.links.len(), 2);
    assert_eq!(six.links[0], six.links[1], "the old shape reads as the new");
    assert_eq!(six.relations[0].identifier, "7");

    let z = read(&old, z_object);
    let redshift = z
        .measurements
        .iter()
        .find_map(|m| m.redshift.as_ref())
        .unwrap();
    assert_eq!(redshift.published_frame, RedshiftFrame::Barycentric);
    let d = read(&old, d_object);
    let distance = d
        .measurements
        .iter()
        .find_map(|m| m.distance.as_ref().filter(|d| d.also_published.is_some()))
        .unwrap();
    assert_eq!(distance.also_published, Some(also));
    assert_eq!(distance.assumed_cosmology.unwrap().hubble_constant, 70.0);
    let mut q = old
        .prepare("SELECT n, release, replaced_by FROM id_replacements WHERE id = 5 ORDER BY n")
        .unwrap();
    let by: Vec<(i64, i64, i64)> = q
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(by, [(0, 2, 10), (1, 2, 11)]);
}

#[test]
fn a_run_that_fails_leaves_the_database_in_hand_at_7_and_the_next_finishes_it() {
    let catalogue = store(SCHEMA_7, OBJECTS_7);
    let shard = store(SCHEMA_7, OBJECTS_7);
    // The catalogue's ledger holds a replacement that does not read; the shard is migrated
    // first, and the catalogue, in hand when it fails, is left as it was.
    catalogue
        .execute_batch("INSERT INTO ids_replaced VALUES (5, 2, 'retired', 'not json');")
        .unwrap();
    let before = dump(&catalogue);
    let e = migrate_databases(&catalogue, &[&shard], &mut |_| {}).unwrap_err();
    assert!(e.contains("ids_replaced (5)"), "{e}");
    assert!(!holds_json(&shard).unwrap());
    // But for the terms the shard named, written before it was committed.
    let after: Vec<String> = dump(&catalogue)
        .into_iter()
        .filter(|r| !before.contains(r))
        .collect();
    assert!(after.iter().all(|r| r.starts_with("terms: ")), "{after:?}");
    assert!(holds_json(&catalogue).unwrap());
    catalogue
        .execute_batch("UPDATE ids_replaced SET replaced_by = '[]' WHERE id = 5;")
        .unwrap();
    assert!(migrate_databases(&catalogue, &[&shard], &mut |_| {}).unwrap());
    assert!(!holds_json(&catalogue).unwrap());
    for id in ids(&shard) {
        assert_eq!(
            read_in(&catalogue, &shard, id),
            read(&catalogue, id),
            "object {id}"
        );
    }
}
