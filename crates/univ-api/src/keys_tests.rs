//! The owner's key operations and API key authentication (issue-and-revoke-keys,
//! write-with-an-api-key, enforce-quotas-and-rate-limits), on the test kit's univ-layers
//! database.

use serde_json::{Value, json};

use crate::http::{Method, Request, Response};
use crate::testkit::{Sqlite, body};

fn read_with(db: &Sqlite, secret: &str) -> Response {
    db.answer(&Request::new(Method::Get, "/v1/openapi.json").with_bearer(secret))
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

fn text(issued: &Value, field: &str) -> String {
    issued[field].as_str().unwrap().to_owned()
}

#[test]
fn an_issued_key_holds_256_random_bits_and_only_its_sha256_is_stored() {
    let db = Sqlite::new();
    let secret = text(&db.issue(&json!({"label": "web app"})), "secret");
    let hex = secret.strip_prefix("univ_").unwrap();
    assert_eq!(hex.len(), 64, "32 bytes, 256 bits");
    assert!(hex.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_ne!(
        secret,
        text(&db.issue(&json!({"label": "other"})), "secret"),
        "each key draws its own bits"
    );
    let rows = stored(&db);
    assert!(rows.contains(&(crate::access::sha256_hex(&secret), "web app".to_owned())));
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
    let issued = db.issue(&json!({"label": "web app"}));
    let secret = text(&issued, "secret");
    let path = format!("/v1/owner/keys/{}", text(&issued, "id"));
    let later = [
        db.as_owner(Request::new(Method::Get, "/v1/owner/keys")),
        db.as_owner(Request::new(Method::Patch, &path).with_json(&json!({"tier": "basic"}))),
        db.as_owner(Request::new(Method::Post, &format!("{path}/revoke"))),
    ];
    for (response, document) in later {
        assert_eq!(response.status, 200, "{document}");
        assert_eq!(response.header("cache-control"), Some("no-store"));
        assert!(!document.to_string().contains(&secret), "{document}");
        assert!(!document.to_string().contains("secret"), "{document}");
    }
}

#[test]
fn a_key_issued_without_quotas_or_tier_gets_the_default_quotas_and_the_standard_tier() {
    let db = Sqlite::new();
    let issued = db.issue(&json!({"label": "plain"}));
    assert_eq!(
        issued["quotas"],
        json!({"layers_per_key": 100, "items_per_layer": 100_000, "bytes_per_item_version": 65_536})
    );
    assert_eq!(issued["tier"], "standard");
    let chosen =
        db.issue(&json!({"label": "big", "quotas": {"layers_per_key": 5}, "tier": "extended"}));
    assert_eq!(chosen["quotas"]["layers_per_key"], 5);
    assert_eq!(chosen["quotas"]["items_per_layer"], 100_000);
    assert_eq!(chosen["tier"], "extended");
}

#[test]
fn a_quota_above_2_pow_53_minus_1_is_refused_and_stores_nothing() {
    let db = Sqlite::new();
    let id = text(&db.issue(&json!({"label": "k"})), "id");
    let huge = json!({"quotas": {"bytes_per_item_version": 1_u64 << 53}});
    let (response, problem) = db.as_owner(
        Request::new(Method::Post, "/v1/owner/keys")
            .with_json(&json!({"label": "x", "quotas": huge["quotas"]})),
    );
    assert_eq!(response.status, 400, "{problem}");
    assert_eq!(problem["quota"], "bytes_per_item_version");
    let (response, _) =
        db.as_owner(Request::new(Method::Patch, &format!("/v1/owner/keys/{id}")).with_json(&huge));
    assert_eq!(response.status, 400);
    assert_eq!(stored(&db).len(), 1);
    let (_, list) = db.as_owner(Request::new(Method::Get, "/v1/owner/keys"));
    assert_eq!(list["keys"][0]["quotas"]["bytes_per_item_version"], 65_536);
}

#[test]
fn a_revocation_is_recorded_with_its_time_and_the_key_is_refused_from_then_on() {
    let db = Sqlite::new();
    let issued = db.issue(&json!({"label": "web app"}));
    let secret = text(&issued, "secret");
    assert_eq!(read_with(&db, &secret).status, 200);
    db.now.set(db.now.get() + 90_000);
    let revoke = || {
        db.as_owner(Request::new(
            Method::Post,
            &format!("/v1/owner/keys/{}/revoke", text(&issued, "id")),
        ))
    };
    let (response, revoked) = revoke();
    assert_eq!(response.status, 200);
    assert_eq!(revoked["revoked_at"], "2026-11-03T12:01:30.000Z");
    assert_eq!(read_with(&db, &secret).status, 401);
    db.now.set(db.now.get() + 90_000);
    assert_eq!(
        revoke().1["revoked_at"],
        revoked["revoked_at"],
        "the first revocation stands"
    );
    let (response, _) = db.as_owner(Request::new(Method::Post, "/v1/owner/keys/key_none/revoke"));
    assert_eq!(response.status, 404);
}

#[test]
fn the_list_gives_each_keys_label_times_quotas_and_tier_without_its_secret() {
    let db = Sqlite::new();
    db.issue(&json!({"label": "one"}));
    db.issue(&json!({"label": "two", "tier": "basic"}));
    let (response, list) = db.as_owner(Request::new(Method::Get, "/v1/owner/keys"));
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
    let issued = db.issue(&json!({"label": "web app"}));
    let key = text(&issued, "secret");
    let id = text(&issued, "id");
    let before = stored(&db);
    for bearer in [None, Some("wrong"), Some(key.as_str())] {
        for request in [
            Request::new(Method::Post, "/v1/owner/keys").with_json(&json!({"label": "x"})),
            Request::new(Method::Get, "/v1/owner/keys"),
            Request::new(Method::Patch, &format!("/v1/owner/keys/{id}"))
                .with_json(&json!({"label": "y"})),
            Request::new(Method::Post, &format!("/v1/owner/keys/{id}/revoke")),
            Request::new(Method::Get, "/v1/owner/nowhere"),
        ] {
            let path = request.path.clone();
            let request = match bearer {
                Some(token) => request.with_bearer(token),
                None => request,
            };
            let response = db.answer(&request);
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
    let (response, _) = db.as_owner(Request::new(Method::Get, "/v1/owner/keys"));
    assert_eq!(response.status, 401);
}

#[test]
fn a_key_is_read_from_the_authorization_header_as_a_bearer_token() {
    let db = Sqlite::new();
    let secret = text(&db.issue(&json!({"label": "web app"})), "secret");
    let lower = Request::new(Method::Get, "/v1/openapi.json")
        .with_header("authorization", &format!("bearer {secret}"));
    assert_eq!(db.answer(&lower).status, 200);
    let basic = Request::new(Method::Get, "/v1/openapi.json")
        .with_header("authorization", &format!("Basic {secret}"));
    let refused = db.answer(&basic);
    assert_eq!(refused.status, 401);
    assert_eq!(refused.header("www-authenticate"), Some("Bearer"));
}

#[test]
fn a_key_is_checked_against_the_sha256_stored_at_its_issuance() {
    let db = Sqlite::new();
    let secret = text(&db.issue(&json!({"label": "web app"})), "secret");
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
    let issued = db.issue(&json!({"label": "web app"}));
    let path = format!("/v1/owner/keys/{}/revoke", text(&issued, "id"));
    db.as_owner(Request::new(Method::Post, &path));
    let revoked = read_with(&db, &text(&issued, "secret"));
    let unknown = read_with(&db, "univ_0000");
    assert_eq!(revoked.status, 401);
    assert_eq!(revoked.status, unknown.status);
    assert_eq!(body(&revoked), body(&unknown));
    assert_eq!(revoked.headers, unknown.headers);
}

#[test]
fn a_key_changed_applies_from_its_next_request_and_an_unknown_key_answers_404() {
    let db = Sqlite::new();
    let id = text(&db.issue(&json!({"label": "web app"})), "id");
    let change = json!({"label": "renamed", "quotas": {"items_per_layer": 10}, "tier": "extended"});
    let (response, changed) = db
        .as_owner(Request::new(Method::Patch, &format!("/v1/owner/keys/{id}")).with_json(&change));
    assert_eq!(response.status, 200, "{changed}");
    assert_eq!(changed["label"], "renamed");
    assert_eq!(changed["quotas"]["items_per_layer"], 10);
    assert_eq!(changed["quotas"]["layers_per_key"], 100);
    assert_eq!(changed["tier"], "extended");
    let (response, problem) =
        db.as_owner(Request::new(Method::Patch, "/v1/owner/keys/key_none").with_json(&json!({})));
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
        let (response, problem) =
            db.as_owner(Request::new(Method::Post, "/v1/owner/keys").with_json(&json));
        assert_eq!(response.status, 400, "{problem}");
        assert_eq!(problem["type"], "urn:univ:problem:invalid-body");
    }
    assert_eq!(stored(&db).len(), 0);
}

#[test]
fn the_test_kit_applies_every_migration_wrangler_does() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    let files = std::fs::read_dir(dir).unwrap().count();
    assert_eq!(crate::MIGRATIONS.len(), files);
}
