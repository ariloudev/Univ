//! End-to-end tests of layers and their items through the Worker under `wrangler dev`: a
//! layer created, an item published, revised and retracted, the changes followed, an entity
//! tag answering 304, a private layer looking absent and a quota refused. Run them with
//! `cargo xtask integration`.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;

use common::{CLIENT, get, header, issue, json, owner, url, with_json};
use reqwest::StatusCode;
use reqwest::blocking::Response;
use serde_json::{Value, json};

/// A key's request with `body`.
fn send(key: &str, method: reqwest::Method, path: &str, body: Option<&Value>) -> Response {
    let request = CLIENT.request(method, url(path)).bearer_auth(key);
    match body {
        Some(body) => with_json(request, body),
        None => request,
    }
    .send()
    .unwrap()
}

/// A TEST FIXTURE item, SN 1987A with values rounded from the literature, hosted by the
/// fixture release's object 11 and located in its structure `structure`.
fn item(id: &str, structure: &str) -> Value {
    json!({
        "id": id,
        "title": "SN 1987A",
        "summary": "A core-collapse supernova in the Large Magellanic Cloud.",
        "geometry": {"frame": "icrs", "unit": "kiloparsec",
                     "geometry": {"kind": "point", "at": [1.977, 18.10, -48.07]}},
        "time_records": [{"kind": "arrival", "timescale": "utc",
                          "instant": "1987-02-23T07:35:35Z",
                          "uncertainty": {"form": "symmetric", "sigma": 60, "unit": "s"}}],
        "kind_of_value": "measured",
        "links": [{"relation": "host", "object": 11},
                  {"relation": "located_in", "structure": structure}],
        "sources": [{"title": "Supernova 1987A", "bibcode": "1989ARA&A..27..629A"}]
    })
}

fn a_structure() -> String {
    json(get("/v1/structures"))["structures"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// A new public layer of a new extended key, and the key's secret.
fn public_layer() -> (String, String) {
    let (_, key) = issue("e2e layers", "extended");
    let created = send(
        &key,
        reqwest::Method::POST,
        "/v1/layers",
        Some(&json!({"title": "E2E", "visibility": "public"})),
    );
    assert_eq!(created.status(), StatusCode::CREATED);
    (json(created)["id"].as_str().unwrap().to_owned(), key)
}

#[test]
#[ignore = "needs the Worker: run `cargo xtask integration`"]
fn an_item_is_published_revised_retracted_and_its_changes_followed() {
    let (layer, key) = public_layer();
    let structure = a_structure();
    let base = format!("/v1/layers/{layer}");
    let published = send(
        &key,
        reqwest::Method::POST,
        &format!("{base}/items"),
        Some(&item("sn", &structure)),
    );
    assert_eq!(published.status(), StatusCode::CREATED);
    assert_eq!(json(published)["version"], 1);
    let mut content = item("sn", &structure);
    content.as_object_mut().unwrap().remove("id");
    let revision = json!({"replaces": 1, "revision": "update", "content": content});
    let revised = send(
        &key,
        reqwest::Method::POST,
        &format!("{base}/items/sn/revisions"),
        Some(&revision),
    );
    assert_eq!(revised.status(), StatusCode::CREATED);
    let stale = send(
        &key,
        reqwest::Method::POST,
        &format!("{base}/items/sn/revisions"),
        Some(&revision),
    );
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    assert_eq!(json(stale)["current_version"], 2);
    let retraction = json!({"replaces": 2, "reason": "A test, withdrawn."});
    let retracted = send(
        &key,
        reqwest::Method::POST,
        &format!("{base}/items/sn/retraction"),
        Some(&retraction),
    );
    assert_eq!(retracted.status(), StatusCode::CREATED);
    let read = json(get(&format!("{base}/items/sn")));
    assert_eq!(read["retracted"], true);
    assert_eq!(read["versions"].as_array().unwrap().len(), 3);
    assert_eq!(
        json(get(&format!("{base}/items/sn/versions/1")))["kind"],
        "published"
    );
    let changes = json(get(&format!("{base}/changes?since=0")));
    let kinds: Vec<&Value> = changes["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| &c["kind"])
        .collect();
    assert_eq!(kinds, ["item_published", "item_revised", "item_retracted"]);
    assert_eq!(changes["cursor"], 3);
}

#[test]
#[ignore = "needs the Worker: run `cargo xtask integration`"]
fn a_layer_read_with_its_entity_tag_answers_304_until_it_changes() {
    let (layer, key) = public_layer();
    let path = format!("/v1/layers/{layer}");
    let first = get(&path);
    let etag = header(&first, "etag").unwrap().to_owned();
    let again = CLIENT
        .get(url(&path))
        .header("if-none-match", &etag)
        .send()
        .unwrap();
    assert_eq!(again.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(again.bytes().unwrap().len(), 0);
    send(
        &key,
        reqwest::Method::PATCH,
        &path,
        Some(&json!({"title": "Changed"})),
    );
    let after = CLIENT
        .get(url(&path))
        .header("if-none-match", &etag)
        .send()
        .unwrap();
    assert_eq!(after.status(), StatusCode::OK);
}

#[test]
#[ignore = "needs the Worker: run `cargo xtask integration`"]
fn a_private_layer_looks_absent_and_a_write_without_a_key_is_refused() {
    let (_, key) = issue("e2e private", "extended");
    let created = send(
        &key,
        reqwest::Method::POST,
        "/v1/layers",
        Some(&json!({"title": "Mine"})),
    );
    let layer = json(created)["id"].as_str().unwrap().to_owned();
    let absent_id = "lay_0000000000000000";
    let hidden = get(&format!("/v1/layers/{layer}"));
    let absent = get(&format!("/v1/layers/{absent_id}"));
    assert_eq!(hidden.status(), StatusCode::NOT_FOUND);
    let as_absent = json(absent).to_string().replace(absent_id, &layer);
    assert_eq!(
        json(hidden).to_string(),
        as_absent,
        "the same body, but for the id asked"
    );
    assert_eq!(
        send(
            &key,
            reqwest::Method::GET,
            &format!("/v1/layers/{layer}"),
            None
        )
        .status(),
        StatusCode::OK
    );
    let anonymous = with_json(CLIENT.post(url("/v1/layers")), &json!({"title": "x"}))
        .send()
        .unwrap();
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
}

#[test]
#[ignore = "needs the Worker: run `cargo xtask integration`"]
fn a_write_beyond_a_quota_is_refused_and_stores_nothing() {
    let (key_id, key) = issue("e2e quota", "extended");
    let lowered = with_json(
        owner(CLIENT.patch(url(&format!("/v1/owner/keys/{key_id}")))),
        &json!({"quotas": {"layers_per_key": 1}}),
    )
    .send()
    .unwrap();
    assert_eq!(lowered.status(), StatusCode::OK);
    let first = send(
        &key,
        reqwest::Method::POST,
        "/v1/layers",
        Some(&json!({"title": "One"})),
    );
    assert_eq!(first.status(), StatusCode::CREATED);
    let second = send(
        &key,
        reqwest::Method::POST,
        "/v1/layers",
        Some(&json!({"title": "Two"})),
    );
    assert_eq!(second.status(), StatusCode::FORBIDDEN);
    let problem = json(second);
    assert_eq!(problem["quota"], "layers_per_key");
    assert_eq!(
        (problem["limit"].clone(), problem["usage"].clone()),
        (json!(1), json!(1))
    );
    let mine = json(send(&key, reqwest::Method::GET, "/v1/layers", None));
    let owned = mine["layers"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|l| l["owner"] == key_id.as_str())
        .count();
    assert_eq!(owned, 1);
}
