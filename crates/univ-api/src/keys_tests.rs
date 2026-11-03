//! The owner's key operations and API key authentication (issue-and-revoke-keys,
//! write-with-an-api-key, enforce-quotas-and-rate-limits), on the test kit's univ-layers
//! database.

use serde_json::{Value, json};

use crate::http::{Method, Request, Response};
use crate::testkit::{OWNER_SECRET, Sqlite, body};

fn request(method: Method, path: &str, bearer: Option<&str>, json: Option<Value>) -> Request {
    let mut request = Request::new(method, path);
    if let Some(token) = bearer {
        request
            .headers
            .push(("authorization".into(), format!("Bearer {token}")));
    }
    if let Some(json) = json {
        request.body = json.to_string().into_bytes();
    }
    request
}

fn owner(db: &Sqlite, method: Method, path: &str, json: Option<Value>) -> (Response, Value) {
    let response = db.answer(&request(method, path, Some(OWNER_SECRET), json));
    let document = body(&response);
    (response, document)
}

fn issue(db: &Sqlite, json: Value) -> Value {
    let (response, issued) = owner(db, Method::Post, "/v1/owner/keys", Some(json));
    assert_eq!(response.status, 201, "{issued}");
    issued
}

fn read_with(db: &Sqlite, secret: &str) -> Response {
    db.answer(&request(
        Method::Get,
        "/v1/openapi.json",
        Some(secret),
        None,
    ))
}

fn stored(db: &Sqlite) -> Vec<(String, String)> {
    let mut statement = db
        .layers
        .prepare("SELECT secret_sha256, label FROM keys ORDER BY id")
        .unwrap();
    statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn an_issued_key_holds_256_random_bits_and_only_its_sha256_is_stored() {
    let db = Sqlite::new();
    let issued = issue(&db, json!({"label": "web app"}));
    let secret = issued["secret"].as_str().unwrap();
    let hex = secret.strip_prefix("univ_").unwrap();
    assert_eq!(hex.len(), 64, "32 bytes, 256 bits");
    assert!(hex.bytes().all(|b| b.is_ascii_hexdigit()));
    let rows = stored(&db);
    assert_eq!(
        rows,
        [(crate::access::sha256_hex(secret), "web app".to_owned())]
    );
    let everything: String = db
        .layers
        .query_row(
            "SELECT group_concat(id || label || secret_sha256 || issued_at) FROM keys",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        !everything.contains(hex),
        "the secret itself is stored nowhere"
    );
}

#[test]
fn the_secret_is_returned_when_issued_and_in_no_later_response() {
    let db = Sqlite::new();
    let issued = issue(&db, json!({"label": "web app"}));
    let secret = issued["secret"].as_str().unwrap();
    let id = issued["id"].as_str().unwrap();
    let path = format!("/v1/owner/keys/{id}");
    let revoke = format!("{path}/revoke");
    let later = [
        owner(&db, Method::Get, "/v1/owner/keys", None),
        owner(&db, Method::Patch, &path, Some(json!({"tier": "basic"}))),
        owner(&db, Method::Post, &revoke, None),
    ];
    for (response, document) in later {
        assert_eq!(response.status, 200, "{document}");
        assert!(!document.to_string().contains(secret), "{document}");
        assert!(!document.to_string().contains("secret"), "{document}");
    }
}

#[test]
fn a_key_issued_without_quotas_or_tier_gets_the_default_quotas_and_the_standard_tier() {
    let db = Sqlite::new();
    let issued = issue(&db, json!({"label": "plain"}));
    assert_eq!(
        issued["quotas"],
        json!({"layers_per_key": 100, "items_per_layer": 100_000, "bytes_per_item_version": 65_536})
    );
    assert_eq!(issued["tier"], "standard");
    let chosen = issue(
        &db,
        json!({"label": "big", "quotas": {"layers_per_key": 5}, "tier": "extended"}),
    );
    assert_eq!(chosen["quotas"]["layers_per_key"], 5);
    assert_eq!(chosen["quotas"]["items_per_layer"], 100_000);
    assert_eq!(chosen["tier"], "extended");
}

#[test]
fn a_revocation_is_recorded_with_its_time_and_the_key_is_refused_from_then_on() {
    let db = Sqlite::new();
    let issued = issue(&db, json!({"label": "web app"}));
    let secret = issued["secret"].as_str().unwrap();
    assert_eq!(read_with(&db, secret).status, 200);
    db.now.set(db.now.get() + 90_000);
    let path = format!("/v1/owner/keys/{}/revoke", issued["id"].as_str().unwrap());
    let (response, revoked) = owner(&db, Method::Post, &path, None);
    assert_eq!(response.status, 200);
    assert_eq!(revoked["revoked_at"], "2026-11-03T12:01:30.000Z");
    assert_eq!(read_with(&db, secret).status, 401);
    db.now.set(db.now.get() + 90_000);
    let (_, again) = owner(&db, Method::Post, &path, None);
    assert_eq!(
        again["revoked_at"], revoked["revoked_at"],
        "the first revocation stands"
    );
}

#[test]
fn the_list_gives_each_keys_label_times_quotas_and_tier_without_its_secret() {
    let db = Sqlite::new();
    issue(&db, json!({"label": "one"}));
    issue(&db, json!({"label": "two", "tier": "basic"}));
    let (response, list) = owner(&db, Method::Get, "/v1/owner/keys", None);
    assert_eq!(response.status, 200);
    let keys = list["keys"].as_array().unwrap();
    assert_eq!(keys.len(), 2);
    for key in keys {
        for field in ["id", "label", "issued_at", "quotas", "tier"] {
            assert!(!key[field].is_null(), "{field}");
        }
        assert_eq!(key["revoked_at"], Value::Null);
        assert!(key.get("secret").is_none());
    }
    assert_eq!(keys[1]["tier"], "basic");
}

#[test]
fn an_owner_operation_without_the_owner_secret_answers_401_and_changes_nothing() {
    let db = Sqlite::new();
    let issued = issue(&db, json!({"label": "web app"}));
    let key = issued["secret"].as_str().unwrap();
    let id = issued["id"].as_str().unwrap();
    let before = stored(&db);
    for bearer in [None, Some("wrong"), Some(key)] {
        for (method, path, json) in [
            (
                Method::Post,
                "/v1/owner/keys".to_owned(),
                Some(json!({"label": "x"})),
            ),
            (Method::Get, "/v1/owner/keys".to_owned(), None),
            (
                Method::Patch,
                format!("/v1/owner/keys/{id}"),
                Some(json!({"label": "y"})),
            ),
            (Method::Post, format!("/v1/owner/keys/{id}/revoke"), None),
            (Method::Get, "/v1/owner/nowhere".to_owned(), None),
        ] {
            let response = db.answer(&request(method, &path, bearer, json));
            assert_eq!(response.status, 401, "{path} with {bearer:?}");
            assert_eq!(body(&response)["type"], "urn:univ:problem:unauthorized");
        }
    }
    assert_eq!(stored(&db), before);
    let revoked: Option<String> = db
        .layers
        .query_row("SELECT revoked_at FROM keys", [], |r| r.get(0))
        .unwrap();
    assert_eq!(revoked, None);
}

#[test]
fn a_worker_given_no_owner_secret_refuses_every_owner_operation() {
    let mut db = Sqlite::new();
    db.owner = None;
    let response = db.answer(&request(
        Method::Get,
        "/v1/owner/keys",
        Some(OWNER_SECRET),
        None,
    ));
    assert_eq!(response.status, 401);
}

#[test]
fn a_key_is_read_from_the_authorization_header_as_a_bearer_token() {
    let db = Sqlite::new();
    let secret = issue(&db, json!({"label": "web app"}))["secret"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut request = Request::new(Method::Get, "/v1/openapi.json");
    request
        .headers
        .push(("authorization".into(), format!("bearer {secret}")));
    assert_eq!(db.answer(&request).status, 200);
    let mut basic = Request::new(Method::Get, "/v1/openapi.json");
    basic
        .headers
        .push(("authorization".into(), format!("Basic {secret}")));
    let refused = db.answer(&basic);
    assert_eq!(refused.status, 401);
    assert_eq!(refused.header("www-authenticate"), Some("Bearer"));
}

#[test]
fn a_key_is_checked_against_the_sha256_stored_at_its_issuance() {
    let db = Sqlite::new();
    let secret = issue(&db, json!({"label": "web app"}))["secret"]
        .as_str()
        .unwrap()
        .to_owned();
    db.layers
        .execute(
            "UPDATE keys SET secret_sha256 = ?1",
            [crate::access::sha256_hex("another")],
        )
        .unwrap();
    assert_eq!(read_with(&db, &secret).status, 401);
    assert_eq!(read_with(&db, "another").status, 200);
}

#[test]
fn an_unknown_key_and_a_revoked_key_are_refused_alike() {
    let db = Sqlite::new();
    let issued = issue(&db, json!({"label": "web app"}));
    let secret = issued["secret"].as_str().unwrap();
    let path = format!("/v1/owner/keys/{}/revoke", issued["id"].as_str().unwrap());
    owner(&db, Method::Post, &path, None);
    let revoked = read_with(&db, secret);
    let unknown = read_with(&db, "univ_0000");
    assert_eq!(revoked.status, 401);
    assert_eq!(revoked.status, unknown.status);
    assert_eq!(body(&revoked), body(&unknown));
    assert_eq!(revoked.headers, unknown.headers);
}

#[test]
fn a_key_changed_applies_from_its_next_request_and_an_unknown_key_answers_404() {
    let db = Sqlite::new();
    let id = issue(&db, json!({"label": "web app"}))["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (response, changed) = owner(
        &db,
        Method::Patch,
        &format!("/v1/owner/keys/{id}"),
        Some(json!({"label": "renamed", "quotas": {"items_per_layer": 10}, "tier": "extended"})),
    );
    assert_eq!(response.status, 200, "{changed}");
    assert_eq!(changed["label"], "renamed");
    assert_eq!(changed["quotas"]["items_per_layer"], 10);
    assert_eq!(changed["quotas"]["layers_per_key"], 100);
    assert_eq!(changed["tier"], "extended");
    let (response, problem) = owner(
        &db,
        Method::Patch,
        "/v1/owner/keys/key_none",
        Some(json!({})),
    );
    assert_eq!(response.status, 404);
    assert_eq!(problem["type"], "urn:univ:problem:key-not-found");
}

#[test]
fn a_body_that_is_not_a_key_to_issue_answers_400_and_stores_nothing() {
    let db = Sqlite::new();
    for json in [
        json!({}),
        json!({"label": "x", "tier": "huge"}),
        json!({"label": "x", "colour": 1}),
    ] {
        let (response, problem) = owner(&db, Method::Post, "/v1/owner/keys", Some(json));
        assert_eq!(response.status, 400, "{problem}");
        assert_eq!(problem["type"], "urn:univ:problem:invalid-body");
    }
    assert_eq!(stored(&db).len(), 0);
}
