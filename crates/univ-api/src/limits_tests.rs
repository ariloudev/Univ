//! Rate limits by client address and by key tier (enforce-quotas-and-rate-limits).

use serde_json::json;

use crate::backend::Limiter;
use crate::http::{Method, Request, Response};
use crate::testkit::{OWNER_SECRET, Sqlite, body};

fn from(address: &str, bearer: Option<&str>) -> Request {
    let mut request = Request::new(Method::Get, "/v1/openapi.json");
    request
        .headers
        .push(("cf-connecting-ip".into(), address.into()));
    if let Some(token) = bearer {
        request
            .headers
            .push(("authorization".into(), format!("Bearer {token}")));
    }
    request
}

fn issue(db: &Sqlite, tier: &str) -> (String, String) {
    let mut request = Request::new(Method::Post, "/v1/owner/keys");
    request
        .headers
        .push(("authorization".into(), format!("Bearer {OWNER_SECRET}")));
    request.body = json!({"label": "k", "tier": tier}).to_string().into_bytes();
    let issued = body(&db.answer(&request));
    (
        issued["id"].as_str().unwrap().to_owned(),
        issued["secret"].as_str().unwrap().to_owned(),
    )
}

/// The number of `request`s answered before the first 429, and that 429.
fn until_refused(db: &Sqlite, request: &Request) -> (u32, Response) {
    for answered in 0..10_000 {
        let response = db.answer(request);
        if response.status == 429 {
            return (answered, response);
        }
        assert_eq!(response.status, 200);
    }
    panic!("never refused");
}

#[test]
fn anonymous_requests_are_limited_per_client_address_at_300_in_60_seconds() {
    assert_eq!(Limiter::Anonymous.requests(), 300);
    assert_eq!(univ_model::api::keys::RATE_WINDOW_SECONDS, 60);
    let db = Sqlite::new();
    let (answered, _) = until_refused(&db, &from("192.0.2.1", None));
    assert_eq!(answered, 300);
    assert_eq!(
        db.answer(&from("192.0.2.2", None)).status,
        200,
        "another address"
    );
    assert!(
        db.counted
            .borrow()
            .contains_key(&(Limiter::Anonymous, "192.0.2.1".to_owned()))
    );
}

#[test]
fn the_tiers_are_basic_300_standard_1200_and_extended_6000_in_60_seconds() {
    assert_eq!(Limiter::Basic.requests(), 300);
    assert_eq!(Limiter::Standard.requests(), 1_200);
    assert_eq!(Limiter::Extended.requests(), 6_000);
}

#[test]
fn requests_with_a_key_are_limited_per_key_at_its_tiers_rate() {
    let db = Sqlite::new();
    let (id, secret) = issue(&db, "basic");
    let (answered, _) = until_refused(&db, &from("192.0.2.1", Some(&secret)));
    assert_eq!(answered, 300);
    assert!(db.counted.borrow().contains_key(&(Limiter::Basic, id)));
    assert_eq!(
        db.answer(&from("192.0.2.1", None)).status,
        200,
        "the address's own limit is apart from the key's"
    );
}

#[test]
fn over_the_limit_the_answer_is_429_with_retry_after() {
    let db = Sqlite::new();
    db.limits.borrow_mut().insert(Limiter::Anonymous, 2);
    let (_, refused) = until_refused(&db, &from("192.0.2.1", None));
    assert_eq!(refused.header("retry-after"), Some("60"));
    let problem = body(&refused);
    assert_eq!(problem["type"], "urn:univ:problem:rate-limited");
    assert_eq!(problem["limit"], 300);
    assert_eq!(problem["window_seconds"], 60);
}

#[test]
fn a_key_moved_to_another_tier_is_limited_at_that_tiers_rate_from_its_next_request() {
    let db = Sqlite::new();
    let (id, secret) = issue(&db, "standard");
    assert_eq!(db.answer(&from("192.0.2.1", Some(&secret))).status, 200);
    let mut change = Request::new(Method::Patch, &format!("/v1/owner/keys/{id}"));
    change
        .headers
        .push(("authorization".into(), format!("Bearer {OWNER_SECRET}")));
    change.body = json!({"tier": "extended"}).to_string().into_bytes();
    assert_eq!(db.answer(&change).status, 200);
    assert_eq!(db.answer(&from("192.0.2.1", Some(&secret))).status, 200);
    let counted = db.counted.borrow();
    assert_eq!(counted.get(&(Limiter::Standard, id.clone())), Some(&1));
    assert_eq!(counted.get(&(Limiter::Extended, id)), Some(&1));
}
