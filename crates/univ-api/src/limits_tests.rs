//! Rate limits by client address and by key tier (enforce-quotas-and-rate-limits).

use serde_json::json;
use univ_model::api::keys::{RATE_WINDOW_SECONDS, RateLimitTier};

use crate::backend::Limiter;
use crate::http::{Method, Request, Response};
use crate::testkit::{Sqlite, body};

fn from(address: &str) -> Request {
    Request::new(Method::Get, "/v1/openapi.json").with_header("cf-connecting-ip", address)
}

/// A key of `tier`: its id and its secret.
fn key(db: &Sqlite, tier: &str) -> (String, String) {
    let issued = db.issue(&json!({"label": "k", "tier": tier}));
    let text = |field: &str| issued[field].as_str().unwrap().to_owned();
    (text("id"), text("secret"))
}

/// The number of `request`s answered before the first 429, and that 429.
fn until_refused(db: &Sqlite, request: &Request) -> (u32, Response) {
    for answered in 0..10_000 {
        let response = db.answer(request);
        if response.status == 429 {
            return (answered, response);
        }
        assert_ne!(response.status, 429);
    }
    panic!("never refused");
}

#[test]
fn anonymous_requests_are_limited_per_client_address_at_300_in_60_seconds() {
    assert_eq!(Limiter::Anonymous.requests(), 300);
    assert_eq!(RATE_WINDOW_SECONDS, 60);
    let db = Sqlite::new();
    let (answered, _) = until_refused(&db, &from("192.0.2.1"));
    assert_eq!(answered, 300);
    assert_eq!(db.answer(&from("192.0.2.2")).status, 200, "another address");
    assert!(
        db.counted
            .borrow()
            .contains_key(&(Limiter::Anonymous, "192.0.2.1".to_owned()))
    );
}

#[test]
fn the_tiers_are_basic_300_standard_1200_and_extended_6000_in_60_seconds() {
    assert_eq!(RateLimitTier::Basic.requests(), 300);
    assert_eq!(RateLimitTier::Standard.requests(), 1_200);
    assert_eq!(RateLimitTier::Extended.requests(), 6_000);
}

#[test]
fn requests_with_a_key_are_limited_per_key_at_its_tiers_rate() {
    let db = Sqlite::new();
    let (id, secret) = key(&db, "basic");
    let (answered, _) = until_refused(&db, &from("192.0.2.1").with_bearer(&secret));
    assert_eq!(answered, 300);
    assert!(
        db.counted
            .borrow()
            .contains_key(&(Limiter::Key(RateLimitTier::Basic), id))
    );
    assert_eq!(
        db.answer(&from("192.0.2.1")).status,
        200,
        "the address's own limit is apart from the key's"
    );
}

#[test]
fn over_the_limit_the_answer_is_429_with_retry_after() {
    let db = Sqlite::new();
    db.limits.borrow_mut().insert(Limiter::Anonymous, 2);
    let (_, refused) = until_refused(&db, &from("192.0.2.1"));
    assert_eq!(refused.header("retry-after"), Some("60"));
    assert_eq!(refused.header("access-control-allow-origin"), Some("*"));
    let problem = body(&refused);
    assert_eq!(problem["type"], "urn:univ:problem:rate-limited");
    assert_eq!(problem["limit"], 300);
    assert_eq!(problem["window_seconds"], 60);
}

#[test]
fn a_request_whose_credentials_fail_counts_against_its_addresss_limit() {
    let db = Sqlite::new();
    db.limits.borrow_mut().insert(Limiter::Anonymous, 3);
    let guess = |path: &str, token: &str| {
        db.answer(
            &Request::new(Method::Get, path)
                .with_header("cf-connecting-ip", "192.0.2.9")
                .with_bearer(token),
        )
    };
    assert_eq!(guess("/v1/owner/keys", "guess-1").status, 401);
    assert_eq!(guess("/v1/openapi.json", "univ_guess").status, 401);
    assert_eq!(guess("/v1/owner/keys", "guess-2").status, 401);
    let refused = guess("/v1/owner/keys", "guess-3");
    assert_eq!(refused.status, 429, "{}", body(&refused));
}

#[test]
fn a_key_moved_to_another_tier_is_limited_at_that_tiers_rate_from_its_next_request() {
    let db = Sqlite::new();
    let (id, secret) = key(&db, "standard");
    assert_eq!(
        db.answer(&from("192.0.2.1").with_bearer(&secret)).status,
        200
    );
    let (response, _) = db.as_owner(
        Request::new(Method::Patch, &format!("/v1/owner/keys/{id}"))
            .with_json(&json!({"tier": "extended"})),
    );
    assert_eq!(response.status, 200);
    assert_eq!(
        db.answer(&from("192.0.2.1").with_bearer(&secret)).status,
        200
    );
    let counted = db.counted.borrow();
    let count = |tier| counted.get(&(Limiter::Key(tier), id.clone()));
    assert_eq!(count(RateLimitTier::Standard), Some(&1));
    assert_eq!(count(RateLimitTier::Extended), Some(&1));
}
