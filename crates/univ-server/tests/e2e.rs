//! End-to-end tests of the Worker inside `wrangler dev`, over the fixture state. They are
//! ignored so that `cargo test` skips them; `cargo xtask integration` starts the Worker and runs
//! them with `UNIV_E2E_URL` naming it.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test helpers unwrap: a failure there is the test failing"
)]

use reqwest::blocking::{Client, Response};
use reqwest::{Method, StatusCode};
use serde_json::Value;

/// The Worker's base URL, as `cargo xtask integration` gives it.
fn base() -> String {
    std::env::var("UNIV_E2E_URL")
        .expect("UNIV_E2E_URL names the Worker: run `cargo xtask integration`")
}

fn client() -> Client {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
}

fn get(path: &str) -> Response {
    client().get(format!("{}{path}", base())).send().unwrap()
}

fn header<'a>(response: &'a Response, name: &str) -> Option<&'a str> {
    response
        .headers()
        .get(name)
        .map(|value| value.to_str().unwrap())
}

#[test]
#[ignore = "needs the Worker: run `cargo xtask integration`"]
fn a_read_with_no_authorization_header_succeeds_from_any_origin() {
    let response = client()
        .get(format!("{}/v1/openapi.json", base()))
        .header("Origin", "https://example.org")
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(header(&response, "access-control-allow-origin"), Some("*"));
    let document: Value = serde_json::from_slice(&response.bytes().unwrap()).unwrap();
    assert_eq!(document["openapi"], "3.1.0");
}

#[test]
#[ignore = "needs the Worker: run `cargo xtask integration`"]
fn a_cors_preflight_for_a_read_is_answered_without_credentials() {
    let response = client()
        .request(Method::OPTIONS, format!("{}/v1/openapi.json", base()))
        .header("Origin", "https://example.org")
        .header("Access-Control-Request-Method", "GET")
        .header("Access-Control-Request-Headers", "authorization")
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(header(&response, "access-control-allow-origin"), Some("*"));
    let allowed = header(&response, "access-control-allow-headers").unwrap();
    assert!(allowed.to_ascii_lowercase().contains("authorization"));
}

#[test]
#[ignore = "needs the Worker: run `cargo xtask integration`"]
fn an_unknown_route_answers_404_with_problem_details() {
    let response = get("/v1/nowhere");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        header(&response, "content-type"),
        Some("application/problem+json")
    );
    let problem: Value = serde_json::from_slice(&response.bytes().unwrap()).unwrap();
    assert_eq!(problem["type"], "urn:univ:problem:route-not-found");
    assert_eq!(problem["status"], 404);
    assert_eq!(problem["path"], "/v1/nowhere");
}
