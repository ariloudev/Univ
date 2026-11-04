//! End-to-end tests of keys, the owner's operations and rate limits through the Worker under
//! `wrangler dev`. Run them with `cargo xtask integration`, which starts the Worker with an
//! owner secret and names both.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

mod common;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use common::{CLIENT, header, issue, json, owner, url};
use reqwest::StatusCode;
use reqwest::blocking::Response;
use serde_json::json;

fn read_with(secret: &str) -> Response {
    CLIENT
        .get(url("/v1/releases"))
        .bearer_auth(secret)
        .send()
        .unwrap()
}

#[test]
#[ignore = "needs the Worker: run `cargo xtask integration`"]
fn a_key_is_issued_read_with_moved_between_tiers_and_revoked() {
    let (id, secret) = issue("e2e lifecycle", "standard");
    let (_, other) = issue("e2e other", "standard");
    assert_ne!(secret, other, "each key draws its own random bits");
    assert_eq!(read_with(&secret).status(), StatusCode::OK);
    let listed = json(owner(CLIENT.get(url("/v1/owner/keys"))).send().unwrap());
    let key = listed["keys"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["id"] == id.as_str())
        .unwrap();
    assert_eq!(key["tier"], "standard");
    assert!(!listed.to_string().contains(&secret), "no secret is listed");
    let changed = owner(CLIENT.patch(url(&format!("/v1/owner/keys/{id}"))))
        .header("content-type", "application/json")
        .body(json!({"tier": "extended"}).to_string())
        .send()
        .unwrap();
    assert_eq!(json(changed)["tier"], "extended");
    assert_eq!(read_with(&secret).status(), StatusCode::OK);
    let revoked = owner(CLIENT.post(url(&format!("/v1/owner/keys/{id}/revoke"))))
        .send()
        .unwrap();
    assert!(json(revoked)["revoked_at"].is_string());
    let refused = read_with(&secret);
    assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);
    let unknown = read_with("univ_unknown");
    assert_eq!(unknown.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        json(refused),
        json(unknown),
        "revoked and unknown read alike"
    );
}

#[test]
#[ignore = "needs the Worker: run `cargo xtask integration`"]
fn the_owners_operations_are_refused_without_the_owner_secret() {
    for request in [
        CLIENT.get(url("/v1/owner/keys")),
        CLIENT.get(url("/v1/owner/keys")).bearer_auth("wrong"),
        CLIENT
            .post(url("/v1/owner/keys"))
            .header("content-type", "application/json")
            .body(json!({"label": "x"}).to_string()),
    ] {
        let response = request.send().unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(json(response)["type"], "urn:univ:problem:unauthorized");
    }
}

#[test]
#[ignore = "needs the Worker: run `cargo xtask integration`"]
fn a_basic_key_is_refused_with_429_and_retry_after_past_300_requests_in_a_minute() {
    let (_, secret) = issue("e2e basic", "basic");
    within_one_window();
    let mut answered = 0;
    let refused = loop {
        let response = CLIENT
            .get(url("/v1/openapi.json"))
            .bearer_auth(&secret)
            .send()
            .unwrap();
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            break response;
        }
        assert_eq!(response.status(), StatusCode::OK);
        answered += 1;
        assert!(
            answered <= 300,
            "a basic key allows 300 requests in 60 seconds"
        );
    };
    assert_eq!(answered, 300);
    assert_eq!(header(&refused, "retry-after"), Some("60"));
    assert_eq!(json(refused)["type"], "urn:univ:problem:rate-limited");
}

/// Waits for a new window when too little of this one is left for the 301 requests: the
/// local limiter's windows start at whole minutes of the wall clock.
fn within_one_window() {
    let into = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        % 60;
    if into > 40 {
        std::thread::sleep(Duration::from_secs(61 - into));
    }
}
