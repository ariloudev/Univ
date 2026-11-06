//! Rows shared between releases (schema 6): release 3 keeps most of release 2's rows, retires
//! some and replaces others. A read pinned to release 2 answers as it did before release 3 was
//! written and published (AC-128), and a read of release 3 sees the rows it shares and adds
//! and none it retired: for object lookups, volume queries, structures and sources.

use serde_json::{Value, json};
use univ_model::release::cells::CellSpace;

use crate::http::Response;
use crate::query_fixture::{BODY, COSMIC_WEB, galaxy, hits, request, sphere, volume_db};
use crate::testkit::{FIXTURE_OBJECTS, RELEASES, SOURCES, STRUCTURES, Sqlite, body};

/// Writes `rows`, what release 3 adds and retires, then publishes release 3 as `latest`.
fn publish_release_3(db: &Sqlite, rows: &str) {
    db.exec(rows).exec(
        "UPDATE releases SET published_at = '2026-11-03T08:00:00Z' WHERE number = 3;
         UPDATE latest SET release = 3 WHERE one = 1;",
    );
}

/// SQL by which release 3 replaces the rows of `table` that `filter` selects: it retires them
/// and adds copies with `set` applied.
fn replace(table: &str, filter: &str, set: &str) -> String {
    format!(
        "CREATE TEMP TABLE added AS SELECT * FROM {table} WHERE {filter} AND retired IS NULL;
         UPDATE added SET release = 3, {set};
         UPDATE {table} SET retired = 3 WHERE {filter} AND retired IS NULL;
         INSERT INTO {table} SELECT * FROM added;
         DROP TABLE added;"
    )
}

/// The answer to a GET of `path` pinned to `release`.
fn read(db: &Sqlite, path: &str, release: u32) -> Response {
    db.get(path, &format!("release={release}"))
}

/// The JSON answer to a GET of `path` pinned to `release`, which must be a 200.
fn json(db: &Sqlite, path: &str, release: u32) -> Value {
    let response = read(db, path, release);
    let json = body(&response);
    assert_eq!(response.status, 200, "{path} of release {release}: {json}");
    assert_eq!(
        response.header("univ-release"),
        Some(release.to_string().as_str())
    );
    json
}

/// The native values of an object record's measurements, in its order.
fn native_values(record: &Value) -> Vec<Value> {
    record["measurements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["native"]["value"].clone())
        .collect()
}

/// An object record without what it takes from its release's row of `releases` rather than
/// from its own rows: the release's number and the name of its ranking rule.
fn from_rows(mut record: Value) -> Value {
    let object = record.as_object_mut().unwrap();
    object.remove("release");
    object["preferred"].as_object_mut().unwrap().remove("rule");
    record
}

/// Release 3 of Univ Ingest's fixture objects: object 1's measurement of native value 6179
/// (quantity 37) replaced by one of 6180, object 2 retired with every row of it, object 3 kept.
/// Odd ids are in shard 1, even ones in shard 0.
fn objects_db() -> Sqlite {
    let db = Sqlite::new();
    db.exec(RELEASES).exec(FIXTURE_OBJECTS);
    db
}

const OBJECTS_OF_RELEASE_3: &str = "
    UPDATE shard0.objects SET retired = 3 WHERE id = 2;
    UPDATE shard0.entries SET retired = 3 WHERE object_id = 2;
    UPDATE shard0.measurements SET retired = 3 WHERE object_id = 2;
    UPDATE shard0.designations SET retired = 3 WHERE object_id = 2;
    UPDATE shard0.object_cells SET retired = 3 WHERE object_id = 2;
    UPDATE names SET retired = 3 WHERE object_id = 2;
    UPDATE object_shards SET retired = 3 WHERE id = 2;
    INSERT INTO ids_replaced VALUES (2, 3, 'retired', '[]');";

fn objects_of_release_3() -> String {
    format!(
        "{OBJECTS_OF_RELEASE_3}\n{}",
        replace(
            "shard1.measurements",
            "object_id = 1 AND quantity = 37",
            "native_value = 6180.0"
        )
    )
}

#[test]
fn an_object_read_pinned_to_a_release_is_unchanged_by_a_later_release_that_shares_its_rows() {
    let db = objects_db();
    let paths = ["/v1/objects/1", "/v1/objects/2", "/v1/objects/3"];
    let before: Vec<Response> = paths.iter().map(|path| read(&db, path, 2)).collect();
    publish_release_3(&db, &objects_of_release_3());
    for (path, before) in paths.iter().zip(before) {
        let after = read(&db, path, 2);
        assert_eq!(before.status, 200, "{path}");
        assert_eq!(after.status, 200, "{path}");
        assert_eq!(before.body, after.body, "{path}");
    }
}

#[test]
fn a_later_release_reads_the_objects_it_keeps_and_replaces_and_not_those_it_retires() {
    let db = objects_db();
    let kept = json(&db, "/v1/objects/3", 2);
    let replaced = json(&db, "/v1/objects/1", 2);
    publish_release_3(&db, &objects_of_release_3());
    assert_eq!(
        from_rows(json(&db, "/v1/objects/3", 3)),
        from_rows(kept),
        "a shared object reads the same"
    );
    let replacing = json(&db, "/v1/objects/1", 3);
    let (was, is) = (native_values(&replaced), native_values(&replacing));
    assert_eq!(was.len(), is.len(), "one measurement replaced by one");
    let count = |values: &[Value], value: Value| values.iter().filter(|v| **v == value).count();
    assert_eq!(
        (count(&was, json!(6180.0)), count(&is, json!(6180.0))),
        (0, 1)
    );
    assert_eq!(count(&is, json!(6179.0)), count(&was, json!(6179.0)) - 1);
    assert_eq!(
        replacing["entries"], replaced["entries"],
        "its entries shared"
    );
    let (response, problem) = db.get_json("/v1/objects/2", "release=3");
    assert_eq!(response.status, 404, "{problem}");
    assert_eq!(problem["replaced_in"], 3);
}

/// A volume query of `volume`, with `filters`, pinned to `release`: its answer and hits.
fn volume(db: &Sqlite, release: u32, volume: &Value, filters: &Value) -> (Response, Vec<String>) {
    let mut request = request(&json!({"volume": volume, "filters": filters}));
    request.query = format!("release={release}");
    let response = db.answer(&request);
    let page = body(&response);
    assert_eq!(response.status, 200, "{page}");
    (response, hits(&page))
}

/// Release 3 of the volume fixture: galaxy 1001 moved to (150, 0, 0) Mpc, galaxy 1006 added at
/// (101, 0, 0), galaxy 1002's entry retired, and the structure `box-only` retired.
fn volume_of_release_3() -> String {
    let cell = |at: [f64; 3]| CellSpace::CosmicWeb.cell(at).unwrap();
    let (moved, added) = (cell([150.0, 0.0, 0.0]), cell([101.0, 0.0, 0.0]));
    format!(
        "{}
         UPDATE shard1.object_cells SET retired = 3 WHERE object_id = 1001;
         INSERT INTO object_cells VALUES (3, {COSMIC_WEB}, {}, {}, {}, 1001, NULL);
         INSERT INTO objects (release, id, kind, placed, scale_domain, x, y, z)
           VALUES (3, 1006, {BODY}, 1, {COSMIC_WEB}, 101.0, 0.0, 0.0);
         INSERT INTO object_cells VALUES (3, {COSMIC_WEB}, {}, {}, {}, 1006, NULL);
         UPDATE shard0.entries SET retired = 3 WHERE object_id = 1002;
         UPDATE structures SET retired = 3 WHERE id = 'box-only';
         UPDATE structure_cells SET retired = 3 WHERE structure = 'box-only';",
        replace("shard1.objects", "id = 1001", "x = 150.0"),
        moved.ix,
        moved.iy,
        moved.iz,
        added.ix,
        added.iy,
        added.iz,
    )
}

#[test]
fn a_volume_query_pinned_to_a_release_is_unchanged_by_a_later_release_that_shares_its_rows() {
    let db = volume_db(&[
        galaxy(1001, [100.0, 0.0, 0.0]),
        galaxy(1002, [104.99, 0.0, 0.0]),
    ]);
    let ball = sphere([100.0, 0.0, 0.0], 5.0);
    let walls = sphere([205.0, 205.0, 205.0], 10.0);
    let hipparcos = json!({"sources": ["hipparcos-2007"]});
    let queries = [(&ball, json!({})), (&ball, hipparcos), (&walls, json!({}))];
    let before: Vec<_> = queries.iter().map(|(v, f)| volume(&db, 2, v, f)).collect();
    publish_release_3(&db, &volume_of_release_3());
    for ((v, f), (response, hits)) in queries.iter().zip(before) {
        let (after, after_hits) = volume(&db, 2, v, f);
        assert_eq!(response.body, after.body, "{v} {f}");
        assert_eq!(hits, after_hits);
    }
    assert_eq!(
        volume(&db, 2, &ball, &json!({})).1,
        ["object:1001", "object:1002"]
    );
}

#[test]
fn a_later_release_queries_the_objects_and_structures_it_keeps_and_adds_and_not_those_it_retires() {
    let db = volume_db(&[
        galaxy(1001, [100.0, 0.0, 0.0]),
        galaxy(1002, [104.99, 0.0, 0.0]),
    ]);
    publish_release_3(&db, &volume_of_release_3());
    let ball = sphere([100.0, 0.0, 0.0], 5.0);
    assert_eq!(
        volume(&db, 3, &ball, &json!({})).1,
        ["object:1002", "object:1006"],
        "1001 moved out, 1006 added, 1002 shared"
    );
    let moved = volume(&db, 3, &sphere([150.0, 0.0, 0.0], 1.0), &json!({})).1;
    assert_eq!(moved, ["object:1001"]);
    let hipparcos = json!({"sources": ["hipparcos-2007"]});
    assert_eq!(
        volume(&db, 3, &ball, &hipparcos).1,
        Vec::<String>::new(),
        "1002's entry retired"
    );
    let walls = sphere([205.0, 205.0, 205.0], 10.0);
    assert_eq!(volume(&db, 2, &walls, &json!({})).1, ["structure:box-only"]);
    assert_eq!(volume(&db, 3, &walls, &json!({})).1, Vec::<String>::new());
}

/// Release 3 of the structures fixture: `bootes-void` renamed, `giant-grb-ring` retired and
/// `new-wall` added; `local-void` and every structure's details kept.
fn structures_of_release_3() -> String {
    format!(
        "{}
         UPDATE structures SET retired = 3 WHERE id = 'giant-grb-ring';
         UPDATE structure_details SET retired = 3 WHERE id = 'giant-grb-ring';
         INSERT INTO structures (release, id, name, kind, status, scale_domain, record)
           VALUES (3, 'new-wall', 'New Wall', 'wall', 'observed', 'cosmic_web', '{{}}');",
        replace("structures", "id = 'bootes-void'", "name = 'Boötes Void'"),
    )
}

fn structures_db() -> Sqlite {
    let db = Sqlite::new();
    db.exec(RELEASES).exec(SOURCES).exec(STRUCTURES);
    db
}

#[test]
fn structures_read_pinned_to_a_release_are_unchanged_by_a_later_release_that_shares_their_rows() {
    let db = structures_db();
    let paths = [
        "/v1/structures",
        "/v1/structures/bootes-void",
        "/v1/structures/giant-grb-ring",
        "/v1/structures/local-void",
    ];
    let before: Vec<Response> = paths.iter().map(|path| read(&db, path, 2)).collect();
    publish_release_3(&db, &structures_of_release_3());
    for (path, before) in paths.iter().zip(before) {
        let after = read(&db, path, 2);
        assert_eq!(before.status, 200, "{path}");
        assert_eq!(before.body, after.body, "{path}");
    }
}

#[test]
fn a_later_release_lists_the_structures_it_keeps_and_adds_and_not_those_it_retires() {
    let db = structures_db();
    let bootes = json(&db, "/v1/structures/bootes-void", 2);
    publish_release_3(&db, &structures_of_release_3());
    let listing = json(&db, "/v1/structures", 3);
    let ids: Vec<&Value> = listing["structures"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| &s["id"])
        .collect();
    assert_eq!(ids, ["bootes-void", "local-void", "new-wall"]);
    let renamed = json(&db, "/v1/structures/bootes-void", 3);
    assert_eq!(bootes["name"], "Bootes Void");
    assert_eq!(renamed["name"], "Boötes Void");
    assert_eq!(renamed["shape"], bootes["shape"], "its details shared");
    assert_eq!(
        read(&db, "/v1/structures/giant-grb-ring", 3).status,
        404,
        "retired by release 3"
    );
}

/// Release 3 of the sources fixture: hipparcos-2007 retired with its details, and the
/// researched structures retitled, their details kept.
fn sources_of_release_3() -> String {
    format!(
        "UPDATE sources SET retired = 3 WHERE id = 'hipparcos-2007';
         UPDATE source_details SET retired = 3 WHERE source = 0;
         UPDATE payloads SET retired = 3 WHERE source = 0;
         UPDATE normalizations SET retired = 3 WHERE source = 0;
         {}",
        replace(
            "sources",
            "id = 'researched-structures'",
            "title = 'Researched structures, revised'"
        ),
    )
}

#[test]
fn sources_read_pinned_to_a_release_are_unchanged_by_a_later_release_that_shares_their_rows() {
    let db = Sqlite::new();
    db.exec(RELEASES).exec(SOURCES);
    let paths = [
        "/v1/sources",
        "/v1/sources/hipparcos-2007",
        "/v1/sources/hipparcos-2007/provenance",
        "/v1/sources/researched-structures",
    ];
    let before: Vec<Response> = paths.iter().map(|path| read(&db, path, 2)).collect();
    publish_release_3(&db, &sources_of_release_3());
    for (path, before) in paths.iter().zip(before) {
        let after = read(&db, path, 2);
        assert_eq!(before.status, 200, "{path}");
        assert_eq!(before.body, after.body, "{path}");
    }
}

#[test]
fn a_later_release_lists_the_sources_it_keeps_and_replaces_and_not_those_it_retires() {
    let db = Sqlite::new();
    db.exec(RELEASES).exec(SOURCES);
    let researched = json(&db, "/v1/sources/researched-structures", 2);
    publish_release_3(&db, &sources_of_release_3());
    let listing = json(&db, "/v1/sources", 3);
    let titles: Vec<&Value> = listing["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| &s["title"])
        .collect();
    assert_eq!(titles, ["Researched structures, revised"]);
    let retitled = json(&db, "/v1/sources/researched-structures", 3);
    assert_eq!(retitled["title"], "Researched structures, revised");
    assert_eq!(
        retitled["reference"], researched["reference"],
        "its details shared"
    );
    for path in [
        "/v1/sources/hipparcos-2007",
        "/v1/sources/hipparcos-2007/provenance",
    ] {
        assert_eq!(read(&db, path, 3).status, 404, "{path}");
    }
}
