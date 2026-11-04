//! Layers: created, configured, retired, listed and read, and their changes followed
//! (create-and-configure-a-layer, read-a-layer, follow-a-layers-changes).

use serde_json::{Value, json};

use crate::http::{Body, Method, Request};
use crate::layers_fixture::{count, db, layer, publish, sn1987a};
use crate::testkit::body;

fn get(path: &str) -> Request {
    Request::new(Method::Get, path)
}

fn text(value: &Value, field: &str) -> String {
    value[field].as_str().unwrap().to_owned()
}

#[test]
fn a_layer_has_an_id_title_description_visibility_owner_and_time_and_starts_at_sequence_0() {
    let db = db();
    let key = db.key();
    let created = layer(
        &db,
        &key,
        &json!({"title": "Supernovae", "description": "Nearby ones"}),
    );
    for field in [
        "id",
        "title",
        "description",
        "visibility",
        "owner",
        "created_at",
    ] {
        assert!(created[field].is_string(), "{field}: {created}");
    }
    assert_eq!(
        created["visibility"], "private",
        "private unless created public"
    );
    assert_eq!(created["sequence"], 0);
    let public = layer(
        &db,
        &key,
        &json!({"title": "Shared", "visibility": "public"}),
    );
    assert_eq!(public["visibility"], "public");
}

#[test]
fn a_write_without_a_key_answers_401_and_stores_nothing() {
    let db = db();
    let response =
        db.answer(&Request::new(Method::Post, "/v1/layers").with_json(&json!({"title": "x"})));
    assert_eq!(response.status, 401);
    assert_eq!(count(&db, "layers"), 0);
}

#[test]
fn a_change_advances_the_sequence_and_a_layer_made_private_leaves_anonymous_reads() {
    let db = db();
    let key = db.key();
    let id = text(
        &layer(&db, &key, &json!({"title": "A", "visibility": "public"})),
        "id",
    );
    let path = format!("/v1/layers/{id}");
    assert_eq!(db.answer(&get(&path)).status, 200);
    let (response, changed) = db.with_key(
        &key,
        Request::new(Method::Patch, &path).with_json(&json!({"visibility": "private"})),
    );
    assert_eq!(response.status, 200, "{changed}");
    assert_eq!(changed["sequence"], 1);
    assert_eq!(changed["title"], "A", "what is not asked stays");
    assert_eq!(db.answer(&get(&path)).status, 404);
    let (_, listing) = db.get_json("/v1/layers", "");
    assert_eq!(listing["layers"], json!([]));
    let (_, mine) = db.with_key(&key, get("/v1/layers"));
    assert_eq!(mine["layers"][0]["id"], id.as_str());
}

#[test]
fn a_private_layer_answers_another_caller_exactly_as_one_that_does_not_exist() {
    let db = db();
    let (owner, other) = (db.key(), db.key());
    let id = text(&layer(&db, &owner, &json!({"title": "Mine"})), "id");
    let reads = [
        format!("/v1/layers/{id}"),
        format!("/v1/layers/{id}/changes"),
        format!("/v1/layers/{id}/items/any"),
    ];
    for path in &reads {
        let unknown = path.replace(&id, "lay_none");
        let hidden = db.answer(&get(path));
        let absent = db.answer(&get(&unknown));
        assert_eq!(hidden.status, 404, "{path}");
        let (keyed, _) = db.with_key(&other, get(path));
        assert_eq!(keyed.status, 404, "{path}");
        let mut expected = body(&absent);
        expected["id"] = json!(id);
        expected["detail"] = json!(format!("No layer has the id {id:?}."));
        assert_eq!(body(&hidden), expected, "{path}");
    }
    let (changed, _) = db.with_key(
        &other,
        Request::new(Method::Patch, &format!("/v1/layers/{id}")).with_json(&json!({"title": "x"})),
    );
    assert_eq!(changed.status, 404);
}

#[test]
fn another_keys_change_of_a_public_layer_answers_403_and_changes_nothing() {
    let db = db();
    let (owner, other) = (db.key(), db.key());
    let id = text(
        &layer(
            &db,
            &owner,
            &json!({"title": "Ours", "visibility": "public"}),
        ),
        "id",
    );
    let path = format!("/v1/layers/{id}");
    for request in [
        Request::new(Method::Patch, &path).with_json(&json!({"title": "Theirs"})),
        Request::new(Method::Delete, &path),
        Request::new(Method::Post, &format!("{path}/items")).with_json(&sn1987a("x")),
    ] {
        let (response, problem) = db.with_key(&other, request);
        assert_eq!(response.status, 403, "{problem}");
        assert_eq!(problem["type"], "urn:univ:problem:not-layer-owner");
    }
    let (_, read) = db.get_json(&path, "");
    assert_eq!(read["layer"]["title"], "Ours");
    assert_eq!(read["layer"]["sequence"], 0);
}

#[test]
fn a_retired_layer_answers_410_to_its_readers_404_to_others_and_leaves_its_keys_quota() {
    let db = db();
    let key = db.key();
    let public = text(
        &layer(&db, &key, &json!({"title": "P", "visibility": "public"})),
        "id",
    );
    let private = text(&layer(&db, &key, &json!({"title": "Q"})), "id");
    db.now.set(db.now.get() + 1_000);
    for id in [&public, &private] {
        let (response, _) = db.with_key(
            &key,
            Request::new(Method::Delete, &format!("/v1/layers/{id}")),
        );
        assert_eq!(response.status, 204);
    }
    let (gone, problem) = db.get_json(&format!("/v1/layers/{public}"), "");
    assert_eq!(gone.status, 410);
    assert_eq!(problem["retired_at"], "2026-11-03T12:00:01.000Z");
    let (owned, _) = db.with_key(&key, get(&format!("/v1/layers/{private}")));
    assert_eq!(owned.status, 410);
    assert_eq!(
        db.answer(&get(&format!("/v1/layers/{private}"))).status,
        404
    );
    let (_, listing) = db.with_key(&key, get("/v1/layers"));
    assert_eq!(
        listing["layers"],
        json!([]),
        "a retired layer is not listed"
    );
    let (_, keys) = db.as_owner(get("/v1/owner/keys"));
    assert_eq!(keys["keys"][0]["usage"]["layers"], 0);
}

#[test]
fn the_listing_gives_each_layers_id_title_visibility_item_count_and_sequence() {
    let db = db();
    let key = db.key();
    let id = text(
        &layer(&db, &key, &json!({"title": "Sky", "visibility": "public"})),
        "id",
    );
    publish(&db, &key, &id, &sn1987a("sn1987a"));
    let (_, listing) = db.get_json("/v1/layers", "");
    let listed = &listing["layers"][0];
    assert_eq!(listed["id"], id.as_str());
    assert_eq!(listed["title"], "Sky");
    assert_eq!(listed["visibility"], "public");
    assert_eq!(listed["item_count"], 1);
    assert_eq!(listed["sequence"], 1);
}

#[test]
fn a_layers_items_come_in_pages_of_500_with_a_cursor_each_with_its_version_and_time() {
    let db = db();
    let key = db.key();
    let id = text(
        &layer(&db, &key, &json!({"title": "Many", "visibility": "public"})),
        "id",
    );
    db.layers
        .execute_batch(&format!(
            "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 501)
             INSERT INTO items (layer_id, id, version) SELECT '{id}', printf('i%04d', i), 1 FROM n;
             INSERT INTO item_versions SELECT layer_id, id, 1, '2026-11-03T12:00:00.000Z',
               'published', '{{\"content\":{{}}}}' FROM items;"
        ))
        .unwrap();
    let (_, first) = db.get_json(&format!("/v1/layers/{id}"), "");
    let items = first["items"].as_array().unwrap();
    assert_eq!(items.len(), 500);
    assert_eq!(items[0]["version"], 1);
    assert_eq!(items[0]["stored_at"], "2026-11-03T12:00:00.000Z");
    assert_eq!(first["next"], "i0500");
    let (_, second) = db.get_json(&format!("/v1/layers/{id}"), "cursor=i0500");
    assert_eq!(second["items"].as_array().unwrap().len(), 1);
    assert_eq!(second["items"][0]["item"], "i0501");
    assert_eq!(second["next"], Value::Null);
}

#[test]
fn the_changes_since_a_cursor_come_in_order_with_their_time_kind_item_and_version() {
    let db = db();
    let key = db.key();
    let id = text(
        &layer(&db, &key, &json!({"title": "Feed", "visibility": "public"})),
        "id",
    );
    let path = format!("/v1/layers/{id}");
    publish(&db, &key, &id, &sn1987a("a"));
    db.with_key(
        &key,
        Request::new(Method::Patch, &path).with_json(&json!({"title": "Fed"})),
    );
    db.with_key(&key, Request::new(Method::Delete, &path));
    let (response, page) = db.get_json(&format!("{path}/changes"), "since=0");
    assert_eq!(response.status, 200, "{page}");
    let changes = page["changes"].as_array().unwrap();
    let kinds: Vec<&Value> = changes.iter().map(|c| &c["kind"]).collect();
    assert_eq!(
        kinds,
        ["item_published", "layer_configured", "layer_retired"]
    );
    assert_eq!(changes[0]["sequence"], 1);
    assert_eq!(changes[0]["item"], "a");
    assert_eq!(changes[0]["version"], 1);
    assert!(changes[0]["at"].is_string());
    assert_eq!(changes[1]["item"], Value::Null);
    assert_eq!(page["cursor"], 3);
    let (_, rest) = db.get_json(&format!("{path}/changes"), "since=1");
    assert_eq!(rest["changes"].as_array().unwrap().len(), 2);
    let (_, none) = db.get_json(&format!("{path}/changes"), "since=3");
    assert_eq!(
        (none["changes"].clone(), none["cursor"].clone()),
        (json!([]), json!(3))
    );
}

#[test]
fn a_page_of_changes_holds_at_most_1000() {
    let db = db();
    let key = db.key();
    let id = text(
        &layer(&db, &key, &json!({"title": "Busy", "visibility": "public"})),
        "id",
    );
    db.layers
        .execute_batch(&format!(
            "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 1001)
             INSERT INTO changes (layer_id, sequence, at, kind) SELECT '{id}', i, 't', 'layer_configured' FROM n;
             UPDATE layers SET sequence = 1001;"
        ))
        .unwrap();
    let (_, page) = db.get_json(&format!("/v1/layers/{id}/changes"), "");
    assert_eq!(page["changes"].as_array().unwrap().len(), 1_000);
    assert_eq!(page["cursor"], 1_000);
}

#[test]
fn a_cursor_malformed_or_beyond_the_sequence_answers_400_with_the_sequence() {
    let db = db();
    let key = db.key();
    let id = text(
        &layer(&db, &key, &json!({"title": "F", "visibility": "public"})),
        "id",
    );
    publish(&db, &key, &id, &sn1987a("a"));
    for since in ["2", "x", "-1"] {
        let (response, problem) = db.get_json(
            &format!("/v1/layers/{id}/changes"),
            &format!("since={since}"),
        );
        assert_eq!(response.status, 400, "{since}");
        assert_eq!(problem["sequence"], 1);
        assert_eq!(problem["parameter"], "since");
    }
}

#[test]
fn a_read_repeated_with_its_entity_tag_answers_304_until_the_layer_changes() {
    let db = db();
    let key = db.key();
    let id = text(
        &layer(&db, &key, &json!({"title": "T", "visibility": "public"})),
        "id",
    );
    publish(&db, &key, &id, &sn1987a("a"));
    let paths = [
        format!("/v1/layers/{id}"),
        format!("/v1/layers/{id}/items/a"),
        format!("/v1/layers/{id}/changes"),
    ];
    for path in &paths {
        let first = db.answer(&get(path));
        let etag = first.header("etag").unwrap().to_owned();
        let again = db.answer(&get(path).with_header("if-none-match", &etag));
        assert_eq!(again.status, 304, "{path}");
        assert_eq!(again.body, Body::Empty);
        assert_eq!(again.header("etag"), Some(etag.as_str()));
    }
    let before = db
        .answer(&get(&paths[1]))
        .header("etag")
        .unwrap()
        .to_owned();
    publish(&db, &key, &id, &sn1987a("b"));
    for path in &paths {
        let after = db.answer(&get(path).with_header("if-none-match", &before));
        assert_eq!(after.status, 200, "{path}: every change gives a new tag");
    }
}

#[test]
fn a_revoked_keys_public_layers_stay_readable_and_its_private_ones_unreadable() {
    let db = db();
    let issued = db.issue(&json!({"label": "gone"}));
    let key = text(&issued, "secret");
    let public = text(
        &layer(&db, &key, &json!({"title": "P", "visibility": "public"})),
        "id",
    );
    let private = text(&layer(&db, &key, &json!({"title": "Q"})), "id");
    db.as_owner(Request::new(
        Method::Post,
        &format!("/v1/owner/keys/{}/revoke", text(&issued, "id")),
    ));
    assert_eq!(db.answer(&get(&format!("/v1/layers/{public}"))).status, 200);
    assert_eq!(
        db.answer(&get(&format!("/v1/layers/{private}"))).status,
        404
    );
    let (refused, _) = db.with_key(&key, get(&format!("/v1/layers/{private}")));
    assert_eq!(refused.status, 401, "the revoked key itself is refused");
}
