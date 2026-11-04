//! Items: published, revised and retracted, checked by JSON pointer, and held to the owning
//! key's quotas (publish-a-layer-item, revise-or-retract-an-item, read-a-layer,
//! enforce-quotas-and-rate-limits).

use serde_json::{Value, json};

use crate::http::{Method, Request};
use crate::layers_fixture::{content, count, db, layer, publish, sn1987a};
use crate::testkit::Sqlite;

struct Layer {
    db: Sqlite,
    key: String,
    id: String,
}

impl Layer {
    fn new() -> Self {
        let db = db();
        let key = db.key();
        let id = layer(
            &db,
            &key,
            &json!({"title": "Events", "visibility": "public"}),
        )["id"]
            .as_str()
            .unwrap()
            .to_owned();
        Self { db, key, id }
    }

    fn publish(&self, item: &Value) -> (crate::Response, Value) {
        publish(&self.db, &self.key, &self.id, item)
    }

    fn post(&self, path: &str, json: &Value) -> (crate::Response, Value) {
        let path = format!("/v1/layers/{}/items/{path}", self.id);
        self.db
            .with_key(&self.key, Request::new(Method::Post, &path).with_json(json))
    }

    fn get(&self, path: &str) -> Value {
        let path = format!("/v1/layers/{}{path}", self.id);
        self.db.get_json(&path, "").1
    }

    fn sequence(&self) -> Value {
        self.get("")["layer"]["sequence"].clone()
    }

    /// The JSON pointers a refusal names.
    fn pointers(problem: &Value) -> Vec<String> {
        problem["errors"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["pointer"].as_str().unwrap().to_owned())
            .collect()
    }
}

#[test]
fn a_published_item_is_its_first_version_with_its_content_and_the_time_it_was_stored() {
    let layer = Layer::new();
    let (response, version) = layer.publish(&sn1987a("sn1987a"));
    assert_eq!(response.status, 201, "{version}");
    assert_eq!(version["version"], 1);
    assert_eq!(version["kind"], "published");
    assert_eq!(version["stored_at"], "2026-11-03T12:00:00.000Z");
    let content = &version["content"];
    for field in [
        "title",
        "summary",
        "geometry",
        "time_records",
        "kind_of_value",
        "links",
        "sources",
    ] {
        assert!(!content[field].is_null(), "{field}");
    }
    assert_eq!(content["geometry"]["frame"], "icrs");
    assert_eq!(content["time_records"][0]["timescale"], "utc");
    assert_eq!(content["time_records"][0]["uncertainty"]["unit"], "s");
    assert_eq!(
        content["links"][0],
        json!({"relation": "host", "object": 10})
    );
    assert_eq!(
        layer.sequence(),
        1,
        "the publish advances the sequence by one"
    );
}

#[test]
fn each_time_record_is_stored_on_tt_with_its_uncertainty_interval() {
    let layer = Layer::new();
    layer.publish(&sn1987a("sn1987a"));
    let rows: Vec<(String, f64, f64, f64)> = layer
        .db
        .layers
        .prepare("SELECT kind, at, earliest, latest FROM item_times ORDER BY record")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(rows.len(), 2);
    let (kind, at, earliest, latest) = &rows[0];
    assert_eq!(kind, "arrival");
    // 1987-02-23T07:35:35 UTC is MJD 46849.316377; TT - UTC was 23 + 32.184 s.
    let expected = 46_849.0 + (7.0 * 3_600.0 + 35.0 * 60.0 + 35.0 + 55.184) / 86_400.0 - 51_544.5;
    assert!((at - expected).abs() < 1e-9, "{at} {expected}");
    assert!(
        (latest - at - 60.0 / 86_400.0).abs() < 1e-9
            && (at - earliest - 60.0 / 86_400.0).abs() < 1e-9
    );
    assert_eq!(rows[1].1, -168_000.0 * 365.25);
}

#[test]
fn an_item_failing_its_checks_is_refused_naming_each_field_and_nothing_is_stored() {
    let layer = Layer::new();
    let mut item = sn1987a("bad id!");
    item["title"] = json!("");
    item["geometry"] = json!({"frame": "nowhere", "unit": "kiloparsec", "geometry": {"kind": "point", "at": [0, 0, 0]}});
    item["time_records"][0]["instant"] = json!("1950-01-01T00:00:00Z");
    item["time_records"][1]["uncertainty"] = json!({"form": "symmetric", "sigma": -1, "unit": "a"});
    item["kind_of_value"] = json!("guessed");
    item["links"] = json!([{"relation": "host", "object": 999}, {"relation": "host"}]);
    item["colour"] = json!("red");
    let (response, problem) = layer.publish(&item);
    assert_eq!(response.status, 422, "{problem}");
    assert_eq!(problem["type"], "urn:univ:problem:item-invalid");
    let mut pointers = Layer::pointers(&problem);
    pointers.sort();
    assert_eq!(
        pointers,
        [
            "/colour",
            "/geometry/frame",
            "/id",
            "/kind_of_value",
            "/links/0/object",
            "/links/1",
            "/time_records/0/instant",
            "/time_records/1/uncertainty",
            "/title",
        ]
    );
    assert!(
        problem["errors"]
            .to_string()
            .contains("give the time in TT")
    );
    assert!(
        problem["errors"]
            .to_string()
            .contains("no release has published the object 999")
    );
    assert_eq!(
        (count(&layer.db, "items"), count(&layer.db, "item_versions")),
        (0, 0)
    );
    assert_eq!(layer.sequence(), 0);
}

#[test]
fn an_item_citing_no_source_answers_422_saying_a_source_is_required() {
    let layer = Layer::new();
    for sources in [json!([]), Value::Null] {
        let mut item = sn1987a("x");
        if sources.is_null() {
            item.as_object_mut().unwrap().remove("sources");
        } else {
            item["sources"] = sources;
        }
        let (response, problem) = layer.publish(&item);
        assert_eq!(response.status, 422);
        assert_eq!(problem["type"], "urn:univ:problem:source-required");
        assert_eq!(Layer::pointers(&problem), ["/sources"]);
    }
    let mut unidentified = sn1987a("x");
    unidentified["sources"] = json!([{"title": "A talk"}]);
    let (response, problem) = layer.publish(&unidentified);
    assert_eq!(response.status, 422);
    assert_eq!(Layer::pointers(&problem), ["/sources/0"]);
    assert_eq!(count(&layer.db, "items"), 0);
}

#[test]
fn a_link_names_its_relation_and_a_published_object_or_structure() {
    let layer = Layer::new();
    let mut item = sn1987a("linked");
    item["links"] = json!([
        {"relation": "host", "object": 10},
        {"relation": "located_in", "structure": "local-void"},
        {"relation": "member_of", "structure": "nowhere"},
        {"relation": "counterpart", "object": 10, "structure": "local-void"}
    ]);
    let (response, problem) = layer.publish(&item);
    assert_eq!(response.status, 422);
    let mut pointers = Layer::pointers(&problem);
    pointers.sort();
    assert_eq!(pointers, ["/links/2/structure", "/links/3"]);
    item["links"].as_array_mut().unwrap().truncate(2);
    assert_eq!(layer.publish(&item).0.status, 201);
}

#[test]
fn a_link_to_an_id_a_later_release_merged_is_served_with_the_id_that_replaced_it() {
    let layer = Layer::new();
    let mut item = sn1987a("merged");
    item["links"] = json!([{"relation": "host", "object": 5}, {"relation": "host", "object": 10}]);
    assert_eq!(layer.publish(&item).0.status, 201);
    let read = layer.get("/items/merged");
    let links = &read["current"]["content"]["links"];
    assert_eq!(links[0]["object"], 5, "the id as written");
    assert_eq!(links[0]["replaced_by"], json!([10]));
    assert_eq!(links[0]["replaced_in"], 2);
    assert_eq!(links[1].get("replaced_by"), None);
    let page = layer.get("");
    assert_eq!(
        page["items"][0]["content"]["links"][0]["replaced_by"],
        json!([10])
    );
}

#[test]
fn an_id_the_layer_holds_answers_409_with_the_items_current_version() {
    let layer = Layer::new();
    layer.publish(&sn1987a("sn1987a"));
    let (response, problem) = layer.publish(&sn1987a("sn1987a"));
    assert_eq!(response.status, 409);
    assert_eq!(problem["current_version"], 1);
    assert_eq!(layer.sequence(), 1);
}

#[test]
fn a_revision_is_the_next_version_and_earlier_versions_stay_readable_and_unchanged() {
    let layer = Layer::new();
    let (_, first) = layer.publish(&sn1987a("sn"));
    let mut revised = content(&sn1987a("sn"));
    revised["summary"] = json!("A type II supernova in the Large Magellanic Cloud.");
    let (response, update) = layer.post(
        "sn/revisions",
        &json!({"replaces": 1, "revision": "update", "content": revised}),
    );
    assert_eq!(response.status, 201, "{update}");
    assert_eq!(update["version"], 2);
    assert_eq!(update["kind"], "update");
    let (_, correction) = layer.post(
        "sn/revisions",
        &json!({"replaces": 2, "revision": "correction", "corrects": 1,
                "what_was_wrong": "The type was missing.", "content": revised}),
    );
    assert_eq!(correction["version"], 3);
    assert_eq!(correction["corrects"], 1);
    assert_eq!(correction["what_was_wrong"], "The type was missing.");
    assert_eq!(layer.get("/items/sn/versions/1"), first);
    let item = layer.get("/items/sn");
    assert_eq!(item["current"]["version"], 3);
    let numbers: Vec<&Value> = item["versions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| &v["version"])
        .collect();
    assert_eq!(numbers, [1, 2, 3]);
    assert_eq!(layer.sequence(), 3);
}

#[test]
fn a_revision_is_checked_as_a_publish_is_and_a_failing_one_stores_nothing() {
    let layer = Layer::new();
    layer.publish(&sn1987a("sn"));
    let mut bad = content(&sn1987a("sn"));
    bad["sources"] = json!([]);
    bad["time_records"][0]["timescale"] = json!("tai");
    let (response, problem) = layer.post(
        "sn/revisions",
        &json!({"replaces": 1, "revision": "correction", "content": bad}),
    );
    assert_eq!(response.status, 422);
    assert_eq!(problem["type"], "urn:univ:problem:source-required");
    let mut pointers = Layer::pointers(&problem);
    pointers.sort();
    assert_eq!(
        pointers,
        [
            "/content/sources",
            "/content/time_records/0/timescale",
            "/what_was_wrong"
        ]
    );
    assert_eq!(count(&layer.db, "item_versions"), 1);
    assert_eq!(layer.sequence(), 1);
    assert_eq!(layer.get("/items/sn")["current"]["version"], 1);
}

#[test]
fn a_revision_or_retraction_of_a_version_not_current_answers_409_with_the_current_one() {
    let layer = Layer::new();
    layer.publish(&sn1987a("sn"));
    let revision = json!({"replaces": 1, "revision": "update", "content": content(&sn1987a("sn"))});
    assert_eq!(layer.post("sn/revisions", &revision).0.status, 201);
    let (response, problem) = layer.post("sn/revisions", &revision);
    assert_eq!(response.status, 409);
    assert_eq!(problem["current_version"], 2);
    let (response, problem) = layer.post("sn/retraction", &json!({"replaces": 1, "reason": "x"}));
    assert_eq!(response.status, 409);
    assert_eq!(problem["current_version"], 2);
    assert_eq!(layer.sequence(), 2);
    let (missing, _) = layer.post("none/retraction", &json!({"replaces": 1, "reason": "x"}));
    assert_eq!(missing.status, 404);
}

#[test]
fn a_retracted_item_leaves_the_current_items_and_reads_as_its_retraction_and_earlier_versions() {
    let layer = Layer::new();
    layer.publish(&sn1987a("sn"));
    layer.publish(&sn1987a("other"));
    let (response, retraction) = layer.post(
        "sn/retraction",
        &json!({"replaces": 1, "reason": "Not a supernova."}),
    );
    assert_eq!(response.status, 201, "{retraction}");
    assert_eq!(retraction["kind"], "retraction");
    assert_eq!(retraction["version"], 2);
    let page = layer.get("");
    let items: Vec<&Value> = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| &i["item"])
        .collect();
    assert_eq!(items, ["other"]);
    assert_eq!(page["layer"]["item_count"], 1);
    let item = layer.get("/items/sn");
    assert_eq!(item["retracted"], true);
    assert_eq!(item["current"]["reason"], "Not a supernova.");
    assert_eq!(item["versions"].as_array().unwrap().len(), 2);
    assert_eq!(layer.sequence(), 3);
    let times: u64 = layer
        .db
        .layers
        .query_row(
            "SELECT COUNT(*) FROM item_times WHERE item_id = 'sn'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(times, 0, "a retracted item matches no time query");
    let (again, _) = layer.post("sn/retraction", &json!({"replaces": 2, "reason": "again"}));
    assert_eq!(again.status, 409);
}

#[test]
fn a_write_beyond_a_quota_is_refused_naming_it_its_limit_and_the_usage_and_stores_nothing() {
    let layer = Layer::new();
    let id = |db: &Sqlite| {
        db.layers
            .query_row("SELECT id FROM keys", [], |r| r.get::<_, String>(0))
            .unwrap()
    };
    let key_id = id(&layer.db);
    let set = |quotas: Value| {
        layer.db.as_owner(
            Request::new(Method::Patch, &format!("/v1/owner/keys/{key_id}"))
                .with_json(&json!({"quotas": quotas})),
        )
    };
    layer.publish(&sn1987a("one"));
    layer.publish(&sn1987a("two"));
    set(json!({"items_per_layer": 1}));
    let (response, problem) = layer.publish(&sn1987a("three"));
    assert_eq!(response.status, 403, "{problem}");
    assert_eq!(problem["type"], "urn:univ:problem:quota-exceeded");
    assert_eq!(
        (
            problem["quota"].clone(),
            problem["limit"].clone(),
            problem["usage"].clone()
        ),
        (json!("items_per_layer"), json!(1), json!(2))
    );
    assert_eq!(
        layer.get("")["layer"]["item_count"],
        2,
        "what is stored stays"
    );
    set(json!({"items_per_layer": 100, "bytes_per_item_version": 200}));
    let (response, problem) = layer.publish(&sn1987a("big"));
    assert_eq!(response.status, 403);
    assert_eq!(problem["quota"], "bytes_per_item_version");
    assert!(problem["usage"].as_u64().unwrap() > 200);
    set(json!({"layers_per_key": 1}));
    let (response, problem) = layer.db.with_key(
        &layer.key,
        Request::new(Method::Post, "/v1/layers").with_json(&json!({"title": "x"})),
    );
    assert_eq!(response.status, 403);
    assert_eq!(problem["quota"], "layers_per_key");
    assert_eq!(problem["usage"], 1);
    assert_eq!(
        (count(&layer.db, "layers"), count(&layer.db, "items")),
        (1, 2)
    );
    assert_eq!(layer.sequence(), 2);
}

#[test]
fn a_keys_usage_gives_its_layers_and_its_largest_layers_items() {
    let layer = Layer::new();
    layer.publish(&sn1987a("one"));
    layer.publish(&sn1987a("two"));
    crate::layers_fixture::layer(&layer.db, &layer.key, &json!({"title": "Empty"}));
    let (_, keys) = layer
        .db
        .as_owner(Request::new(Method::Get, "/v1/owner/keys"));
    assert_eq!(
        keys["keys"][0]["usage"],
        json!({"layers": 2, "largest_layer_items": 2})
    );
}
