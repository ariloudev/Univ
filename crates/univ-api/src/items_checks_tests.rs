//! The checks of an item that its parts carry: each failing member of a geometry or time
//! record named on its own, members no type holds refused, links held to published releases,
//! and the quotas and conflicts that every new version meets (publish-a-layer-item,
//! revise-or-retract-an-item, enforce-quotas-and-rate-limits).

use serde_json::{Value, json};

use crate::http::{Method, Request};
use crate::layers_fixture::{count, db, layer, publish, sn1987a};
use crate::testkit::Sqlite;

fn setup() -> (Sqlite, String, String) {
    let db = db();
    let key = db.key();
    let id = layer(
        &db,
        &key,
        &json!({"title": "Checks", "visibility": "public"}),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    (db, key, id)
}

fn pointers(problem: &Value) -> Vec<String> {
    let mut pointers: Vec<String> = problem["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["pointer"].as_str().unwrap().to_owned())
        .collect();
    pointers.sort();
    pointers
}

#[test]
fn each_failing_member_of_a_geometry_or_time_record_is_named_on_its_own() {
    let (db, key, layer) = setup();
    let mut item = sn1987a("x");
    item["geometry"]["frame"] = json!("nowhere");
    item["geometry"]["unit"] = json!("furlong");
    item["time_records"][0]["kind"] = json!("nope");
    item["time_records"][0]["timescale"] = json!("tai");
    item["time_records"][1]["uncertainty"]["form"] = json!("guess");
    let (response, problem) = publish(&db, &key, &layer, &item);
    assert_eq!(response.status, 422, "{problem}");
    assert_eq!(
        pointers(&problem),
        [
            "/geometry/frame",
            "/geometry/unit",
            "/time_records/0/kind",
            "/time_records/0/timescale",
            "/time_records/1/uncertainty/form",
        ]
    );
}

#[test]
fn a_member_no_type_holds_is_refused_not_dropped() {
    let (db, key, layer) = setup();
    let mut item = sn1987a("x");
    item["geometry"]["geometry"]["note"] = json!("near the LMC bar");
    item["time_records"][0]["note"] = json!("Kamiokande-II");
    item["links"][0]["weight"] = json!(1);
    item["sources"][0]["page"] = json!(629);
    let (response, problem) = publish(&db, &key, &layer, &item);
    assert_eq!(response.status, 422, "{problem}");
    assert_eq!(
        pointers(&problem),
        [
            "/geometry/geometry/note",
            "/links/0/weight",
            "/sources/0/page",
            "/time_records/0/note",
        ]
    );
    assert_eq!(count(&db, "items"), 0);
}

#[test]
fn a_time_record_gives_an_instant_or_years_before_j2000_and_not_both() {
    let (db, key, layer) = setup();
    let mut both = sn1987a("x");
    both["time_records"][0]["years_before_j2000"] = json!(13);
    let (response, problem) = publish(&db, &key, &layer, &both);
    assert_eq!(response.status, 422);
    assert_eq!(pointers(&problem), ["/time_records/0/years_before_j2000"]);
    let mut neither = sn1987a("x");
    neither["time_records"][0]
        .as_object_mut()
        .unwrap()
        .remove("instant");
    let (_, problem) = publish(&db, &key, &layer, &neither);
    assert_eq!(pointers(&problem), ["/time_records/0"]);
}

#[test]
fn a_link_to_what_only_an_unpublished_release_holds_is_refused() {
    let (db, key, layer) = setup();
    db.exec(
        "INSERT INTO ids_issued VALUES (3, 61, 70);
         INSERT INTO structures (release, id, name, kind, status, scale_domain, record)
           VALUES (3, 'built-not-published', 'B', 'void', 'observed', 'cosmic_web', '{}');",
    );
    let mut item = sn1987a("x");
    item["links"] = json!([
        {"relation": "host", "object": 65},
        {"relation": "located_in", "structure": "built-not-published"}
    ]);
    let (response, problem) = publish(&db, &key, &layer, &item);
    assert_eq!(response.status, 422, "{problem}");
    assert_eq!(
        pointers(&problem),
        ["/links/0/object", "/links/1/structure"]
    );
}

#[test]
fn a_retraction_is_held_to_the_quota_of_bytes_per_item_version() {
    let (db, key, layer) = setup();
    publish(&db, &key, &layer, &sn1987a("sn"));
    let key_id: String = db
        .layers
        .query_row("SELECT id FROM keys", [], |r| r.get(0))
        .unwrap();
    db.as_owner(
        Request::new(Method::Patch, &format!("/v1/owner/keys/{key_id}"))
            .with_json(&json!({"quotas": {"bytes_per_item_version": 100}})),
    );
    let reason = "x".repeat(1_000);
    let (response, problem) = db.with_key(
        &key,
        Request::new(
            Method::Post,
            &format!("/v1/layers/{layer}/items/sn/retraction"),
        )
        .with_json(&json!({"replaces": 1, "reason": reason})),
    );
    assert_eq!(response.status, 403, "{problem}");
    assert_eq!(problem["quota"], "bytes_per_item_version");
    assert_eq!(count(&db, "item_versions"), 1);
}

#[test]
fn an_id_the_layer_holds_answers_409_even_with_the_layer_at_its_quota() {
    let (db, key, layer) = setup();
    publish(&db, &key, &layer, &sn1987a("sn"));
    let key_id: String = db
        .layers
        .query_row("SELECT id FROM keys", [], |r| r.get(0))
        .unwrap();
    db.as_owner(
        Request::new(Method::Patch, &format!("/v1/owner/keys/{key_id}"))
            .with_json(&json!({"quotas": {"items_per_layer": 1}})),
    );
    let (response, problem) = publish(&db, &key, &layer, &sn1987a("sn"));
    assert_eq!(response.status, 409, "{problem}");
    assert_eq!(problem["current_version"], 1);
}

#[test]
fn a_read_whose_links_a_new_latest_release_resolves_differently_gets_a_new_entity_tag() {
    let (db, key, layer) = setup();
    let mut item = sn1987a("sn");
    item["links"] = json!([{"relation": "host", "object": 50}]);
    assert_eq!(publish(&db, &key, &layer, &item).0.status, 201);
    let path = format!("/v1/layers/{layer}/items/sn");
    let first = db.answer(&Request::new(Method::Get, &path));
    assert_eq!(first.header("univ-release"), Some("2"));
    let etag = first.header("etag").unwrap().to_owned();
    db.exec(
        "UPDATE releases SET published_at = '2026-11-03T09:00:00Z' WHERE number = 3;
         UPDATE latest SET release = 3;
         INSERT INTO objects (release, id, kind, placed) VALUES (3, 10, 1, 1);",
    );
    let after = db.answer(&Request::new(Method::Get, &path).with_header("if-none-match", &etag));
    assert_eq!(
        after.status, 200,
        "release 3 merged object 50: the read changed"
    );
    assert_eq!(after.header("univ-release"), Some("3"));
    let read = crate::testkit::body(&after);
    assert_eq!(
        read["current"]["content"]["links"][0]["replaced_by"],
        json!([10])
    );
    assert_eq!(read["current"]["content"]["links"][0]["replaced_in"], 3);
}
